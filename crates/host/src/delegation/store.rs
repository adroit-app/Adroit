//! Which session opened which and what sessions told each other, kept by the session
//! daemon so both outlive the window and the sessions.
use adroit_sys::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::fs::{DirBuilder, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

use adroit_model::profile::ProfileId;
use adroit_model::{Agent, AgentPermissions, TerminalSession};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::{Code, Error, Message, MessageId, Readiness, Result, Session, SessionId};
use crate::settings::Settings;

const SCHEMA_VERSION: i64 = 5;

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
];

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
        let id = SessionId::new();
        let token = format!("{}{}", SessionId::new(), SessionId::new());
        transaction.execute(
            "INSERT INTO sessions (id, token, parent, agent, profile, home, name, cwd, launch, readiness, started_ms, model, permissions)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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
        let id = insert(&transaction, from, to, body)?;
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
            "UPDATE messages SET delivered = 1 WHERE id = ?1",
            [id.to_string()],
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

const SESSION_COLUMNS: &str = "id, parent, agent, name, cwd, terminal, conversation, readiness, started_ms, ended, profile, home, model, permissions, generation, interrupted, brought_back_by";

fn read_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Session>> {
    let id: String = row.get(0)?;
    let parent: Option<String> = row.get(1)?;
    let agent: String = row.get(2)?;
    let cwd: String = row.get(4)?;
    let terminal: Option<String> = row.get(5)?;
    let readiness: String = row.get(7)?;
    let started_ms: i64 = row.get(8)?;
    let name = row.get(3)?;
    let conversation = row.get(6)?;
    let ended = row.get(9)?;
    let profile: String = row.get(10)?;
    let home: Option<String> = row.get(11)?;
    let model = row.get(12)?;
    let permissions: Option<String> = row.get(13)?;
    let generation = row.get(14)?;
    let interrupted = row.get(15)?;
    let brought_back_by: Option<String> = row.get(16)?;
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
        })
    })())
}

fn read_terminal(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<KeptTerminal>> {
    let id: String = row.get(0)?;
    let generation = row.get(1)?;
    let name = row.get(2)?;
    let cwd: String = row.get(3)?;
    let program: String = row.get(4)?;
    let file: Option<String> = row.get(5)?;
    let command: String = row.get(6)?;
    let started_ms: i64 = row.get(7)?;
    let resource_id: String = row.get(8)?;
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

const MESSAGE_COLUMNS: &str = "id, sender, recipient, body, sent_ms, delivered";

fn read_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Message>> {
    let id: String = row.get(0)?;
    let sender: String = row.get(1)?;
    let recipient: String = row.get(2)?;
    let sent_ms: i64 = row.get(4)?;
    let body = row.get(3)?;
    let delivered = row.get(5)?;
    Ok((|| {
        Ok(Message {
            id: id.parse().map_err(Error::storage)?,
            sender: sender.parse().map_err(Error::storage)?,
            recipient: recipient.parse().map_err(Error::storage)?,
            body,
            sent_ms: sent_ms as u64,
            delivered,
        })
    })())
}

fn insert(
    connection: &Connection,
    sender: SessionId,
    recipient: SessionId,
    body: &str,
) -> Result<MessageId> {
    if body.trim().is_empty() {
        return Err(Error::new(Code::Invalid, "a message needs a body"));
    }
    let id = MessageId::new();
    connection.execute(
        "INSERT INTO messages (id, sender, recipient, kind, body, sent_ms) VALUES (?1, ?2, ?3, 'message', ?4, ?5)",
        params![
            id.to_string(),
            sender.to_string(),
            recipient.to_string(),
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
    Error::new(
        Code::Unsupported,
        format!(
            "a newer adroit has written this database (schema {schema}); restart the work host from the installed application"
        ),
    )
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
        }
    }

    fn register(store: &mut Store, parent: Option<SessionId>, name: &str) -> SessionId {
        store.register(new(parent, name)).unwrap().0
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
        assert_eq!((kept[0].sender, kept[0].body.as_str()), (opened, "done"));
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
}
