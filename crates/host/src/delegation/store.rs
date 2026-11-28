//! Which session opened which and what sessions told each other, kept by the session
//! daemon so both outlive the window and the sessions.
use adroit_sys::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::fs::{DirBuilder, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

use adroit_model::profile::ProfileId;
use adroit_model::timeline::Entry;
use adroit_model::{
    Agent, AgentPermissions, Relay, RelayTo, Requester, SessionCode, TerminalSession,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::{Code, Error, Kind, Message, MessageId, Readiness, Result, Sender, Session, SessionId};
use crate::settings::Settings;

const SCHEMA_VERSION: i64 = 12;

/// The tables as the first schema made them; a new database is made by it
/// and then by every step in `MIGRATIONS`, so it ends as one carried forward.
const SCHEMA: &str = "
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    parent TEXT REFERENCES sessions(id),
    agent TEXT NOT NULL,
    name TEXT NOT NULL,
    cwd TEXT NOT NULL,
    launch TEXT NOT NULL,
    terminal TEXT,
    conversation TEXT,
    readiness TEXT NOT NULL,
    started_ms INTEGER NOT NULL,
    reported INTEGER NOT NULL DEFAULT 0,
    ended INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX sessions_by_parent ON sessions(parent);
CREATE TABLE messages (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    sender TEXT REFERENCES sessions(id),
    recipient TEXT NOT NULL REFERENCES sessions(id),
    kind TEXT NOT NULL,
    body TEXT NOT NULL,
    sent_ms INTEGER NOT NULL,
    delivered INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX undelivered ON messages(recipient, delivered);
";

/// What carries a database from each schema to the next, the first from 1 to 2.
const MIGRATIONS: &[&str] = &[
    // Sessions from before ran in their agent's own environment, which its
    // built-in profile and implicit connection, both named by its key, stand for.
    "ALTER TABLE sessions ADD COLUMN profile TEXT NOT NULL DEFAULT '';
     ALTER TABLE sessions ADD COLUMN connection TEXT NOT NULL DEFAULT '';
     UPDATE sessions SET profile = agent, connection = agent;",
    // A session that ran in a connection of its own keeps the folder the
    // connection named, as the settings it was launched with define it: once
    // profiles own their logins, a connection's id names nothing.
    "ALTER TABLE sessions ADD COLUMN home TEXT;
     UPDATE sessions SET home = (
         SELECT json_extract(c.value, '$.home')
         FROM json_each(sessions.launch, '$.connections') AS c
         WHERE json_extract(c.value, '$.id') = sessions.connection
     ) WHERE connection <> agent AND json_valid(launch);
     ALTER TABLE sessions DROP COLUMN connection;",
    // Sessions no longer report to the one that opened them: a report was a
    // message, and adroit's notices of how an opened session stood were
    // meant for a session waiting on it, which none does any more.
    "UPDATE messages SET kind = 'message' WHERE kind = 'report';
     DELETE FROM messages WHERE kind = 'notice';
     ALTER TABLE sessions DROP COLUMN reported;",
    // A session or a terminal the terminal host took with it is told from
    // one that ended by the terminal host it ran in, and keeps what it is
    // brought back with. Those from before ran in none Adroit knows.
    "ALTER TABLE sessions ADD COLUMN generation TEXT;
     ALTER TABLE sessions ADD COLUMN interrupted INTEGER NOT NULL DEFAULT 0;
     ALTER TABLE sessions ADD COLUMN brought_back_by TEXT;
     ALTER TABLE sessions ADD COLUMN model TEXT;
     ALTER TABLE sessions ADD COLUMN permissions TEXT;
     CREATE TABLE terminals (
         id TEXT PRIMARY KEY,
         resource_id TEXT NOT NULL,
         generation TEXT,
         name TEXT NOT NULL,
         cwd TEXT NOT NULL,
         program TEXT NOT NULL,
         file TEXT,
         command TEXT NOT NULL,
         started_ms INTEGER NOT NULL,
         ended INTEGER NOT NULL DEFAULT 0,
         interrupted INTEGER NOT NULL DEFAULT 0
     );",
    // A name a person gave a session is kept from its agent's renaming;
    // every name from before was made from the prompt or by a session.
    "ALTER TABLE sessions ADD COLUMN name_given INTEGER NOT NULL DEFAULT 0;",
    // Every session keeps a code; those from before are given theirs once
    // the step has run, by `fill_codes`, since SQL draws no code.
    "ALTER TABLE sessions ADD COLUMN code TEXT;
     CREATE INDEX sessions_by_code ON sessions(code);",
    // What each session's agent recorded of its work, by the code that the
    // session keeps when it is brought back.
    "CREATE TABLE timeline (
         seq INTEGER PRIMARY KEY AUTOINCREMENT,
         code TEXT NOT NULL,
         at_ms INTEGER NOT NULL,
         ops TEXT NOT NULL
     );
     CREATE INDEX timeline_by_code ON timeline(code);",
    // A program outside Adroit that asks a session for work is kept by the
    // name it gave itself; every message from before is a session's.
    "ALTER TABLE messages ADD COLUMN outside TEXT;",
    // Each change of how a session stands, for programs outside Adroit to
    // follow.
    "CREATE TABLE standings (
         seq INTEGER PRIMARY KEY AUTOINCREMENT,
         session TEXT NOT NULL,
         at_ms INTEGER NOT NULL,
         body TEXT NOT NULL
     );
     CREATE INDEX standings_by_session ON standings(session);",
    // Whom the user let, or no longer lets, ask which session for work, by
    // the codes sessions keep when they are brought back.
    "CREATE TABLE requesters (
         sender TEXT NOT NULL,
         recipient TEXT NOT NULL,
         taken_up INTEGER NOT NULL,
         PRIMARY KEY (sender, recipient)
     );",
    // The user's words sessions propose to pass on, to the codes of those
    // they are to reach, and what the user decided of them; and when a
    // message was handed over, which tells what its recipient was told last
    // beside when the user last wrote to it.
    "CREATE TABLE relays (
         id TEXT PRIMARY KEY,
         proposer TEXT NOT NULL REFERENCES sessions(id),
         recipients TEXT NOT NULL,
         body TEXT NOT NULL,
         proposed_ms INTEGER NOT NULL,
         decision TEXT,
         decided_ms INTEGER
     );
     ALTER TABLE messages ADD COLUMN delivered_ms INTEGER;
     ALTER TABLE sessions ADD COLUMN wrote_ms INTEGER;",
];

/// The most changes of how sessions stand kept: the latest.
const STANDINGS: i64 = 10_000;

/// The longest name a program outside Adroit goes by, in characters.
const OUTSIDE_NAME: usize = 64;
/// The longest request a program outside Adroit makes, in bytes: escaped and
/// enveloped, it still fits the frame it is handed over in.
const OUTSIDE_BODY: usize = 64 * 1024;

/// The longest words proposed to be passed on, in bytes, for the reason
/// [`OUTSIDE_BODY`] is, and the most sessions they are to reach.
const RELAY_BODY: usize = 64 * 1024;
const RELAY_TO: usize = 32;
/// How many of a session's proposals wait for the user at once.
const RELAYS_WAITING: i64 = 3;

/// What came of passing the user's words on: the sessions they were sent to
/// and those that had ended, by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passed {
    pub sent: Vec<String>,
    pub missed: Vec<String>,
}

impl Passed {
    /// What the user is told of it.
    pub fn said(&self) -> String {
        let sent = format!("Passed your words on to {}", self.sent.join(", "));
        match self.missed.as_slice() {
            [] => sent,
            missed => format!("{sent}; not to {}, ended meanwhile", missed.join(", ")),
        }
    }
}

/// Whether `c` would not be seen, or would change how what is around it is
/// read: words approved are to be the words the user saw.
fn hidden(c: char) -> bool {
    (c.is_control() && c != '\n' && c != '\t')
        || matches!(c,
            '\u{ad}' | '\u{61c}' | '\u{180e}' | '\u{200b}' | '\u{200e}' | '\u{200f}'
            | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}'
            | '\u{e0000}'..='\u{e007f}')
}

/// What the window or the work host knows of a session before its program starts.
#[derive(Debug, Clone)]
pub struct NewSession {
    pub parent: Option<SessionId>,
    pub agent: Agent,
    pub profile: ProfileId,
    pub home: Option<PathBuf>,
    pub name: String,
    pub cwd: PathBuf,
    pub launch: Settings,
    /// The model asked for, none for the profile's own.
    pub model: Option<String>,
    pub permissions: AgentPermissions,
    pub started_ms: u64,
    /// Whether a person gave its name, which its agent then never changes.
    pub name_given: bool,
    /// The code it had before, when it is brought back or resumes a
    /// conversation; none to be given a code no session has had.
    pub code: Option<SessionCode>,
}

/// A terminal as Adroit keeps it, with the terminal host that holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptTerminal {
    pub terminal: TerminalSession,
    pub generation: Option<String>,
    pub started_ms: u64,
}

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let created = private_database(path)?;
        // SQLite answers a change of journal mode that another connection
        // holds up with "database is locked" at once, without waiting as
        // busy_timeout has it wait for a write: so those setting a database
        // up go one at a time.
        let _setting_up = setting_up(path)?;
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "journal_mode", "wal")?;
        if schema(&connection)? == 0 {
            let tables: i64 = connection.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )?;
            if !created && tables != 0 {
                return Err(Error::new(
                    Code::Storage,
                    "the database holds no recognised delegation schema",
                ));
            }
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(SCHEMA)?;
            transaction.pragma_update(None, "user_version", 1)?;
            transaction.commit()?;
        }
        migrate(&mut connection)?;
        fill_codes(&mut connection)?;
        Ok(Self { connection })
    }

    /// Fails once a newer adroit has upgraded the database this one opened.
    pub fn ensure_current_schema(&self) -> Result<()> {
        schema(&self.connection).map(drop)
    }

    /// Records a session and returns the identity and credential it acts with.
    pub fn register(&mut self, new: NewSession) -> Result<(SessionId, String)> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(parent) = new.parent
            && session(&transaction, parent)?
                .ok_or_else(|| Error::new(Code::NotFound, "the opening session is unknown"))?
                .ended
        {
            return Err(Error::new(Code::Refused, "the opening session has ended"));
        }
        let code = match new.code {
            Some(code) => code,
            None => draw_code(&transaction, SessionCode::random)?,
        };
        let id = SessionId::new();
        let token = format!("{}{}", SessionId::new(), SessionId::new());
        transaction.execute(
            "INSERT INTO sessions (id, token, parent, agent, profile, home, name, cwd, launch, readiness, started_ms, model, permissions, name_given, code)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                id.to_string(),
                token,
                new.parent.map(|parent| parent.to_string()),
                new.agent.key(),
                new.profile.as_str(),
                new.home.as_ref().map(|home| home.to_string_lossy().into_owned()),
                new.name,
                new.cwd.to_string_lossy(),
                serde_json::to_string(&new.launch).map_err(Error::storage)?,
                Readiness::Starting.key(),
                new.started_ms as i64,
                new.model,
                serde_json::to_string(&new.permissions).map_err(Error::storage)?,
                new.name_given,
                code.as_str(),
            ],
        )?;
        transaction.commit()?;
        Ok((id, token))
    }

    pub fn authenticate(&self, token: &str) -> Result<SessionId> {
        self.connection
            .query_row(
                "SELECT id FROM sessions WHERE token = ?1 AND ended = 0",
                [token],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .and_then(|id| id.parse().ok())
            .ok_or_else(|| Error::new(Code::Refused, "the session credential is not valid"))
    }

    pub fn session(&self, id: SessionId) -> Result<Option<Session>> {
        session(&self.connection, id)
    }

    /// The session recorded as `id`, refusing one that never was.
    pub fn known(&self, id: SessionId) -> Result<Session> {
        known(&self.connection, id)
    }

    /// Every session, ended ones included, in the order they were registered.
    pub fn sessions(&self) -> Result<Vec<Session>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {SESSION_COLUMNS} FROM sessions ORDER BY rowid"
        ))?;
        let rows = statement.query_map([], read_session)?;
        rows.map(|row| row.map_err(Error::from).and_then(|row| row))
            .collect()
    }

    /// A code `draw` gives that no session has had.
    pub fn draw_code(&self, draw: impl FnMut() -> SessionCode) -> Result<SessionCode> {
        draw_code(&self.connection, draw)
    }

    /// The record standing for the session with `code` now: the newest one
    /// holding it, since one brought back or resumed takes on the code.
    pub fn by_code(&self, code: &SessionCode) -> Result<Option<Session>> {
        by_code(&self.connection, code)
    }

    /// The code of the session that last held `conversation`, for a session
    /// that resumes it.
    pub fn code_of_conversation(&self, conversation: &str) -> Result<Option<SessionCode>> {
        self.connection
            .query_row(
                "SELECT code FROM sessions WHERE conversation = ?1 ORDER BY rowid DESC LIMIT 1",
                [conversation],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten()
            .map(|code| code.parse().map_err(Error::storage))
            .transpose()
    }

    /// The settings `id` was launched with, which its later deliveries follow.
    pub fn launch(&self, id: SessionId) -> Result<Option<Settings>> {
        self.connection
            .query_row(
                "SELECT launch FROM sessions WHERE id = ?1",
                [id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|launch| serde_json::from_str(&launch).map_err(Error::storage))
            .transpose()
    }

    /// Records the terminal `id` runs in, and the terminal host that holds it.
    pub fn attach(
        &mut self,
        id: SessionId,
        terminal: u128,
        generation: Option<String>,
    ) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE sessions SET terminal = ?2, generation = ?3 WHERE id = ?1",
            params![id.to_string(), crate::sessions::hex(terminal), generation],
        )?;
        if changed == 0 {
            return Err(Error::new(Code::NotFound, "the session is unknown"));
        }
        Ok(())
    }

    /// Names `id` anew, as given by a person or not.
    pub fn rename(&mut self, id: SessionId, name: &str, given: bool) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE sessions SET name = ?2, name_given = ?3 WHERE id = ?1",
            params![id.to_string(), name, given],
        )?;
        if changed == 0 {
            return Err(Error::new(Code::NotFound, "the session is unknown"));
        }
        Ok(())
    }

    /// Names anew, as given by a person or not, the session running in
    /// `terminal`, if one is recorded there.
    pub fn rename_terminal(&mut self, terminal: u128, name: &str, given: bool) -> Result<()> {
        self.connection.execute(
            "UPDATE sessions SET name = ?2, name_given = ?3 WHERE terminal = ?1 AND ended = 0",
            params![crate::sessions::hex(terminal), name, given],
        )?;
        Ok(())
    }

    /// Lets the agent of `id` name it again: the name it has is kept, as one
    /// no person gave.
    pub fn hand_back_name(&mut self, id: SessionId) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE sessions SET name_given = 0 WHERE id = ?1",
            params![id.to_string()],
        )?;
        if changed == 0 {
            return Err(Error::new(Code::NotFound, "the session is unknown"));
        }
        Ok(())
    }

    /// [`Store::hand_back_name`] for the session running in `terminal`, if
    /// one is recorded there.
    pub fn hand_back_name_of_terminal(&mut self, terminal: u128) -> Result<()> {
        self.connection.execute(
            "UPDATE sessions SET name_given = 0 WHERE terminal = ?1 AND ended = 0",
            params![crate::sessions::hex(terminal)],
        )?;
        Ok(())
    }

    pub fn learn_conversation(&mut self, id: SessionId, conversation: &str) -> Result<()> {
        self.update(
            id,
            "UPDATE sessions SET conversation = ?2 WHERE id = ?1",
            conversation.to_owned(),
        )
    }

    /// Records how `id` stands while it starts, and returns whether that changed.
    pub fn set_readiness(&mut self, id: SessionId, readiness: Readiness) -> Result<bool> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = known(&transaction, id)?;
        if current.readiness == readiness || current.ended {
            return Ok(false);
        }
        transaction.execute(
            "UPDATE sessions SET readiness = ?2 WHERE id = ?1",
            params![id.to_string(), readiness.key()],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn end(&mut self, id: SessionId) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = known(&transaction, id)?;
        if current.ended {
            return Ok(());
        }
        transaction.execute(
            "UPDATE sessions SET ended = 1 WHERE id = ?1",
            [id.to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Records that `id` ended with the terminal host it ran in, to be
    /// brought back; one that had ended already stays as it ended.
    pub fn interrupt(&mut self, id: SessionId) -> Result<()> {
        known(&self.connection, id)?;
        self.connection.execute(
            "UPDATE sessions SET ended = 1, interrupted = 1 WHERE id = ?1 AND ended = 0",
            [id.to_string()],
        )?;
        Ok(())
    }

    /// The sessions interrupted and neither brought back nor let go, in the
    /// order they were registered.
    pub fn interrupted(&self) -> Result<Vec<Session>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {SESSION_COLUMNS} FROM sessions WHERE interrupted = 1 ORDER BY rowid"
        ))?;
        let rows = statement.query_map([], read_session)?;
        rows.map(|row| row.map_err(Error::from).and_then(|row| row))
            .collect()
    }

    /// Records that the interrupted `id` was brought back as `by`.
    pub fn brought_back(&mut self, id: SessionId, by: SessionId) -> Result<()> {
        self.update(
            id,
            "UPDATE sessions SET interrupted = 0, brought_back_by = ?2 WHERE id = ?1",
            by.to_string(),
        )
    }

    /// Records that the interrupted `id` is not to be brought back.
    pub fn let_go(&mut self, id: SessionId) -> Result<()> {
        known(&self.connection, id)?;
        self.connection.execute(
            "UPDATE sessions SET interrupted = 0 WHERE id = ?1",
            [id.to_string()],
        )?;
        Ok(())
    }

    /// Records the terminal `terminal` opened in the terminal host
    /// `generation` names.
    pub fn open_terminal(
        &mut self,
        terminal: &TerminalSession,
        generation: Option<String>,
        started_ms: u64,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT OR REPLACE INTO terminals (id, generation, name, cwd, program, file, command, started_ms, resource_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                crate::sessions::hex(terminal.id),
                generation,
                terminal.name,
                terminal.cwd.to_string_lossy(),
                serde_json::to_string(&terminal.program).map_err(Error::storage)?,
                terminal
                    .file
                    .as_ref()
                    .map(|file| file.to_string_lossy().into_owned()),
                serde_json::to_string(&terminal.command).map_err(Error::storage)?,
                started_ms as i64,
                terminal.resource_id.to_string(),
            ],
        )?;
        Ok(())
    }

    /// The terminals that have not ended, in the order they were opened.
    pub fn terminals(&self) -> Result<Vec<KeptTerminal>> {
        self.kept_terminals("ended = 0")
    }

    /// The terminals interrupted and neither opened again nor let go.
    pub fn interrupted_terminals(&self) -> Result<Vec<KeptTerminal>> {
        self.kept_terminals("interrupted = 1")
    }

    pub fn end_terminal(&mut self, terminal: u128) -> Result<()> {
        self.terminal_update(terminal, "ended = 1")
    }

    /// Records that `terminal` ended with the terminal host it ran in; one
    /// that had ended already stays as it ended.
    pub fn interrupt_terminal(&mut self, terminal: u128) -> Result<()> {
        self.terminal_update(terminal, "interrupted = 1 - ended, ended = 1")
    }

    /// Records that the interrupted `terminal` was opened again, or is not
    /// to be.
    pub fn reopened_terminal(&mut self, terminal: u128) -> Result<()> {
        self.terminal_update(terminal, "interrupted = 0")
    }

    fn terminal_update(&mut self, terminal: u128, set: &str) -> Result<()> {
        self.connection.execute(
            &format!("UPDATE terminals SET {set} WHERE id = ?1"),
            [crate::sessions::hex(terminal)],
        )?;
        Ok(())
    }

    fn kept_terminals(&self, filter: &str) -> Result<Vec<KeptTerminal>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT id, generation, name, cwd, program, file, command, started_ms, resource_id FROM terminals WHERE {filter} ORDER BY rowid"
        ))?;
        let rows = statement.query_map([], read_terminal)?;
        rows.map(|row| row.map_err(Error::from).and_then(|row| row))
            .collect()
    }

    pub fn send(&mut self, from: SessionId, to: SessionId, body: &str) -> Result<MessageId> {
        if from == to {
            return Err(Error::new(Code::Invalid, "a session cannot message itself"));
        }
        self.sent(&Sender::Session(from), to, body, Kind::Message)
    }

    /// Keeps `body` for `to` as work `from` asks of it.
    pub fn ask(&mut self, from: SessionId, to: SessionId, body: &str) -> Result<MessageId> {
        if from == to {
            return Err(Error::new(Code::Invalid, "a session cannot ask itself"));
        }
        self.sent(&Sender::Session(from), to, body, Kind::Request)
    }

    /// Whether `to` takes up work `from` asks of it: as the user said, and
    /// otherwise only when `from` opened it.
    pub fn takes_requests(&self, from: SessionId, to: SessionId) -> Result<bool> {
        let (from, to) = (self.known(from)?.code, self.known(to)?.code);
        self.takes_up(&from, &to)
    }

    fn takes_up(&self, from: &SessionCode, to: &SessionCode) -> Result<bool> {
        let codes = [from.as_str(), to.as_str()];
        let said = self
            .connection
            .query_row(
                "SELECT taken_up FROM requesters WHERE sender = ?1 AND recipient = ?2",
                codes,
                |row| row.get(0),
            )
            .optional()?;
        if let Some(said) = said {
            return Ok(said);
        }
        Ok(self.connection.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM sessions AS opened
                 JOIN sessions AS opener ON opener.id = opened.parent
                 WHERE opener.code = ?1 AND opened.code = ?2 AND ?1 <> ?2
             )",
            codes,
            |row| row.get(0),
        )?)
    }

    /// Keeps whether the session `to` takes up the work the one with the
    /// code `from` asks of it.
    pub fn let_ask(&mut self, from: &SessionCode, to: SessionId, taken_up: bool) -> Result<()> {
        let to = self.known(to)?.code;
        self.by_code(from)?
            .ok_or_else(|| Error::new(Code::NotFound, "no session has that code"))?;
        self.connection.execute(
            "INSERT INTO requesters (sender, recipient, taken_up) VALUES (?1, ?2, ?3)
             ON CONFLICT (sender, recipient) DO UPDATE SET taken_up = ?3",
            params![from.as_str(), to.as_str(), taken_up],
        )?;
        Ok(())
    }

    /// [`Store::let_ask`] for the session running in `terminal`.
    pub fn let_ask_terminal(
        &mut self,
        from: &SessionCode,
        terminal: u128,
        taken_up: bool,
    ) -> Result<()> {
        let to = self
            .sessions()?
            .into_iter()
            .find(|s| s.terminal == Some(terminal) && !s.ended)
            .ok_or_else(|| Error::new(Code::NotFound, "no session runs there"))?
            .id;
        self.let_ask(from, to, taken_up)
    }

    /// The sessions that may ask `id` for work or have asked it: the one that
    /// opened it, then the others.
    pub fn requesters(&self, id: SessionId) -> Result<Vec<Requester>> {
        let to = self.known(id)?.code;
        let mut statement = self.connection.prepare(
            "SELECT opener.code FROM sessions AS opened
                 JOIN sessions AS opener ON opener.id = opened.parent
                 WHERE opened.code = ?1
             UNION ALL
             SELECT sender.code FROM messages
                 JOIN sessions AS sender ON sender.id = messages.sender
                 JOIN sessions AS recipient ON recipient.id = messages.recipient
                 WHERE recipient.code = ?1 AND messages.kind IN ('request', 'declined')
             UNION ALL
             SELECT sender FROM requesters WHERE recipient = ?1",
        )?;
        let codes: Vec<String> = statement
            .query_map([to.as_str()], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let mut out: Vec<Requester> = Vec::new();
        for code in codes {
            let code: SessionCode = code.parse().map_err(Error::storage)?;
            if code == to || out.iter().any(|known| known.code == code) {
                continue;
            }
            let Some(session) = self.by_code(&code)? else {
                continue;
            };
            out.push(Requester {
                taken_up: self.takes_up(&code, &to)?,
                name: session.name,
                code,
            });
        }
        Ok(out)
    }

    /// Keeps `body`, which `from` proposes to pass on to the sessions `to` as
    /// the user's own words, until the user decides of it.
    pub fn propose_relay(
        &mut self,
        from: SessionId,
        to: &[SessionId],
        body: &str,
    ) -> Result<Relay> {
        let invalid = |why: String| Err(Error::new(Code::Invalid, why));
        if body.trim().is_empty() {
            return invalid("there are no words to pass on".into());
        }
        if body.len() > RELAY_BODY {
            return invalid(format!("words passed on hold at most {RELAY_BODY} bytes"));
        }
        if let Some(c) = body.chars().find(|c| hidden(*c)) {
            return invalid(format!(
                "the words hold U+{:04X}, which the user would not see",
                c as u32
            ));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let proposer = known(&transaction, from)?;
        let mut recipients: Vec<RelayTo> = Vec::new();
        for id in to {
            let to = known(&transaction, *id)?;
            if to.code == proposer.code {
                return invalid("a session cannot pass words on to itself".into());
            }
            if to.ended {
                return Err(Error::new(
                    Code::Refused,
                    format!("{} has ended and cannot be sent to", to.name),
                ));
            }
            if !recipients.iter().any(|known| known.code == to.code) {
                recipients.push(RelayTo {
                    code: to.code,
                    name: to.name,
                });
            }
        }
        if recipients.is_empty() || recipients.len() > RELAY_TO {
            return invalid(format!("words are passed on to 1 to {RELAY_TO} sessions"));
        }
        let waiting: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM relays WHERE proposer = ?1 AND decision IS NULL",
            [from.to_string()],
            |row| row.get(0),
        )?;
        if waiting >= RELAYS_WAITING {
            return Err(Error::new(
                Code::Refused,
                "what you proposed before still waits for the user; let them decide of it first",
            ));
        }
        let relay = Relay {
            id: MessageId::new(),
            from: proposer.code,
            from_name: proposer.name,
            to: recipients,
            body: body.to_owned(),
            proposed_ms: super::now_ms(),
        };
        let codes: Vec<&str> = relay.to.iter().map(|to| to.code.as_str()).collect();
        transaction.execute(
            "INSERT INTO relays (id, proposer, recipients, body, proposed_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                relay.id.to_string(),
                from.to_string(),
                serde_json::to_string(&codes).map_err(Error::storage)?,
                relay.body,
                relay.proposed_ms as i64,
            ],
        )?;
        transaction.commit()?;
        Ok(relay)
    }

    /// What sessions that run propose to pass on and the user has not decided
    /// of, oldest first, each to those it is to reach that still run.
    pub fn relays_waiting(&self) -> Result<Vec<Relay>> {
        let mut statement = self.connection.prepare(
            "SELECT relays.id, relays.proposer, recipients, body, proposed_ms FROM relays
                 JOIN sessions ON sessions.id = relays.proposer
                 WHERE decision IS NULL AND sessions.ended = 0 ORDER BY relays.rowid",
        )?;
        let rows: Vec<(String, String, String, String, i64)> = statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut out = Vec::new();
        for (id, proposer, recipients, body, proposed_ms) in rows {
            let proposer = self.known(proposer.parse().map_err(Error::storage)?)?;
            let codes: Vec<SessionCode> =
                serde_json::from_str(&recipients).map_err(Error::storage)?;
            let mut to = Vec::new();
            for code in codes {
                if let Some(session) = self.by_code(&code)?.filter(|s| !s.ended) {
                    to.push(RelayTo {
                        code,
                        name: session.name,
                    });
                }
            }
            out.push(Relay {
                id: id.parse().map_err(Error::storage)?,
                from: proposer.code,
                from_name: proposer.name,
                to,
                body,
                proposed_ms: proposed_ms as u64,
            });
        }
        Ok(out)
    }

    /// Sends what is proposed as `id` to those of `to` that run, as the user's
    /// own words, which the user approved as it stands.
    pub fn approve_relay(&mut self, id: MessageId, to: &[SessionCode]) -> Result<Passed> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (proposer, recipients, body) = undecided(&transaction, id)?;
        let refused = |why: &str| Err(Error::new(Code::Refused, why));
        if to.is_empty() || to.iter().any(|code| !recipients.contains(code)) {
            return refused("the words were not proposed to those sessions");
        }
        let mut passed = Passed {
            sent: Vec::new(),
            missed: Vec::new(),
        };
        // In the order proposed, each once.
        for code in recipients.iter().filter(|code| to.contains(code)) {
            let session = by_code(&transaction, code)?
                .ok_or_else(|| Error::new(Code::NotFound, "no session has that code"))?;
            if session.ended {
                passed.missed.push(session.name);
                continue;
            }
            insert(
                &transaction,
                &Sender::Session(proposer),
                session.id,
                &body,
                Kind::Relayed,
            )?;
            passed.sent.push(session.name);
        }
        if passed.sent.is_empty() {
            return refused("every session the words were to reach has ended");
        }
        decide(&transaction, id, "approved")?;
        transaction.commit()?;
        Ok(passed)
    }

    /// Keeps that the user passes on nothing of what is proposed as `id`.
    pub fn decline_relay(&mut self, id: MessageId) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        undecided(&transaction, id)?;
        decide(&transaction, id, "declined")?;
        transaction.commit()?;
        Ok(())
    }

    /// Keeps that the user sent what they wrote to the session running in
    /// `terminal` at `at_ms`.
    pub fn wrote(&mut self, terminal: u128, at_ms: u64) -> Result<()> {
        self.connection.execute(
            "UPDATE sessions SET wrote_ms = MAX(COALESCE(wrote_ms, 0), ?2)
                 WHERE terminal = ?1 AND ended = 0",
            params![crate::sessions::hex(terminal), at_ms as i64],
        )?;
        Ok(())
    }

    /// Whether the user's words passed on were handed to the session `id`,
    /// also as the one it was resumed from, since the user last wrote to it.
    pub fn told_through_another(&self, id: SessionId) -> Result<bool> {
        let code = self.known(id)?.code;
        Ok(self.connection.query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM messages JOIN sessions ON sessions.id = messages.recipient
                 WHERE sessions.code = ?1 AND kind = 'relayed' AND delivered = 1
                     AND delivered_ms >= (
                         SELECT COALESCE(MAX(wrote_ms), 0) FROM sessions WHERE code = ?1
                     )
             )",
            [code.as_str()],
            |row| row.get(0),
        )?)
    }

    /// Keeps how the request `id` is handed over.
    pub fn handing_over(&mut self, id: MessageId, kind: Kind) -> Result<()> {
        self.connection.execute(
            "UPDATE messages SET kind = ?2 WHERE id = ?1",
            params![id.to_string(), kind.key()],
        )?;
        Ok(())
    }

    /// Keeps `body` for `to` as asked by the program outside Adroit that
    /// calls itself `from`.
    pub fn ask_from_outside(&mut self, from: &str, to: SessionId, body: &str) -> Result<MessageId> {
        let from = from.trim();
        if from.is_empty()
            || from.chars().count() > OUTSIDE_NAME
            || from.chars().any(char::is_control)
        {
            return Err(Error::new(
                Code::Invalid,
                format!("the sender's name is one line of 1 to {OUTSIDE_NAME} characters"),
            ));
        }
        if body.len() > OUTSIDE_BODY {
            return Err(Error::new(
                Code::Invalid,
                format!(
                    "a request holds at most {OUTSIDE_BODY} bytes; refer to larger material by its path"
                ),
            ));
        }
        self.sent(&Sender::Outside(from.to_owned()), to, body, Kind::Request)
    }

    /// Keeps that the program outside Adroit called `by` did `what` to
    /// `session`, as a message of its that nothing is left to hand over of.
    pub fn record_act(&mut self, by: &str, session: SessionId, what: &str) -> Result<()> {
        let id = insert(
            &self.connection,
            &Sender::Outside(by.to_owned()),
            session,
            what,
            Kind::Message,
        )?;
        self.delivered(id)
    }

    fn sent(&mut self, from: &Sender, to: SessionId, body: &str, kind: Kind) -> Result<MessageId> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let recipient = session(&transaction, to)?
            .ok_or_else(|| Error::new(Code::NotFound, "no session has that id"))?;
        if recipient.ended {
            return Err(Error::new(
                Code::Refused,
                format!("{} has ended and cannot be sent to", recipient.name),
            ));
        }
        let id = insert(&transaction, from, to, body, kind)?;
        transaction.commit()?;
        Ok(id)
    }

    /// The oldest message waiting for `recipient` while it runs.
    pub fn next_undelivered(&self, recipient: SessionId) -> Result<Option<Message>> {
        if session(&self.connection, recipient)?.is_none_or(|session| session.ended) {
            return Ok(None);
        }
        self.connection
            .query_row(
                &format!(
                    "SELECT {MESSAGE_COLUMNS} FROM messages WHERE recipient = ?1 AND delivered = 0 ORDER BY seq LIMIT 1"
                ),
                [recipient.to_string()],
                read_message,
            )
            .optional()?
            .transpose()
    }

    pub fn delivered(&mut self, id: MessageId) -> Result<()> {
        self.connection.execute(
            "UPDATE messages SET delivered = 1, delivered_ms = COALESCE(delivered_ms, ?2) WHERE id = ?1",
            params![id.to_string(), super::now_ms() as i64],
        )?;
        Ok(())
    }

    /// What `id` sent and received, the newest first.
    pub fn messages_of(&self, id: SessionId) -> Result<Vec<Message>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {MESSAGE_COLUMNS} FROM messages WHERE sender = ?1 OR recipient = ?1 ORDER BY seq DESC"
        ))?;
        let rows = statement.query_map([id.to_string()], read_message)?;
        rows.map(|row| row.map_err(Error::from).and_then(|row| row))
            .collect()
    }

    /// Adds `entry` to the journal of the session with `code`.
    pub fn record_timeline(&mut self, code: &SessionCode, entry: &Entry) -> Result<()> {
        self.connection.execute(
            "INSERT INTO timeline (code, at_ms, ops) VALUES (?1, ?2, ?3)",
            params![
                code.as_str(),
                entry.at_ms as i64,
                serde_json::to_string(&entry.ops).map_err(Error::storage)?,
            ],
        )?;
        Ok(())
    }

    /// The journal of the session with `code`, the oldest call first.
    pub fn timeline_of(&self, code: &SessionCode) -> Result<Vec<Entry>> {
        let mut statement = self
            .connection
            .prepare("SELECT at_ms, ops FROM timeline WHERE code = ?1 ORDER BY seq")?;
        let rows = statement.query_map([code.as_str()], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (at_ms, ops) = row?;
            Ok(Entry {
                at_ms: at_ms as u64,
                ops: serde_json::from_str(&ops).map_err(Error::storage)?,
            })
        })
        .collect()
    }

    /// Keeps that `session` now stands as `body` says.
    pub fn record_standing(&mut self, session: &str, body: &serde_json::Value) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO standings (session, at_ms, body) VALUES (?1, ?2, ?3)",
            params![session, super::now_ms() as i64, body.to_string()],
        )?;
        transaction.execute(
            "DELETE FROM standings WHERE seq <= ?1",
            [transaction.last_insert_rowid() - STANDINGS],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// The number of the oldest change kept and of the newest there was.
    pub fn standings_kept(&self) -> Result<(i64, i64)> {
        let (oldest, newest): (Option<i64>, Option<i64>) =
            self.connection
                .query_row("SELECT MIN(seq), MAX(seq) FROM standings", [], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?;
        Ok((oldest.unwrap_or(1), newest.unwrap_or(0)))
    }

    /// The first `most` changes after `since`: the number of each, when it
    /// was and how it left its session.
    pub fn standings_since(
        &self,
        since: i64,
        most: usize,
    ) -> Result<Vec<(i64, u64, serde_json::Value)>> {
        let mut statement = self.connection.prepare(
            "SELECT seq, at_ms, body FROM standings WHERE seq > ?1 ORDER BY seq LIMIT ?2",
        )?;
        let rows = statement.query_map(params![since, most as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (seq, at_ms, body) = row?;
            let body = serde_json::from_str(&body).map_err(Error::storage)?;
            Ok((seq, at_ms as u64, body))
        })
        .collect()
    }

    /// How each session last stood, by its id.
    pub fn last_standings(&self) -> Result<Vec<(String, serde_json::Value)>> {
        let mut statement = self.connection.prepare(
            "SELECT session, body FROM standings WHERE seq IN (SELECT MAX(seq) FROM standings GROUP BY session)",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (session, body) = row?;
            Ok((
                session,
                serde_json::from_str(&body).map_err(Error::storage)?,
            ))
        })
        .collect()
    }

    fn update(&mut self, id: SessionId, sql: &str, value: String) -> Result<()> {
        let changed = self
            .connection
            .execute(sql, params![id.to_string(), value])?;
        if changed == 0 {
            return Err(Error::new(Code::NotFound, "the session is unknown"));
        }
        Ok(())
    }
}

const SESSION_COLUMNS: &str = "id, parent, agent, name, cwd, terminal, conversation, readiness, started_ms, ended, profile, home, model, permissions, generation, interrupted, brought_back_by, name_given, code";

fn read_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Session>> {
    let id: String = row.get("id")?;
    let parent: Option<String> = row.get("parent")?;
    let agent: String = row.get("agent")?;
    let cwd: String = row.get("cwd")?;
    let terminal: Option<String> = row.get("terminal")?;
    let readiness: String = row.get("readiness")?;
    let started_ms: i64 = row.get("started_ms")?;
    let name = row.get("name")?;
    let conversation = row.get("conversation")?;
    let ended = row.get("ended")?;
    let profile: String = row.get("profile")?;
    let home: Option<String> = row.get("home")?;
    let model = row.get("model")?;
    let permissions: Option<String> = row.get("permissions")?;
    let generation = row.get("generation")?;
    let interrupted = row.get("interrupted")?;
    let brought_back_by: Option<String> = row.get("brought_back_by")?;
    let name_given = row.get("name_given")?;
    let code: Option<String> = row.get("code")?;
    Ok((|| {
        Ok(Session {
            id: id.parse().map_err(Error::storage)?,
            parent: parent
                .map(|parent| parent.parse())
                .transpose()
                .map_err(Error::storage)?,
            agent: Agent::ALL
                .into_iter()
                .find(|known| known.key() == agent)
                .ok_or_else(|| Error::storage(format!("unknown agent {agent}")))?,
            profile: profile.parse().map_err(Error::storage)?,
            home: home.map(PathBuf::from),
            name,
            cwd: PathBuf::from(cwd),
            terminal: terminal.as_deref().and_then(crate::sessions::id_of),
            conversation,
            readiness: Readiness::from_key(&readiness)
                .ok_or_else(|| Error::storage(format!("unknown readiness {readiness}")))?,
            started_ms: started_ms as u64,
            ended,
            model,
            permissions: permissions
                .map(|permissions| serde_json::from_str(&permissions))
                .transpose()
                .map_err(Error::storage)?
                .unwrap_or_default(),
            generation,
            interrupted,
            brought_back_by: brought_back_by
                .map(|by| by.parse())
                .transpose()
                .map_err(Error::storage)?,
            name_given,
            code: code
                .ok_or_else(|| Error::storage("a session has no code"))?
                .parse()
                .map_err(Error::storage)?,
        })
    })())
}

fn read_terminal(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<KeptTerminal>> {
    let id: String = row.get("id")?;
    let generation = row.get("generation")?;
    let name = row.get("name")?;
    let cwd: String = row.get("cwd")?;
    let program: String = row.get("program")?;
    let file: Option<String> = row.get("file")?;
    let command: String = row.get("command")?;
    let started_ms: i64 = row.get("started_ms")?;
    let resource_id: String = row.get("resource_id")?;
    Ok((|| {
        Ok(KeptTerminal {
            terminal: TerminalSession {
                id: crate::sessions::id_of(&id)
                    .ok_or_else(|| Error::storage(format!("unknown terminal {id}")))?,
                name,
                cwd: PathBuf::from(cwd),
                program: serde_json::from_str(&program).map_err(Error::storage)?,
                file: file.map(PathBuf::from),
                command: serde_json::from_str(&command).map_err(Error::storage)?,
                resource_id: resource_id.parse().map_err(Error::storage)?,
                ..Default::default()
            },
            generation,
            started_ms: started_ms as u64,
        })
    })())
}

fn session(connection: &Connection, id: SessionId) -> Result<Option<Session>> {
    connection
        .query_row(
            &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
            [id.to_string()],
            read_session,
        )
        .optional()?
        .transpose()
}

fn known(connection: &Connection, id: SessionId) -> Result<Session> {
    session(connection, id)?.ok_or_else(|| Error::new(Code::NotFound, "the session is unknown"))
}

fn by_code(connection: &Connection, code: &SessionCode) -> Result<Option<Session>> {
    connection
        .query_row(
            &format!(
                "SELECT {SESSION_COLUMNS} FROM sessions WHERE code = ?1 ORDER BY rowid DESC LIMIT 1"
            ),
            [code.as_str()],
            read_session,
        )
        .optional()?
        .transpose()
}

/// What is proposed as `id` while the user has not decided of it and the
/// session that proposed it runs: that session, the codes it is to reach and
/// the words.
fn undecided(
    connection: &Connection,
    id: MessageId,
) -> Result<(SessionId, Vec<SessionCode>, String)> {
    let found: Option<(String, String, String, bool)> = connection
        .query_row(
            "SELECT proposer, recipients, body, decision IS NULL AND sessions.ended = 0 FROM relays
                 JOIN sessions ON sessions.id = relays.proposer WHERE relays.id = ?1",
            [id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    match found {
        Some((proposer, recipients, body, true)) => Ok((
            proposer.parse().map_err(Error::storage)?,
            serde_json::from_str(&recipients).map_err(Error::storage)?,
            body,
        )),
        Some(_) => Err(Error::new(
            Code::Refused,
            "the words were decided of already, or the session that proposed them has ended",
        )),
        None => Err(Error::new(Code::NotFound, "no words are proposed as that")),
    }
}

fn decide(connection: &Connection, id: MessageId, decision: &str) -> Result<()> {
    connection.execute(
        "UPDATE relays SET decision = ?2, decided_ms = ?3 WHERE id = ?1",
        params![id.to_string(), decision, super::now_ms() as i64],
    )?;
    Ok(())
}

const MESSAGE_COLUMNS: &str = "id, sender, outside, recipient, kind, body, sent_ms, delivered";

fn read_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Message>> {
    let id: String = row.get("id")?;
    let sender: Option<String> = row.get("sender")?;
    let outside: Option<String> = row.get("outside")?;
    let recipient: String = row.get("recipient")?;
    let sent_ms: i64 = row.get("sent_ms")?;
    let kind: String = row.get("kind")?;
    let body = row.get("body")?;
    let delivered = row.get("delivered")?;
    Ok((|| {
        Ok(Message {
            id: id.parse().map_err(Error::storage)?,
            sender: match (outside, sender) {
                (Some(name), _) => Sender::Outside(name),
                (None, Some(id)) => Sender::Session(id.parse().map_err(Error::storage)?),
                (None, None) => return Err(Error::storage("a message names no sender")),
            },
            recipient: recipient.parse().map_err(Error::storage)?,
            kind: Kind::from_key(&kind)
                .ok_or_else(|| Error::storage(format!("no message is a {kind}")))?,
            body,
            sent_ms: sent_ms as u64,
            delivered,
        })
    })())
}

fn insert(
    connection: &Connection,
    sender: &Sender,
    recipient: SessionId,
    body: &str,
    kind: Kind,
) -> Result<MessageId> {
    if body.trim().is_empty() {
        return Err(Error::new(Code::Invalid, "a message needs a body"));
    }
    let id = MessageId::new();
    let (session, outside) = match sender {
        Sender::Session(id) => (Some(id), None),
        Sender::Outside(name) => (None, Some(name)),
    };
    connection.execute(
        "INSERT INTO messages (id, sender, outside, recipient, kind, body, sent_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id.to_string(),
            session.map(|id| id.to_string()),
            outside,
            recipient.to_string(),
            kind.key(),
            body,
            super::now_ms() as i64,
        ],
    )?;
    Ok(id)
}

/// Carries the database from the schema it holds to this adroit's, one step
/// at a time, each step whole or not at all.
fn migrate(connection: &mut Connection) -> Result<()> {
    loop {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let schema: i64 = transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let Some(step) = usize::try_from(schema - 1)
            .ok()
            .and_then(|at| MIGRATIONS.get(at))
        else {
            return Ok(());
        };
        transaction.execute_batch(step)?;
        transaction.pragma_update(None, "user_version", schema + 1)?;
        transaction.commit()?;
    }
}

/// How many codes are drawn before giving up: with few of the codes there
/// are given, a draw that hits one given before is rare.
const DRAWS: usize = 64;

fn draw_code(
    connection: &Connection,
    mut draw: impl FnMut() -> SessionCode,
) -> Result<SessionCode> {
    let mut given = connection.prepare_cached("SELECT 1 FROM sessions WHERE code = ?1")?;
    for _ in 0..DRAWS {
        let code = draw();
        if !given.exists([code.as_str()])? {
            return Ok(code);
        }
    }
    Err(Error::storage("no session code is left to give"))
}

/// Gives every session recorded before codes were its own, the one brought
/// back taking the code of the one it was brought back for.
fn fill_codes(connection: &mut Connection) -> Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let missing: Vec<String> = {
        let mut statement =
            transaction.prepare("SELECT id FROM sessions WHERE code IS NULL ORDER BY rowid")?;
        statement
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    for id in missing {
        let inherited: Option<String> = transaction
            .query_row(
                "SELECT code FROM sessions WHERE brought_back_by = ?1 AND code IS NOT NULL",
                [&id],
                |row| row.get(0),
            )
            .optional()?;
        let code = match inherited {
            Some(code) => code,
            None => draw_code(&transaction, SessionCode::random)?.to_string(),
        };
        transaction.execute(
            "UPDATE sessions SET code = ?2 WHERE id = ?1",
            params![id, code],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

/// The lock on the file beside the database at `path`, held while this
/// process sets the database up.
fn setting_up(path: &Path) -> Result<std::fs::File> {
    let mut lock = path.as_os_str().to_owned();
    lock.push(".lock");
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(lock)
        .map_err(Error::storage)?;
    file.lock().map_err(Error::storage)?;
    Ok(file)
}

/// Creates the database privately, returning whether it was created now.
fn private_database(path: &Path) -> Result<bool> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::storage("the database path has no directory"))?;
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(Error::storage)?;
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(path).map_err(Error::storage)?;
            if !metadata.is_file() || metadata.uid() != adroit_sys::euid() {
                return Err(Error::storage(
                    "the database must be a regular file of this user",
                ));
            }
            Ok(false)
        }
        Err(error) => Err(Error::storage(error)),
    }
}

/// The schema version the database is at, refusing one a newer adroit wrote.
fn schema(connection: &Connection) -> Result<i64> {
    let schema: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if schema > SCHEMA_VERSION {
        return Err(newer(schema));
    }
    Ok(schema)
}

fn newer(schema: i64) -> Error {
    Error::new(Code::Unsupported, adroit_protocol::newer_state(schema))
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Error::storage(format!("cannot update delegation state: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_model::TerminalProgram;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("delegation/state.sqlite3")).unwrap();
        (dir, store)
    }

    #[test]
    fn a_new_database_opened_by_several_at_once_is_created_once() {
        for _ in 0..20 {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("delegation/state.sqlite3");
            let barrier = std::sync::Barrier::new(8);
            std::thread::scope(|scope| {
                let opened: Vec<_> = (0..8)
                    .map(|_| {
                        scope.spawn(|| {
                            barrier.wait();
                            Store::open(&path).map(drop)
                        })
                    })
                    .collect();
                for open in opened {
                    open.join().unwrap().unwrap();
                }
            });
        }
    }

    fn new(parent: Option<SessionId>, name: &str) -> NewSession {
        NewSession {
            parent,
            agent: Agent::Claude,
            profile: ProfileId::builtin(Agent::Claude),
            home: None,
            name: name.into(),
            cwd: "/repo".into(),
            launch: Settings::default(),
            model: None,
            permissions: AgentPermissions::Default,
            started_ms: 1,
            name_given: false,
            code: None,
        }
    }

    fn register(store: &mut Store, parent: Option<SessionId>, name: &str) -> SessionId {
        store.register(new(parent, name)).unwrap().0
    }

    fn code(store: &Store, id: SessionId) -> SessionCode {
        store.known(id).unwrap().code
    }

    fn bodies(store: &mut Store, id: SessionId) -> Vec<String> {
        let mut bodies = Vec::new();
        while let Some(message) = store.next_undelivered(id).unwrap() {
            store.delivered(message.id).unwrap();
            bodies.push(message.body);
        }
        bodies
    }

    #[test]
    fn a_session_never_registered_is_not_known() {
        let (_dir, store) = store();

        let error = store.known(SessionId::new()).unwrap_err();

        assert_eq!(error, Error::new(Code::NotFound, "the session is unknown"));
    }

    #[test]
    fn a_registered_session_is_known() {
        let (_dir, mut store) = store();
        let id = register(&mut store, None, "opened");

        assert_eq!(store.known(id).unwrap().id, id);
    }

    #[test]
    fn a_registered_session_acts_with_its_credential_and_no_other() {
        let (_dir, mut store) = store();
        let (id, token) = store.register(new(None, "opener")).unwrap();

        assert_eq!(store.authenticate(&token).unwrap(), id);
        assert_eq!(
            store.authenticate("forged").unwrap_err().code,
            Code::Refused
        );
    }

    #[test]
    fn a_session_that_has_ended_no_longer_acts() {
        let (_dir, mut store) = store();
        let (id, token) = store.register(new(None, "opener")).unwrap();
        store.end(id).unwrap();

        assert_eq!(store.authenticate(&token).unwrap_err().code, Code::Refused);
    }

    #[test]
    fn a_session_opened_by_another_opens_sessions_too() {
        let (_dir, mut store) = store();
        let first = register(&mut store, None, "first");
        let second = register(&mut store, Some(first), "second");

        let third = register(&mut store, Some(second), "third");

        assert_eq!(store.session(third).unwrap().unwrap().parent, Some(second));
    }

    #[test]
    fn messages_reach_their_recipient_in_the_order_sent() {
        let (_dir, mut store) = store();
        let a = register(&mut store, None, "a");
        let b = register(&mut store, None, "b");
        store.send(a, b, "first").unwrap();
        store.send(a, b, "second").unwrap();

        assert_eq!(bodies(&mut store, b), ["first", "second"]);
        assert_eq!(bodies(&mut store, a), Vec::<String>::new());
    }

    #[test]
    fn a_delivered_message_is_not_handed_over_again() {
        let (_dir, mut store) = store();
        let a = register(&mut store, None, "a");
        let b = register(&mut store, None, "b");
        let id = store.send(a, b, "once").unwrap();
        store.delivered(id).unwrap();

        assert_eq!(store.next_undelivered(b).unwrap(), None);
    }

    #[test]
    fn any_session_can_message_an_unrelated_one() {
        let (_dir, mut store) = store();
        let opener = register(&mut store, None, "opener");
        let opened = register(&mut store, Some(opener), "opened");
        let other = register(&mut store, None, "other");

        store
            .send(opened, other, "I am about to rebase main")
            .unwrap();

        assert_eq!(bodies(&mut store, other), ["I am about to rebase main"]);
    }

    #[test]
    fn a_session_that_has_ended_cannot_be_sent_to() {
        let (_dir, mut store) = store();
        let a = register(&mut store, None, "a");
        let b = register(&mut store, None, "b");
        store.end(b).unwrap();

        assert_eq!(store.send(a, b, "hello").unwrap_err().code, Code::Refused);
    }

    #[test]
    fn a_session_may_ask_those_it_opened_for_work_and_no_other_until_the_user_lets_it() {
        let (_dir, mut store) = store();
        let opener = register(&mut store, None, "opener");
        let opened = register(&mut store, Some(opener), "opened");
        let sibling = register(&mut store, Some(opener), "sibling");
        let grandchild = register(&mut store, Some(opened), "grandchild");
        for (from, to, takes) in [
            (opener, opened, true),
            (opened, opener, false),
            (opened, sibling, false),
            (opener, grandchild, false),
        ] {
            assert_eq!(store.takes_requests(from, to).unwrap(), takes);
        }

        store.let_ask(&code(&store, opened), opener, true).unwrap();
        store.let_ask(&code(&store, opener), opened, false).unwrap();

        assert!(store.takes_requests(opened, opener).unwrap());
        assert!(!store.takes_requests(opener, opened).unwrap());
        assert!(!store.takes_requests(opened, sibling).unwrap());
    }

    #[test]
    fn whom_the_user_let_ask_holds_for_the_records_that_keep_both_sessions_codes() {
        let (_dir, mut store) = store();
        let a = register(&mut store, None, "a");
        let b = register(&mut store, None, "b");
        store.let_ask(&code(&store, a), b, true).unwrap();
        let resumed = |store: &mut Store, id, name| {
            let code = code(store, id);
            store.end(id).unwrap();
            store
                .register(NewSession {
                    code: Some(code),
                    ..new(None, name)
                })
                .unwrap()
                .0
        };
        let (a, b) = (
            resumed(&mut store, a, "a again"),
            resumed(&mut store, b, "b again"),
        );
        assert!(store.takes_requests(a, b).unwrap());
    }

    /// `from`, `to` and `other`, of which none opened another.
    fn three(store: &mut Store) -> (SessionId, SessionId, SessionId) {
        (
            register(store, None, "from"),
            register(store, None, "to"),
            register(store, None, "other"),
        )
    }

    #[test]
    fn words_proposed_reach_no_session_until_the_user_approves_them() {
        let (_dir, mut store) = store();
        let (from, to, other) = three(&mut store);
        let relay = store.propose_relay(from, &[to, other], "use tabs").unwrap();
        assert!(store.next_undelivered(to).unwrap().is_none());
        assert_eq!(
            store.relays_waiting().unwrap(),
            std::slice::from_ref(&relay)
        );

        let codes = [code(&store, to), code(&store, other)];
        let passed = store.approve_relay(relay.id, &codes).unwrap();

        assert_eq!(passed.said(), "Passed your words on to to, other");
        for id in [to, other] {
            let words = store.next_undelivered(id).unwrap().unwrap();
            assert_eq!(
                (words.kind, words.sender, words.body.as_str()),
                (Kind::Relayed, Sender::Session(from), "use tabs")
            );
        }
        assert!(store.relays_waiting().unwrap().is_empty());
        assert_eq!(store.messages_of(from).unwrap().len(), 2);
    }

    #[test]
    fn words_declined_reach_no_session_and_cannot_be_approved_after() {
        let (_dir, mut store) = store();
        let (from, to, _) = three(&mut store);
        let relay = store.propose_relay(from, &[to], "use tabs").unwrap();
        store.decline_relay(relay.id).unwrap();

        let refused = store.approve_relay(relay.id, &[code(&store, to)]);
        assert_eq!(refused.unwrap_err().code, Code::Refused);
        assert!(store.next_undelivered(to).unwrap().is_none());
        assert!(store.relays_waiting().unwrap().is_empty());
    }

    #[test]
    fn words_are_approved_once_and_only_for_sessions_they_were_proposed_to() {
        let (_dir, mut store) = store();
        let (from, to, other) = three(&mut store);
        let relay = store.propose_relay(from, &[to], "use tabs").unwrap();
        let (to_code, other_code) = (code(&store, to), code(&store, other));
        let refused = |store: &mut Store, to: &[SessionCode]| {
            store.approve_relay(relay.id, to).unwrap_err().code
        };
        assert_eq!(refused(&mut store, &[]), Code::Refused);
        assert_eq!(refused(&mut store, &[other_code]), Code::Refused);
        store
            .approve_relay(relay.id, std::slice::from_ref(&to_code))
            .unwrap();
        assert_eq!(refused(&mut store, &[to_code]), Code::Refused);
        let unknown = store.approve_relay(MessageId::new(), &[]).unwrap_err();
        assert_eq!(unknown.code, Code::NotFound);
        assert_eq!(store.messages_of(to).unwrap().len(), 1);
    }

    #[test]
    fn words_go_only_to_those_chosen_and_still_running_as_they_are_approved() {
        let (_dir, mut store) = store();
        let (from, to, other) = three(&mut store);
        let left_out = register(&mut store, None, "left out");
        let relay = store
            .propose_relay(from, &[to, other, left_out, to], "use tabs")
            .unwrap();
        let codes = [code(&store, to), code(&store, other)];
        store.end(other).unwrap();

        let passed = store.approve_relay(relay.id, &codes).unwrap();

        assert_eq!(
            passed.said(),
            "Passed your words on to to; not to other, ended meanwhile"
        );
        assert!(store.messages_of(left_out).unwrap().is_empty());
        let late = store.propose_relay(from, &[to], "again").unwrap();
        store.end(to).unwrap();
        let refused = store.approve_relay(late.id, &codes[..1]).unwrap_err();
        assert_eq!(refused.code, Code::Refused);
    }

    #[test]
    fn what_a_session_proposed_goes_with_it() {
        let (_dir, mut store) = store();
        let (from, to, _) = three(&mut store);
        let relay = store.propose_relay(from, &[to], "use tabs").unwrap();
        store.end(from).unwrap();
        assert!(store.relays_waiting().unwrap().is_empty());
        let refused = store.approve_relay(relay.id, &[code(&store, to)]);
        assert_eq!(refused.unwrap_err().code, Code::Refused);
    }

    #[test]
    fn words_the_user_would_not_see_whole_or_to_no_running_other_session_are_not_proposed() {
        let (_dir, mut store) = store();
        let (from, to, ended) = three(&mut store);
        store.end(ended).unwrap();
        let mut refused =
            |to: &[SessionId], body: &str| store.propose_relay(from, to, body).unwrap_err().code;
        let long = "x".repeat(RELAY_BODY + 1);
        for body in [" ", "a\u{202e}b", "a\u{1b}[8mb", "a\u{200b}", &long] {
            assert_eq!(refused(&[to], body), Code::Invalid, "{body:.20?}");
        }
        assert_eq!(refused(&[], "hi"), Code::Invalid);
        assert_eq!(refused(&[to, from], "hi"), Code::Invalid);
        assert_eq!(refused(&[to, ended], "hi"), Code::Refused);
        assert_eq!(refused(&[SessionId::new()], "hi"), Code::NotFound);
        let said = "\tタブを使って。\n👩‍💻";
        assert_eq!(store.propose_relay(from, &[to], said).unwrap().body, said);
    }

    #[test]
    fn only_a_few_proposals_of_a_session_wait_at_once() {
        let (_dir, mut store) = store();
        let (from, to, other) = three(&mut store);
        let first = store.propose_relay(from, &[to], "one").unwrap();
        for _ in 1..RELAYS_WAITING {
            store.propose_relay(from, &[to], "more").unwrap();
        }
        let refused = store.propose_relay(from, &[to], "more").unwrap_err();
        assert_eq!(refused.code, Code::Refused);
        store.propose_relay(other, &[to], "mine").unwrap();
        store.decline_relay(first.id).unwrap();
        store.propose_relay(from, &[to], "more").unwrap();
    }

    #[test]
    fn words_count_as_told_once_handed_over_until_the_user_writes_also_after_resuming() {
        let (_dir, mut store) = store();
        let (from, to, _) = three(&mut store);
        let relay = store.propose_relay(from, &[to], "use tabs").unwrap();
        let to_code = code(&store, to);
        store
            .approve_relay(relay.id, std::slice::from_ref(&to_code))
            .unwrap();
        assert!(!store.told_through_another(to).unwrap());
        let words = store.next_undelivered(to).unwrap().unwrap();
        store.delivered(words.id).unwrap();
        assert!(store.told_through_another(to).unwrap());
        assert!(!store.told_through_another(from).unwrap());
        store.end(to).unwrap();
        let again = NewSession {
            code: Some(to_code),
            ..new(None, "again")
        };
        let (again, _) = store.register(again).unwrap();
        store.attach(again, 7, None).unwrap();
        assert!(store.told_through_another(again).unwrap());

        let now = crate::delegation::now_ms();
        store.wrote(8, now + 1000).unwrap();
        store.wrote(7, now - 60_000).unwrap();
        assert!(store.told_through_another(again).unwrap());
        store.wrote(7, now + 1000).unwrap();
        assert!(!store.told_through_another(again).unwrap());
        store.wrote(7, now).unwrap();
        assert!(!store.told_through_another(again).unwrap());
    }

    #[test]
    fn a_request_is_told_apart_from_a_message_and_remembers_how_it_was_handed_over() {
        let (_dir, mut store) = store();
        let a = register(&mut store, None, "a");
        let b = register(&mut store, Some(a), "b");
        store.ask(a, b, "do it").unwrap();
        store.send(a, b, "know it").unwrap();

        let request = store.next_undelivered(b).unwrap().unwrap();
        assert_eq!(request.kind, Kind::Request);
        store.handing_over(request.id, Kind::Declined).unwrap();
        store.delivered(request.id).unwrap();

        let kinds: Vec<_> = store
            .messages_of(b)
            .unwrap()
            .into_iter()
            .map(|m| m.kind)
            .collect();
        assert_eq!(kinds, [Kind::Message, Kind::Declined]);
    }

    #[test]
    fn those_listed_as_asking_a_session_are_its_opener_and_those_that_asked_or_were_let() {
        let (_dir, mut store) = store();
        let opener = register(&mut store, None, "opener");
        let id = register(&mut store, Some(opener), "shown");
        let asked = register(&mut store, None, "asked");
        let told = register(&mut store, None, "told");
        let allowed = register(&mut store, None, "allowed");
        store.ask(asked, id, "do it").unwrap();
        store.send(told, id, "know it").unwrap();
        store.let_ask(&code(&store, allowed), id, true).unwrap();

        let mut listed: Vec<_> = store
            .requesters(id)
            .unwrap()
            .into_iter()
            .map(|r| (r.name, r.taken_up))
            .collect();
        listed.sort();
        assert_eq!(
            listed,
            [
                ("allowed".into(), true),
                ("asked".into(), false),
                ("opener".into(), true)
            ]
        );
    }

    #[test]
    fn a_request_from_outside_waits_for_its_recipient_under_the_name_it_gave_itself() {
        let (_dir, mut store) = store();
        let id = register(&mut store, None, "worker");

        store.ask_from_outside(" Famulen ", id, "go on").unwrap();

        let waiting = store.next_undelivered(id).unwrap().unwrap();
        assert_eq!(waiting.sender, Sender::Outside("Famulen".into()));
        assert_eq!(waiting.body, "go on");
    }

    #[test]
    fn a_request_from_outside_is_refused_without_a_one_line_name_a_body_or_a_running_recipient() {
        let (_dir, mut store) = store();
        let id = register(&mut store, None, "worker");
        let long = "n".repeat(OUTSIDE_NAME + 1);
        for name in ["", "  ", "a\nb", "a\u{1b}[0m", &long] {
            let refused = store.ask_from_outside(name, id, "go on").unwrap_err();
            assert_eq!(refused.code, Code::Invalid, "{name:?}");
        }
        for body in [" ".to_string(), "x".repeat(OUTSIDE_BODY + 1)] {
            let refused = store.ask_from_outside("Famulen", id, &body).unwrap_err();
            assert_eq!(refused.code, Code::Invalid);
        }
        store.end(id).unwrap();
        let refused = store.ask_from_outside("Famulen", id, "go on").unwrap_err();
        assert_eq!(refused.code, Code::Refused);
        assert!(store.messages_of(id).unwrap().is_empty());
    }

    #[test]
    fn what_a_program_did_is_kept_beside_the_session_and_handed_to_no_one() {
        let (_dir, mut store) = store();
        let id = register(&mut store, None, "worker");
        store
            .record_act("Famulen", id, "Ended the session")
            .unwrap();
        assert!(store.next_undelivered(id).unwrap().is_none());
        let kept = store.messages_of(id).unwrap();
        assert_eq!(kept[0].sender, Sender::Outside("Famulen".into()));
        assert_eq!(kept[0].body, "Ended the session");
    }

    #[test]
    fn the_last_standing_of_each_session_and_only_the_latest_changes_are_kept() {
        let (_dir, mut store) = store();
        let state = |state: &str| serde_json::json!({"state": state});
        store.record_standing("a", &state("working")).unwrap();
        store.record_standing("b", &state("idle")).unwrap();
        store.record_standing("a", &state("done")).unwrap();
        let mut last = store.last_standings().unwrap();
        last.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            last,
            [("a".into(), state("done")), ("b".into(), state("idle"))]
        );
        for _ in 0..STANDINGS {
            store.record_standing("b", &state("idle")).unwrap();
        }
        assert_eq!(store.standings_kept().unwrap(), (4, STANDINGS + 3));
    }

    #[test]
    fn a_message_to_no_known_session_is_refused() {
        let (_dir, mut store) = store();
        let a = register(&mut store, None, "a");

        assert_eq!(
            store.send(a, SessionId::new(), "hello").unwrap_err().code,
            Code::NotFound
        );
    }

    #[test]
    fn a_message_without_words_is_refused() {
        let (_dir, mut store) = store();
        let a = register(&mut store, None, "a");
        let b = register(&mut store, None, "b");

        assert_eq!(store.send(a, b, "  ").unwrap_err().code, Code::Invalid);
    }

    #[test]
    fn ending_a_session_leaves_the_sessions_it_opened_running() {
        let (_dir, mut store) = store();
        let opener = register(&mut store, None, "opener");
        let opened = register(&mut store, Some(opener), "opened");

        store.end(opener).unwrap();

        assert!(!store.session(opened).unwrap().unwrap().ended);
    }

    #[test]
    fn the_session_that_opened_another_hears_nothing_of_how_it_starts_or_ends() {
        let (_dir, mut store) = store();
        let opener = register(&mut store, None, "opener");
        let opened = register(&mut store, Some(opener), "opened");

        assert!(store.set_readiness(opened, Readiness::Waiting).unwrap());
        assert!(!store.set_readiness(opened, Readiness::Waiting).unwrap());
        assert!(store.set_readiness(opened, Readiness::Ready).unwrap());
        store.end(opened).unwrap();

        assert!(store.messages_of(opener).unwrap().is_empty());
    }

    #[test]
    fn however_many_sessions_opened_by_sessions_run_one_more_is_recorded() {
        let (_dir, mut store) = store();
        let opener = register(&mut store, None, "opener");
        let other = register(&mut store, None, "in another repository");
        for n in 0..8 {
            register(&mut store, Some(other), &format!("other {n}"));
        }

        assert!(store.register(new(Some(opener), "one more")).is_ok());
    }

    #[test]
    fn what_a_session_sent_and_received_is_listed_newest_first() {
        let (_dir, mut store) = store();
        let a = register(&mut store, None, "a");
        let b = register(&mut store, None, "b");
        let c = register(&mut store, None, "c");
        store.send(a, b, "to b").unwrap();
        store.send(c, a, "to a").unwrap();
        store.send(b, c, "not a's").unwrap();

        let listed: Vec<_> = store
            .messages_of(a)
            .unwrap()
            .into_iter()
            .map(|message| message.body)
            .collect();

        assert_eq!(listed, ["to a", "to b"]);
    }

    #[test]
    fn a_session_keeps_the_settings_it_was_launched_with() {
        let (_dir, mut store) = store();
        let mut launch = Settings::default();
        launch
            .profile_of_mut(Agent::Claude)
            .claude_mut()
            .unwrap()
            .model = "opus".into();
        let (id, _) = store
            .register(NewSession {
                launch,
                ..new(None, "opener")
            })
            .unwrap();

        let kept = store.launch(id).unwrap().unwrap();
        assert_eq!(
            kept.profile_of(Agent::Claude).claude().unwrap().model,
            "opus"
        );
    }

    #[test]
    fn a_database_written_by_a_newer_adroit_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        let store = Store::open(&path).unwrap();
        store
            .connection
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();

        assert_eq!(
            store.ensure_current_schema().unwrap_err().code,
            Code::Unsupported
        );
        drop(store);
        assert_eq!(Store::open(&path).err().unwrap().code, Code::Unsupported);
    }

    #[test]
    fn a_session_keeps_the_profile_and_the_folder_it_started_from() {
        let (_dir, mut store) = store();
        let (id, _) = store
            .register(NewSession {
                agent: Agent::Codex,
                profile: "codex-review".parse().unwrap(),
                home: Some("/w".into()),
                ..new(None, "review")
            })
            .unwrap();

        let kept = store.session(id).unwrap().unwrap();
        assert_eq!(
            (kept.profile.as_str(), kept.home.as_deref()),
            ("codex-review", Some(Path::new("/w")))
        );
        assert_eq!(store.sessions().unwrap()[0], kept);
    }

    /// The tables as adroit created them before it recorded profiles.
    const SCHEMA_1: &str = "
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    parent TEXT REFERENCES sessions(id),
    agent TEXT NOT NULL,
    name TEXT NOT NULL,
    cwd TEXT NOT NULL,
    launch TEXT NOT NULL,
    terminal TEXT,
    conversation TEXT,
    readiness TEXT NOT NULL,
    started_ms INTEGER NOT NULL,
    reported INTEGER NOT NULL DEFAULT 0,
    ended INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX sessions_by_parent ON sessions(parent);
CREATE TABLE messages (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    sender TEXT REFERENCES sessions(id),
    recipient TEXT NOT NULL REFERENCES sessions(id),
    kind TEXT NOT NULL,
    body TEXT NOT NULL,
    sent_ms INTEGER NOT NULL,
    delivered INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX undelivered ON messages(recipient, delivered);
PRAGMA user_version = 1;
";

    #[test]
    fn a_database_from_before_profiles_were_recorded_reads_each_session_in_its_agent_s_own_environment()
     {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        let (claude, codex) = (SessionId::new(), SessionId::new());
        {
            let old = Connection::open(&path).unwrap();
            old.execute_batch(SCHEMA_1).unwrap();
            // Launches kept then name each agent's tables, not profiles.
            let launch = r#"{"agent":"codex","claude":{"command":[],"via_shell":true,"env":{}},"codex_sessions":{"model":"m","reasoning_effort":"","approval_policy":"","sandbox_mode":""}}"#;
            for (id, agent) in [(claude, "claude"), (codex, "codex")] {
                old.execute(
                    "INSERT INTO sessions (id, token, agent, name, cwd, launch, readiness, started_ms)
                     VALUES (?1, ?2, ?3, 'old', '/repo', ?4, 'ready', 1)",
                    params![id.to_string(), format!("token-{agent}"), agent, launch],
                )
                .unwrap();
            }
            old.execute(
                "INSERT INTO messages (id, sender, recipient, kind, body, sent_ms) VALUES (?1, ?2, ?3, 'message', 'kept', 1)",
                params![MessageId::new().to_string(), claude.to_string(), codex.to_string()],
            )
            .unwrap();
        }

        let mut store = Store::open(&path).unwrap();

        store.ensure_current_schema().unwrap();
        for (id, agent) in [(claude, Agent::Claude), (codex, Agent::Codex)] {
            let session = store.session(id).unwrap().unwrap();
            assert_eq!(session.profile, ProfileId::builtin(agent));
            assert_eq!(session.home, None);
        }
        let launch = store.launch(codex).unwrap().unwrap();
        assert_eq!(launch.default_agent(), Agent::Codex);
        assert_eq!(launch.profile_of(Agent::Codex).codex().unwrap().model, "m");
        assert!(launch.profile_of(Agent::Claude).launch.via_shell);
        assert_eq!(bodies(&mut store, codex), ["kept"]);
        assert_eq!(store.authenticate("token-claude").unwrap(), claude);
        // Opened again, it is as it was left.
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.sessions().unwrap().len(), 2);
    }

    #[test]
    fn a_database_from_before_profiles_owned_their_logins_reads_each_session_in_the_folder_its_connection_named()
     {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        let (work, own, gone) = (SessionId::new(), SessionId::new(), SessionId::new());
        {
            let old = Connection::open(&path).unwrap();
            old.execute_batch(SCHEMA_1).unwrap();
            old.execute_batch(MIGRATIONS[0]).unwrap();
            old.pragma_update(None, "user_version", 2).unwrap();
            // Launches kept then define the connections their profiles name.
            let launch = r#"{"profile":"work","profiles":[{"id":"claude","agent":"claude"},{"id":"work","agent":"claude","connection":"work"}],"connections":[{"id":"work","name":"Work","agent":"claude","home":"/Users/me/work"}]}"#;
            for (id, profile, connection) in [
                (work, "work", "work"),
                (own, "claude", "claude"),
                (gone, "work", "elsewhere"),
            ] {
                old.execute(
                    "INSERT INTO sessions (id, token, agent, name, cwd, launch, readiness, started_ms, profile, connection)
                     VALUES (?1, ?2, 'claude', 'old', '/repo', ?3, 'ready', 1, ?4, ?5)",
                    params![id.to_string(), format!("token-{id}"), launch, profile, connection],
                )
                .unwrap();
            }
        }

        let store = Store::open(&path).unwrap();

        store.ensure_current_schema().unwrap();
        let home = |id| store.session(id).unwrap().unwrap().home;
        assert_eq!(home(work).as_deref(), Some(Path::new("/Users/me/work")));
        assert_eq!(home(own), None);
        // A connection its launch does not define names no folder.
        assert_eq!(home(gone), None);
        let launch = store.launch(work).unwrap().unwrap();
        assert_eq!(
            launch.profiles[1].home.as_deref(),
            Some(Path::new("/Users/me/work"))
        );
    }
    #[test]
    fn a_database_from_when_sessions_reported_to_their_starters_keeps_reports_as_messages_and_drops_adroit_s_notices()
     {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        let (opener, opened) = (SessionId::new(), SessionId::new());
        {
            let old = Connection::open(&path).unwrap();
            old.execute_batch(SCHEMA_1).unwrap();
            old.execute_batch(MIGRATIONS[0]).unwrap();
            old.execute_batch(MIGRATIONS[1]).unwrap();
            old.pragma_update(None, "user_version", 3).unwrap();
            for (id, parent, reported) in [(opener, None, 0), (opened, Some(opener), 1)] {
                old.execute(
                    "INSERT INTO sessions (id, token, parent, agent, name, cwd, launch, readiness, started_ms, reported, profile)
                     VALUES (?1, ?2, ?3, 'claude', 'old', '/repo', '{}', 'ready', 1, ?4, 'claude')",
                    params![id.to_string(), format!("token-{id}"), parent.map(|p: SessionId| p.to_string()), reported],
                )
                .unwrap();
            }
            for (sender, kind, body) in [
                (Some(opened), "report", "done"),
                (None, "notice", "child is ready."),
            ] {
                old.execute(
                    "INSERT INTO messages (id, sender, recipient, kind, body, sent_ms) VALUES (?1, ?2, ?3, ?4, ?5, 1)",
                    params![MessageId::new().to_string(), sender.map(|s| s.to_string()), opener.to_string(), kind, body],
                )
                .unwrap();
            }
        }

        let store = Store::open(&path).unwrap();

        store.ensure_current_schema().unwrap();
        let kept = store.messages_of(opener).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(
            (&kept[0].sender, kept[0].body.as_str()),
            (&Sender::Session(opened), "done")
        );
        assert_eq!(store.session(opened).unwrap().unwrap().parent, Some(opener));
    }
    #[test]
    fn a_session_keeps_the_model_and_the_permissions_it_started_with() {
        let (_dir, mut store) = store();
        let (id, _) = store
            .register(NewSession {
                model: Some("opus".into()),
                permissions: AgentPermissions::ReadOnly,
                ..new(None, "reader")
            })
            .unwrap();

        let kept = store.known(id).unwrap();
        assert_eq!(kept.model.as_deref(), Some("opus"));
        assert_eq!(kept.permissions, AgentPermissions::ReadOnly);
    }

    #[test]
    fn a_session_keeps_whether_a_person_gave_its_name() {
        let (_dir, mut store) = store();
        let (given, _) = store
            .register(NewSession {
                name_given: true,
                ..new(None, "fixer")
            })
            .unwrap();
        let made = register(&mut store, None, "fix-tests");

        assert!(store.known(given).unwrap().name_given);
        assert!(!store.known(made).unwrap().name_given);
    }

    #[test]
    fn a_session_renamed_keeps_its_new_name_and_who_gave_it() {
        let (_dir, mut store) = store();
        let id = register(&mut store, None, "fix-tests");
        store.attach(id, 7, None).unwrap();

        store.rename(id, "テストの修正", false).unwrap();
        let kept = store.known(id).unwrap();
        assert_eq!(
            (kept.name.as_str(), kept.name_given),
            ("テストの修正", false)
        );

        store.rename_terminal(7, "fixer", true).unwrap();
        let kept = store.known(id).unwrap();
        assert_eq!((kept.name.as_str(), kept.name_given), ("fixer", true));
        // A terminal no session is recorded in renames nothing.
        store.rename_terminal(8, "other", true).unwrap();
    }

    #[test]
    fn a_name_handed_back_is_kept_as_one_its_agent_may_change() {
        let (_dir, mut store) = store();
        let id = register(&mut store, None, "fix-tests");
        store.attach(id, 7, None).unwrap();
        store.rename_terminal(7, "fixer", true).unwrap();

        store.hand_back_name(id).unwrap();

        let kept = store.known(id).unwrap();
        assert_eq!((kept.name.as_str(), kept.name_given), ("fixer", false));
    }

    #[test]
    fn a_database_from_before_names_were_told_apart_reads_every_name_as_made() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("delegation/state.sqlite3");
        let id = {
            let mut store = Store::open(&path).unwrap();
            let id = register(&mut store, None, "fixer");
            store
                .connection
                .execute_batch(
                    "DROP INDEX sessions_by_code; ALTER TABLE sessions DROP COLUMN code;
                     ALTER TABLE sessions DROP COLUMN name_given; DROP TABLE timeline; DROP TABLE standings; DROP TABLE requesters; DROP TABLE relays; ALTER TABLE sessions DROP COLUMN wrote_ms; ALTER TABLE messages DROP COLUMN delivered_ms; ALTER TABLE messages DROP COLUMN outside; PRAGMA user_version = 5;",
                )
                .unwrap();
            id
        };

        let store = Store::open(&path).unwrap();

        assert!(!store.known(id).unwrap().name_given);
    }

    #[test]
    fn a_session_keeps_the_terminal_host_its_terminal_ran_in() {
        let (_dir, mut store) = store();
        let id = register(&mut store, None, "a");
        store.attach(id, 7, Some("gen-1".into())).unwrap();

        let kept = store.known(id).unwrap();
        assert_eq!(kept.terminal, Some(7));
        assert_eq!(kept.generation.as_deref(), Some("gen-1"));
    }

    #[test]
    fn an_interrupted_session_has_ended_and_waits_to_be_brought_back() {
        let (_dir, mut store) = store();
        let (id, token) = store.register(new(None, "a")).unwrap();
        let other = register(&mut store, None, "b");
        store.interrupt(id).unwrap();
        store.end(other).unwrap();

        let kept = store.known(id).unwrap();
        assert!(kept.ended && kept.interrupted);
        assert_eq!(store.authenticate(&token).unwrap_err().code, Code::Refused);
        let waiting: Vec<_> = store
            .interrupted()
            .unwrap()
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(waiting, [id]);
    }

    #[test]
    fn a_session_that_already_ended_is_not_interrupted_after() {
        let (_dir, mut store) = store();
        let id = register(&mut store, None, "a");
        store.end(id).unwrap();
        store.interrupt(id).unwrap();

        assert!(!store.known(id).unwrap().interrupted);
    }

    #[test]
    fn an_interrupted_session_brought_back_or_let_go_waits_no_more() {
        let (_dir, mut store) = store();
        let back = register(&mut store, None, "back");
        let gone = register(&mut store, None, "gone");
        store.interrupt(back).unwrap();
        store.interrupt(gone).unwrap();
        let by = register(&mut store, None, "back");

        store.brought_back(back, by).unwrap();
        store.let_go(gone).unwrap();

        assert!(store.interrupted().unwrap().is_empty());
        assert_eq!(store.known(back).unwrap().brought_back_by, Some(by));
        assert!(store.known(gone).unwrap().ended);
    }

    #[test]
    fn a_terminal_is_kept_until_it_ends_and_one_interrupted_waits() {
        let (_dir, mut store) = store();
        let shell = TerminalSession {
            id: 1,
            name: "shell".into(),
            cwd: "/repo".into(),
            ..Default::default()
        };
        let nvim = TerminalSession {
            id: 2,
            name: "nvim".into(),
            cwd: "/repo".into(),
            program: TerminalProgram::Neovim,
            file: Some("/repo/a.rs".into()),
            ..Default::default()
        };
        let gone = TerminalSession {
            id: 3,
            ..shell.clone()
        };
        for terminal in [&shell, &nvim, &gone] {
            store
                .open_terminal(terminal, Some("gen-1".into()), 1)
                .unwrap();
        }

        store.interrupt_terminal(2).unwrap();
        store.end_terminal(3).unwrap();

        let running: Vec<_> = store
            .terminals()
            .unwrap()
            .into_iter()
            .map(|t| t.terminal.id)
            .collect();
        assert_eq!(running, [1]);
        let waiting = store.interrupted_terminals().unwrap();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].terminal, nvim);
        assert_eq!(waiting[0].generation.as_deref(), Some("gen-1"));
        store.reopened_terminal(2).unwrap();
        assert!(store.interrupted_terminals().unwrap().is_empty());
    }

    #[test]
    fn a_database_from_before_sessions_were_interrupted_reads_each_session_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        let id = SessionId::new();
        {
            let old = Connection::open(&path).unwrap();
            old.execute_batch(SCHEMA_1).unwrap();
            for step in &MIGRATIONS[..3] {
                old.execute_batch(step).unwrap();
            }
            old.pragma_update(None, "user_version", 4).unwrap();
            old.execute(
                "INSERT INTO sessions (id, token, agent, name, cwd, launch, terminal, readiness, started_ms, profile)
                 VALUES (?1, 'token', 'claude', 'old', '/repo', '{}', ?2, 'ready', 1, 'claude')",
                [id.to_string(), crate::sessions::hex(7)],
            )
            .unwrap();
        }

        let store = Store::open(&path).unwrap();

        store.ensure_current_schema().unwrap();
        let kept = store.known(id).unwrap();
        assert_eq!(
            (kept.terminal, kept.generation, kept.interrupted),
            (Some(7), None, false)
        );
        assert_eq!(
            (kept.model, kept.permissions),
            (None, AgentPermissions::Default)
        );
        assert!(store.terminals().unwrap().is_empty());
    }

    #[test]
    fn every_session_is_given_a_code_no_other_session_has() {
        let (_dir, mut store) = store();
        let ids: Vec<_> = (0..200)
            .map(|n| register(&mut store, None, &n.to_string()))
            .collect();

        let codes: std::collections::HashSet<_> = ids
            .iter()
            .map(|id| store.known(*id).unwrap().code)
            .collect();
        assert_eq!(codes.len(), ids.len());
    }

    #[test]
    fn a_session_keeps_its_code_through_renaming_and_ending() {
        let (_dir, mut store) = store();
        let id = register(&mut store, None, "first");
        let code = store.known(id).unwrap().code;

        store.rename(id, "second", true).unwrap();
        store.end(id).unwrap();

        assert_eq!(store.known(id).unwrap().code, code);
    }

    #[test]
    fn a_session_recorded_with_a_code_it_had_keeps_that_code() {
        let (_dir, mut store) = store();
        let old = register(&mut store, None, "old");
        let code = store.known(old).unwrap().code;
        store.interrupt(old).unwrap();

        let (by, _) = store
            .register(NewSession {
                code: Some(code.clone()),
                ..new(None, "old")
            })
            .unwrap();

        assert_eq!(store.known(by).unwrap().code, code);
    }

    #[test]
    fn a_code_is_never_drawn_again_once_given() {
        let (_dir, mut store) = store();
        let taken: SessionCode = "k7f3".parse().unwrap();
        let first = store
            .register(NewSession {
                code: Some(taken.clone()),
                ..new(None, "first")
            })
            .unwrap()
            .0;
        store.end(first).unwrap();

        let drawn = store.draw_code(|| taken.clone()).unwrap_err();

        assert_eq!(drawn.code, Code::Storage);
    }

    #[test]
    fn the_code_of_a_conversation_is_that_of_the_session_that_last_held_it() {
        let (_dir, mut store) = store();
        let older = register(&mut store, None, "older");
        store.learn_conversation(older, "conv").unwrap();
        let newer = register(&mut store, None, "newer");
        store.learn_conversation(newer, "conv").unwrap();

        assert_eq!(
            store.code_of_conversation("conv").unwrap(),
            Some(store.known(newer).unwrap().code)
        );
        assert_eq!(store.code_of_conversation("other").unwrap(), None);
    }

    #[test]
    fn a_session_is_found_by_its_code_as_the_record_that_stands_for_it_now() {
        let (_dir, mut store) = store();
        let old = register(&mut store, None, "old");
        let code = store.known(old).unwrap().code;
        store.interrupt(old).unwrap();
        let (by, _) = store
            .register(NewSession {
                code: Some(code.clone()),
                ..new(None, "old")
            })
            .unwrap();
        store.brought_back(old, by).unwrap();

        assert_eq!(store.by_code(&code).unwrap().map(|s| s.id), Some(by));
        assert_eq!(store.by_code(&"zzzz".parse().unwrap()).unwrap(), None);
    }

    #[test]
    fn a_database_from_before_codes_gives_every_session_one_and_a_brought_back_one_its_old_code() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("delegation/state.sqlite3");
        let (old, by, other) = {
            let mut store = Store::open(&path).unwrap();
            let old = register(&mut store, None, "old");
            store.interrupt(old).unwrap();
            let by = register(&mut store, None, "old");
            store.brought_back(old, by).unwrap();
            let other = register(&mut store, None, "other");
            store
                .connection
                .execute_batch(
                    "DROP TABLE timeline; DROP TABLE standings; DROP TABLE requesters; DROP TABLE relays; ALTER TABLE sessions DROP COLUMN wrote_ms; ALTER TABLE messages DROP COLUMN delivered_ms; ALTER TABLE messages DROP COLUMN outside; DROP INDEX sessions_by_code; ALTER TABLE sessions DROP COLUMN code; PRAGMA user_version = 6;",
                )
                .unwrap();
            (old, by, other)
        };

        let store = Store::open(&path).unwrap();

        let code = |id| store.known(id).unwrap().code;
        assert_eq!(code(old), code(by));
        assert_ne!(code(old), code(other));
    }

    fn entry(at_ms: u64, id: &str) -> Entry {
        Entry {
            at_ms,
            ops: vec![adroit_model::timeline::Op::Remove { id: id.into() }],
        }
    }

    #[test]
    fn a_session_s_timeline_keeps_every_call_in_the_order_made() {
        let (_dir, mut store) = store();
        let one = register(&mut store, None, "one");
        let one = store.known(one).unwrap().code;
        let two = register(&mut store, None, "two");
        let two = store.known(two).unwrap().code;

        store.record_timeline(&one, &entry(2, "b")).unwrap();
        store.record_timeline(&two, &entry(1, "x")).unwrap();
        store.record_timeline(&one, &entry(1, "a")).unwrap();

        assert_eq!(
            store.timeline_of(&one).unwrap(),
            [entry(2, "b"), entry(1, "a")]
        );
        assert_eq!(store.timeline_of(&two).unwrap(), [entry(1, "x")]);
    }

    #[test]
    fn a_session_brought_back_goes_on_with_the_timeline_of_the_one_it_stands_for() {
        let (_dir, mut store) = store();
        let old = register(&mut store, None, "old");
        let code = store.known(old).unwrap().code;
        store.record_timeline(&code, &entry(1, "a")).unwrap();
        store.interrupt(old).unwrap();

        let (by, _) = store
            .register(NewSession {
                code: Some(code.clone()),
                ..new(None, "old")
            })
            .unwrap();
        store.brought_back(old, by).unwrap();

        let code = store.known(by).unwrap().code;
        assert_eq!(store.timeline_of(&code).unwrap(), [entry(1, "a")]);
    }
}
