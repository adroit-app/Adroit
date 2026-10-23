//! Who started whom and what sessions told each other, kept by the session
//! daemon so both outlive the window and the sessions.
use std::fs::{DirBuilder, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use adroit_model::Agent;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::{Code, Error, Message, MessageId, MessageKind, Readiness, Result, Session, SessionId};
use crate::settings::Settings;

const SCHEMA_VERSION: i64 = 1;

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

/// What the window or the daemon knows of a session before its program starts.
#[derive(Debug, Clone)]
pub struct NewSession {
    pub parent: Option<SessionId>,
    pub agent: Agent,
    pub name: String,
    pub cwd: PathBuf,
    pub launch: Settings,
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
        let schema: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if schema > SCHEMA_VERSION {
            return Err(newer(schema));
        }
        if schema == 0 {
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
            transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            transaction.commit()?;
        }
        Ok(Self { connection })
    }

    /// Fails once a newer adroit has upgraded the database this one opened.
    pub fn ensure_current_schema(&self) -> Result<()> {
        let schema: i64 = self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        if schema > SCHEMA_VERSION {
            return Err(newer(schema));
        }
        Ok(())
    }

    /// Records a session and returns the identity and credential it acts with.
    pub fn register(&mut self, new: NewSession) -> Result<(SessionId, String)> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(parent) = new.parent {
            let starter = session(&transaction, parent)?
                .ok_or_else(|| Error::new(Code::NotFound, "the starting session is unknown"))?;
            if starter.parent.is_some() {
                return Err(Error::new(
                    Code::Refused,
                    "a session started by another session cannot start sessions; tell the session that started you how to divide the work",
                ));
            }
            if starter.ended {
                return Err(Error::new(Code::Refused, "the starting session has ended"));
            }
        }
        let id = SessionId::new();
        let token = format!("{}{}", SessionId::new(), SessionId::new());
        transaction.execute(
            "INSERT INTO sessions (id, token, parent, agent, name, cwd, launch, readiness, started_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id.to_string(),
                token,
                new.parent.map(|parent| parent.to_string()),
                new.agent.key(),
                new.name,
                new.cwd.to_string_lossy(),
                serde_json::to_string(&new.launch).map_err(storage)?,
                Readiness::Starting.key(),
                new.started_ms as i64,
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
            .map(|launch| serde_json::from_str(&launch).map_err(storage))
            .transpose()
    }

    pub fn attach(&mut self, id: SessionId, terminal: u128) -> Result<()> {
        self.update(
            id,
            "UPDATE sessions SET terminal = ?2 WHERE id = ?1",
            crate::sessions::hex(terminal),
        )
    }

    pub fn learn_conversation(&mut self, id: SessionId, conversation: &str) -> Result<()> {
        self.update(
            id,
            "UPDATE sessions SET conversation = ?2 WHERE id = ?1",
            conversation.to_owned(),
        )
    }

    /// Records how `id` stands while it starts, telling its starter when that
    /// changed, and returns whether it did.
    pub fn set_readiness(&mut self, id: SessionId, readiness: Readiness) -> Result<bool> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = session(&transaction, id)?
            .ok_or_else(|| Error::new(Code::NotFound, "the session is unknown"))?;
        if current.readiness == readiness || current.ended {
            return Ok(false);
        }
        transaction.execute(
            "UPDATE sessions SET readiness = ?2 WHERE id = ?1",
            params![id.to_string(), readiness.key()],
        )?;
        if let Some(parent) = current.parent {
            let body = match readiness {
                Readiness::Starting => None,
                Readiness::Ready => Some(format!("{} is ready.", current.name)),
                Readiness::Waiting => Some(format!(
                    "{} waits on a dialog in its console for the user to answer.",
                    current.name
                )),
                Readiness::Stalled => Some(format!(
                    "{} has not become ready and shows no dialog; it may have stalled.",
                    current.name
                )),
            };
            if let Some(body) = body
                && !ended(&transaction, parent)?
            {
                insert(&transaction, None, parent, MessageKind::Notice, &body)?;
            }
        }
        transaction.commit()?;
        Ok(true)
    }

    /// Ends `id`; a started session that never reported tells its starter so.
    pub fn end(&mut self, id: SessionId) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(current) = session(&transaction, id)? else {
            return Err(Error::new(Code::NotFound, "the session is unknown"));
        };
        if current.ended {
            return Ok(());
        }
        transaction.execute(
            "UPDATE sessions SET ended = 1 WHERE id = ?1",
            [id.to_string()],
        )?;
        if let Some(parent) = current.parent
            && !current.reported
            && !ended(&transaction, parent)?
        {
            let body = format!("{} ended without reporting.", current.name);
            insert(&transaction, None, parent, MessageKind::Notice, &body)?;
        }
        transaction.commit()?;
        Ok(())
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
        let id = insert(&transaction, Some(from), to, MessageKind::Message, body)?;
        transaction.commit()?;
        Ok(id)
    }

    /// Sends `from`'s report to its starter, which is kept even when the
    /// starter has ended, and marks `from` as reported.
    pub fn report(&mut self, from: SessionId, body: &str) -> Result<MessageId> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let sender = session(&transaction, from)?
            .ok_or_else(|| Error::new(Code::NotFound, "the session is unknown"))?;
        let Some(parent) = sender.parent else {
            return Err(Error::new(
                Code::Refused,
                "this session was started by the user and has no session to report to",
            ));
        };
        if sender.reported {
            return Err(Error::new(
                Code::Refused,
                "this session has already reported",
            ));
        }
        let id = insert(&transaction, Some(from), parent, MessageKind::Report, body)?;
        transaction.execute(
            "UPDATE sessions SET reported = 1 WHERE id = ?1",
            [from.to_string()],
        )?;
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

    /// Sessions started by sessions that have not ended.
    pub fn running_started(&self) -> Result<usize> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM sessions WHERE parent IS NOT NULL AND ended = 0",
            [],
            |row| row.get(0),
        )?;
        Ok(count as usize)
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

const SESSION_COLUMNS: &str =
    "id, parent, agent, name, cwd, terminal, conversation, readiness, started_ms, reported, ended";

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
    let reported = row.get(9)?;
    let ended = row.get(10)?;
    Ok((|| {
        Ok(Session {
            id: id.parse().map_err(storage)?,
            parent: parent
                .map(|parent| parent.parse())
                .transpose()
                .map_err(storage)?,
            agent: Agent::ALL
                .into_iter()
                .find(|known| known.key() == agent)
                .ok_or_else(|| storage(format!("unknown agent {agent}")))?,
            name,
            cwd: PathBuf::from(cwd),
            terminal: terminal.as_deref().and_then(crate::sessions::id_of),
            conversation,
            readiness: Readiness::from_key(&readiness)
                .ok_or_else(|| storage(format!("unknown readiness {readiness}")))?,
            started_ms: started_ms as u64,
            reported,
            ended,
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

fn ended(connection: &Connection, id: SessionId) -> Result<bool> {
    Ok(session(connection, id)?.is_none_or(|session| session.ended))
}

const MESSAGE_COLUMNS: &str = "id, sender, recipient, kind, body, sent_ms, delivered";

fn read_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Message>> {
    let id: String = row.get(0)?;
    let sender: Option<String> = row.get(1)?;
    let recipient: String = row.get(2)?;
    let kind: String = row.get(3)?;
    let sent_ms: i64 = row.get(5)?;
    let body = row.get(4)?;
    let delivered = row.get(6)?;
    Ok((|| {
        Ok(Message {
            id: id.parse().map_err(storage)?,
            sender: sender
                .map(|sender| sender.parse())
                .transpose()
                .map_err(storage)?,
            recipient: recipient.parse().map_err(storage)?,
            kind: MessageKind::from_key(&kind)
                .ok_or_else(|| storage(format!("unknown message kind {kind}")))?,
            body,
            sent_ms: sent_ms as u64,
            delivered,
        })
    })())
}

fn insert(
    connection: &Connection,
    sender: Option<SessionId>,
    recipient: SessionId,
    kind: MessageKind,
    body: &str,
) -> Result<MessageId> {
    if body.trim().is_empty() {
        return Err(Error::new(Code::Invalid, "a message needs a body"));
    }
    let id = MessageId::new();
    connection.execute(
        "INSERT INTO messages (id, sender, recipient, kind, body, sent_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id.to_string(),
            sender.map(|sender| sender.to_string()),
            recipient.to_string(),
            kind.key(),
            body,
            super::now_ms() as i64,
        ],
    )?;
    Ok(id)
}

/// Creates the database privately, returning whether it was created now.
fn private_database(path: &Path) -> Result<bool> {
    let parent = path
        .parent()
        .ok_or_else(|| storage("the database path has no directory"))?;
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(storage)?;
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(path).map_err(storage)?;
            // SAFETY: geteuid only reads the current process identity.
            if !metadata.is_file() || metadata.uid() != unsafe { libc::geteuid() } {
                return Err(storage("the database must be a regular file of this user"));
            }
            Ok(false)
        }
        Err(error) => Err(storage(error)),
    }
}

fn newer(schema: i64) -> Error {
    Error::new(
        Code::Unsupported,
        format!(
            "a newer adroit has written this database (schema {schema}); restart the session daemon from the installed application"
        ),
    )
}

fn storage(error: impl std::fmt::Display) -> Error {
    Error::new(Code::Storage, error.to_string())
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        storage(format!("cannot update delegation state: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("delegation/state.sqlite3")).unwrap();
        (dir, store)
    }

    fn new(parent: Option<SessionId>, name: &str) -> NewSession {
        NewSession {
            parent,
            agent: Agent::Claude,
            name: name.into(),
            cwd: "/repo".into(),
            launch: Settings::default(),
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
    fn a_registered_session_acts_with_its_credential_and_no_other() {
        let (_dir, mut store) = store();
        let (id, token) = store.register(new(None, "lead")).unwrap();

        assert_eq!(store.authenticate(&token).unwrap(), id);
        assert_eq!(
            store.authenticate("forged").unwrap_err().code,
            Code::Refused
        );
    }

    #[test]
    fn a_session_that_has_ended_no_longer_acts() {
        let (_dir, mut store) = store();
        let (id, token) = store.register(new(None, "lead")).unwrap();
        store.end(id).unwrap();

        assert_eq!(store.authenticate(&token).unwrap_err().code, Code::Refused);
    }

    #[test]
    fn a_started_session_cannot_start_sessions() {
        let (_dir, mut store) = store();
        let lead = register(&mut store, None, "lead");
        let child = register(&mut store, Some(lead), "child");

        let refused = store.register(new(Some(child), "grandchild")).unwrap_err();

        assert_eq!(refused.code, Code::Refused);
        assert!(refused.message.contains("divide the work"), "{refused:?}");
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
        let lead = register(&mut store, None, "lead");
        let child = register(&mut store, Some(lead), "child");
        let other = register(&mut store, None, "other");

        store
            .send(child, other, "I am about to rebase main")
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
    fn a_report_goes_to_the_starter_once() {
        let (_dir, mut store) = store();
        let lead = register(&mut store, None, "lead");
        let child = register(&mut store, Some(lead), "child");

        store.report(child, "done: tests pass").unwrap();

        assert_eq!(bodies(&mut store, lead), ["done: tests pass"]);
        assert!(store.session(child).unwrap().unwrap().reported);
        assert_eq!(
            store.report(child, "again").unwrap_err().code,
            Code::Refused
        );
    }

    #[test]
    fn a_session_the_user_started_has_no_one_to_report_to() {
        let (_dir, mut store) = store();
        let lead = register(&mut store, None, "lead");

        assert_eq!(store.report(lead, "done").unwrap_err().code, Code::Refused);
    }

    #[test]
    fn a_report_to_a_starter_that_has_ended_is_kept() {
        let (_dir, mut store) = store();
        let lead = register(&mut store, None, "lead");
        let child = register(&mut store, Some(lead), "child");
        store.end(lead).unwrap();

        store.report(child, "done").unwrap();

        let kept = store.messages_of(lead).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].kind, MessageKind::Report);
        assert_eq!(store.next_undelivered(lead).unwrap(), None);
    }

    #[test]
    fn ending_a_session_leaves_the_sessions_it_started_running() {
        let (_dir, mut store) = store();
        let lead = register(&mut store, None, "lead");
        let child = register(&mut store, Some(lead), "child");

        store.end(lead).unwrap();

        assert!(!store.session(child).unwrap().unwrap().ended);
    }

    #[test]
    fn a_started_session_that_ends_without_reporting_tells_its_starter() {
        let (_dir, mut store) = store();
        let lead = register(&mut store, None, "lead");
        let silent = register(&mut store, Some(lead), "silent");
        let reporting = register(&mut store, Some(lead), "reporting");
        store.report(reporting, "done").unwrap();

        store.end(silent).unwrap();
        store.end(reporting).unwrap();

        assert_eq!(
            bodies(&mut store, lead),
            ["done", "silent ended without reporting."]
        );
    }

    #[test]
    fn a_starter_hears_how_a_started_session_stands_while_it_starts_and_only_when_that_changes() {
        let (_dir, mut store) = store();
        let lead = register(&mut store, None, "lead");
        let child = register(&mut store, Some(lead), "child");

        assert!(store.set_readiness(child, Readiness::Waiting).unwrap());
        assert!(!store.set_readiness(child, Readiness::Waiting).unwrap());
        assert!(store.set_readiness(child, Readiness::Ready).unwrap());

        let heard = bodies(&mut store, lead);
        assert_eq!(heard.len(), 2);
        assert!(heard[0].contains("waits on a dialog"), "{heard:?}");
        assert!(heard[1].contains("is ready"), "{heard:?}");
    }

    #[test]
    fn only_started_sessions_still_running_count_against_the_limit() {
        let (_dir, mut store) = store();
        let lead = register(&mut store, None, "lead");
        register(&mut store, None, "other lead");
        let first = register(&mut store, Some(lead), "first");
        register(&mut store, Some(lead), "second");
        store.end(first).unwrap();

        assert_eq!(store.running_started().unwrap(), 1);
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
        launch.sessions.model = "opus".into();
        let (id, _) = store
            .register(NewSession {
                launch,
                ..new(None, "lead")
            })
            .unwrap();

        assert_eq!(store.launch(id).unwrap().unwrap().sessions.model, "opus");
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
}
