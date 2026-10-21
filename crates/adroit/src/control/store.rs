//! Durable state changes, events and request responses in one transaction.

use std::fmt;
use std::fs::{DirBuilder, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use adroit_model::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

mod candidates;
mod checks;
mod completion;
mod context;
mod coordination;
mod dependencies;
mod lifecycle;
mod results;
mod runs;
mod runtime;
pub mod ui;
mod workareas;

const SCHEMA_VERSION: i64 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    NotFound,
    OwnershipMismatch,
    RevisionConflict,
    Conflict,
    UnsupportedSchema,
    CorruptStore,
    StorageFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlError {
    pub code: ErrorCode,
    pub message: String,
}

impl ControlError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ControlError {}
impl From<rusqlite::Error> for ControlError {
    fn from(error: rusqlite::Error) -> Self {
        let code = match &error {
            rusqlite::Error::SqliteFailure(error, _)
                if matches!(
                    error.code,
                    rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
                ) =>
            {
                ErrorCode::CorruptStore
            }
            _ => ErrorCode::StorageFailure,
        };
        Self::new(code, format!("cannot update orchestration state: {error}"))
    }
}

type Result<T> = std::result::Result<T, ControlError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply<T> {
    pub value: T,
    pub event_cursor: u64,
}

impl<T> Reply<T> {
    pub fn map<U>(self, project: impl FnOnce(T) -> U) -> Reply<U> {
        Reply {
            value: project(self.value),
            event_cursor: self.event_cursor,
        }
    }
}

pub struct Store {
    connection: Connection,
}

const SCHEMA: &str = "
CREATE TABLE logical_sessions (id TEXT PRIMARY KEY, data TEXT NOT NULL);
CREATE TABLE runs (
    id TEXT PRIMARY KEY,
    leader_id TEXT NOT NULL REFERENCES logical_sessions(id),
    state TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE UNIQUE INDEX one_active_run_per_leader ON runs(leader_id)
    WHERE state IN ('active', 'verifying');
CREATE TABLE tasks (id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(id), data TEXT NOT NULL);
CREATE INDEX tasks_by_run ON tasks(run_id);
CREATE TABLE task_dependencies (
    task_id TEXT NOT NULL REFERENCES tasks(id),
    dependency_id TEXT NOT NULL REFERENCES tasks(id),
    PRIMARY KEY(task_id, dependency_id),
    CHECK(task_id != dependency_id)
);
CREATE TABLE attempts (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    session_id TEXT NOT NULL UNIQUE REFERENCES logical_sessions(id),
    active INTEGER NOT NULL CHECK(active IN (0, 1)),
    data TEXT NOT NULL
);
CREATE UNIQUE INDEX one_active_attempt_per_task ON attempts(task_id) WHERE active = 1;
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(id),
    sender TEXT NOT NULL REFERENCES logical_sessions(id),
    recipient TEXT NOT NULL REFERENCES logical_sessions(id),
    data TEXT NOT NULL
);
CREATE TABLE deliveries (
    message_id TEXT NOT NULL REFERENCES messages(id),
    recipient TEXT NOT NULL REFERENCES logical_sessions(id),
    data TEXT NOT NULL,
    PRIMARY KEY(message_id, recipient)
);
CREATE TABLE requests (id TEXT PRIMARY KEY, body TEXT NOT NULL, response TEXT NOT NULL);
CREATE TABLE events (
    cursor INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT REFERENCES runs(id),
    kind TEXT NOT NULL,
    data TEXT NOT NULL
);
";

fn invalid(message: impl Into<String>) -> ControlError {
    ControlError::new(ErrorCode::InvalidRequest, message)
}
fn conflict(message: impl Into<String>) -> ControlError {
    ControlError::new(ErrorCode::Conflict, message)
}
fn encode(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|error| invalid(format!("cannot encode the request: {error}")))
}
fn decode<T: DeserializeOwned>(text: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|error| {
        ControlError::new(
            ErrorCode::CorruptStore,
            format!("cannot read orchestration state: {error}"),
        )
    })
}

#[derive(Clone, Copy)]
enum Table {
    Sessions,
    Runs,
    Tasks,
    Attempts,
    Messages,
    WorkAreas,
    RunWorkAreas,
    Reports,
    Completions,
    CompletionRequests,
    Candidates,
    CandidateChecks,
    Coordination,
    CoordinationReplies,
}
impl Table {
    fn name(self) -> &'static str {
        match self {
            Self::Sessions => "logical_sessions",
            Self::Runs => "runs",
            Self::Tasks => "tasks",
            Self::Attempts => "attempts",
            Self::Messages => "messages",
            Self::WorkAreas => "work_areas",
            Self::RunWorkAreas => "run_work_areas",
            Self::Reports => "task_reports",
            Self::Completions => "run_completions",
            Self::CompletionRequests => "completion_requests",
            Self::Candidates => "candidates",
            Self::CandidateChecks => "candidate_checks",
            Self::Coordination => "coordination_notices",
            Self::CoordinationReplies => "coordination_replies",
        }
    }
}
fn load<T: DeserializeOwned>(
    connection: &Connection,
    table: Table,
    id: impl ToString,
) -> Result<Option<T>> {
    let query = format!("SELECT data FROM {} WHERE id = ?1", table.name());
    let text: Option<String> = connection
        .query_row(&query, [id.to_string()], |row| row.get(0))
        .optional()?;
    text.as_deref().map(decode).transpose()
}
fn require<T: DeserializeOwned>(
    connection: &Connection,
    table: Table,
    id: impl ToString,
) -> Result<T> {
    let id = id.to_string();
    load(connection, table, &id)?.ok_or_else(|| {
        ControlError::new(
            ErrorCode::NotFound,
            format!("no {} record {id}", table.name()),
        )
    })
}
fn all<T: DeserializeOwned>(connection: &Connection, table: Table) -> Result<Vec<T>> {
    let query = format!("SELECT data FROM {} ORDER BY rowid", table.name());
    let mut statement = connection.prepare(&query)?;
    statement
        .query_map([], |row| row.get::<_, String>(0))?
        .map(|row| decode(&row?))
        .collect()
}
fn cursor(connection: &Connection) -> Result<u64> {
    let cursor: i64 =
        connection.query_row("SELECT COALESCE(MAX(cursor), 0) FROM events", [], |row| {
            row.get(0)
        })?;
    cursor
        .try_into()
        .map_err(|_| ControlError::new(ErrorCode::CorruptStore, "the event cursor is negative"))
}
fn event(
    connection: &Connection,
    run: Option<RunId>,
    kind: &str,
    data: &impl Serialize,
) -> Result<()> {
    connection.execute(
        "INSERT INTO events(run_id, kind, data) VALUES (?1, ?2, ?3)",
        params![run.map(|id| id.to_string()), kind, encode(data)?],
    )?;
    Ok(())
}
fn store_message(connection: &Connection, message: &AgentMessage) -> Result<()> {
    if load::<AgentMessage>(connection, Table::Messages, message.id)?.is_some() {
        return Err(conflict("the message ID is already in use"));
    }
    connection.execute(
        "INSERT INTO messages(id, run_id, sender, recipient, data) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            message.id.to_string(),
            message.run_id.to_string(),
            message.from.to_string(),
            message.to.to_string(),
            encode(message)?
        ],
    )?;
    let delivery = DeliveryRecord {
        message_id: message.id,
        recipient: message.to,
        state: DeliveryState::Stored,
        attempts: 0,
        last_error: None,
    };
    connection.execute(
        "INSERT INTO deliveries(message_id, recipient, data) VALUES (?1, ?2, ?3)",
        params![
            message.id.to_string(),
            message.to.to_string(),
            encode(&delivery)?
        ],
    )?;
    event(connection, Some(message.run_id), "message_stored", message)
}

fn save_session(connection: &Connection, session: &ManagedSession) -> Result<()> {
    connection.execute(
        "INSERT INTO logical_sessions(id, data) VALUES (?1, ?2)
        ON CONFLICT(id) DO UPDATE SET data = excluded.data",
        params![session.binding.logical_id().to_string(), encode(session)?],
    )?;
    Ok(())
}
fn save_run(connection: &Connection, run: &RunRecord) -> Result<()> {
    connection.execute(
        "INSERT INTO runs(id, leader_id, state, data) VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(id) DO UPDATE SET state = excluded.state, data = excluded.data",
        params![
            run.id.to_string(),
            run.leader_id.to_string(),
            run.state.word(),
            encode(run)?
        ],
    )?;
    Ok(())
}
fn save_task(connection: &Connection, task: &TaskRecord) -> Result<()> {
    connection.execute(
        "INSERT INTO tasks(id, run_id, data) VALUES (?1, ?2, ?3)
        ON CONFLICT(id) DO UPDATE SET data = excluded.data",
        params![task.id.to_string(), task.run_id.to_string(), encode(task)?],
    )?;
    connection.execute(
        "DELETE FROM task_dependencies WHERE task_id = ?1",
        [task.id.to_string()],
    )?;
    for dependency in &task.dependencies {
        connection.execute(
            "INSERT INTO task_dependencies(task_id, dependency_id) VALUES (?1, ?2)",
            params![task.id.to_string(), dependency.to_string()],
        )?;
    }
    Ok(())
}

fn save_attempt(connection: &Connection, attempt: &AttemptRecord) -> Result<()> {
    connection.execute(
        "UPDATE attempts SET active = ?1, data = ?2 WHERE id = ?3",
        params![
            attempt.state.holds_slot(),
            encode(attempt)?,
            attempt.id.to_string()
        ],
    )?;
    Ok(())
}

fn delivery(
    connection: &Connection,
    message: &AgentMessage,
    recipient: LogicalSessionId,
) -> Result<DeliveryRecord> {
    if message.to != recipient {
        return Err(ControlError::new(
            ErrorCode::OwnershipMismatch,
            "the message belongs to another recipient",
        ));
    }
    let text: String = connection.query_row(
        "SELECT data FROM deliveries WHERE message_id = ?1 AND recipient = ?2",
        params![message.id.to_string(), recipient.to_string()],
        |row| row.get(0),
    )?;
    decode(&text)
}

fn save_delivery(connection: &Connection, delivery: &DeliveryRecord) -> Result<()> {
    connection.execute(
        "UPDATE deliveries SET data = ?1 WHERE message_id = ?2 AND recipient = ?3",
        params![
            encode(delivery)?,
            delivery.message_id.to_string(),
            delivery.recipient.to_string()
        ],
    )?;
    Ok(())
}
fn reference(connection: &Connection, identity: SessionRef) -> Result<ManagedSession> {
    let session: ManagedSession = require(connection, Table::Sessions, identity.id)?;
    if session.generation != identity.generation {
        return Err(ControlError::new(
            ErrorCode::OwnershipMismatch,
            "the session generation is outdated",
        ));
    }
    Ok(session)
}
fn run_owner(connection: &Connection, identity: SessionRef, run_id: RunId) -> Result<RunRecord> {
    let session = reference(connection, identity)?;
    let run: RunRecord = require(connection, Table::Runs, run_id)?;
    if session.binding.role() != SessionRole::Leader
        || run.leader_id != identity.id
        || session.binding.run_id() != Some(run_id)
    {
        return Err(ControlError::new(
            ErrorCode::OwnershipMismatch,
            "the caller does not lead this run",
        ));
    }
    Ok(run)
}
fn owner(connection: &Connection, identity: SessionRef, run_id: RunId) -> Result<RunRecord> {
    let run = run_owner(connection, identity, run_id)?;
    if !matches!(run.state, RunState::Active | RunState::Verifying) {
        return Err(conflict("the run is not active"));
    }
    Ok(run)
}
fn version(
    connection: &Connection,
    identity: SessionRef,
    version: TaskVersion,
) -> Result<TaskRecord> {
    let task: TaskRecord = require(connection, Table::Tasks, version.task_id)?;
    owner(connection, identity, task.run_id)?;
    if task.revision != version.revision {
        return Err(ControlError::new(
            ErrorCode::RevisionConflict,
            "the task revision is outdated",
        ));
    }
    Ok(task)
}

fn private_database(path: &Path) -> Result<bool> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|error| ControlError::new(ErrorCode::StorageFailure, error.to_string()))?;
    let metadata = std::fs::symlink_metadata(parent)
        .map_err(|error| ControlError::new(ErrorCode::StorageFailure, error.to_string()))?;
    // SAFETY: geteuid only reads the calling process's identity.
    let uid = unsafe { libc::geteuid() };
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
        return Err(ControlError::new(
            ErrorCode::StorageFailure,
            "the control folder must be owned by the current user and private",
        ));
    }
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(path)
                .map_err(|error| ControlError::new(ErrorCode::StorageFailure, error.to_string()))?;
            if !metadata.is_file() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
                return Err(ControlError::new(
                    ErrorCode::StorageFailure,
                    "the control database must be a private regular file owned by the current user",
                ));
            }
            Ok(false)
        }
        Err(error) => Err(ControlError::new(
            ErrorCode::StorageFailure,
            error.to_string(),
        )),
    }
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let created = private_database(path)?;
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        let schema: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if schema > SCHEMA_VERSION {
            return Err(ControlError::new(
                ErrorCode::UnsupportedSchema,
                format!("control schema {schema} is newer than {SCHEMA_VERSION}"),
            ));
        }
        if schema == 0 {
            if !created {
                return Err(ControlError::new(
                    ErrorCode::CorruptStore,
                    "an existing database has no recognized control schema",
                ));
            }
            let tables: i64 = connection.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'", [], |row| row.get(0))?;
            if tables != 0 {
                return Err(ControlError::new(
                    ErrorCode::CorruptStore,
                    "the database has no recognized control schema",
                ));
            }
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(SCHEMA)?;
            transaction.pragma_update(None, "user_version", 1)?;
            transaction.commit()?;
        }
        if schema < 2 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch("CREATE TABLE session_credentials (session_id TEXT PRIMARY KEY REFERENCES logical_sessions(id), generation TEXT NOT NULL, token TEXT NOT NULL UNIQUE);")?;
            transaction.pragma_update(None, "user_version", 2)?;
            transaction.commit()?;
        }
        if schema < 3 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch("CREATE TABLE work_areas (id TEXT PRIMARY KEY, path TEXT NOT NULL UNIQUE, data TEXT NOT NULL);
                CREATE TABLE run_work_areas (run_id TEXT NOT NULL REFERENCES runs(id), work_area_id TEXT NOT NULL REFERENCES work_areas(id), is_default INTEGER NOT NULL CHECK(is_default IN (0, 1)), data TEXT NOT NULL, PRIMARY KEY(run_id, work_area_id));
                CREATE UNIQUE INDEX one_default_work_area_per_run ON run_work_areas(run_id) WHERE is_default = 1;")?;
            transaction.pragma_update(None, "user_version", 3)?;
            transaction.commit()?;
        }
        if schema < 4 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch("CREATE TABLE task_reports (id TEXT PRIMARY KEY REFERENCES messages(id), attempt_id TEXT NOT NULL UNIQUE REFERENCES attempts(id), data TEXT NOT NULL);")?;
            transaction.pragma_update(None, "user_version", 4)?;
            transaction.commit()?;
        }
        if schema < 5 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch("CREATE TABLE session_launches (session_id TEXT NOT NULL REFERENCES logical_sessions(id), generation TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(session_id,generation));")?;
            transaction.pragma_update(None, "user_version", 5)?;
            transaction.commit()?;
        }
        if schema < 6 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch("DROP INDEX one_active_run_per_leader; CREATE UNIQUE INDEX one_active_run_per_leader ON runs(leader_id) WHERE state IN ('active','verifying','cancelling');")?;
            transaction.pragma_update(None, "user_version", 6)?;
            transaction.commit()?;
        }
        if schema < 7 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch("CREATE TABLE run_completions (id TEXT PRIMARY KEY REFERENCES runs(id), data TEXT NOT NULL);")?;
            transaction.pragma_update(None, "user_version", 7)?;
            transaction.commit()?;
        }
        if schema < 8 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch("CREATE TABLE coordination_notices (id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(id), data TEXT NOT NULL); CREATE TABLE coordination_replies (id TEXT PRIMARY KEY, notice_id TEXT NOT NULL REFERENCES coordination_notices(id), data TEXT NOT NULL);")?;
            transaction.pragma_update(None, "user_version", 8)?;
            transaction.commit()?;
        }
        if schema < 9 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch("CREATE TABLE candidates (id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(id), source_task_id TEXT NOT NULL REFERENCES tasks(id), data TEXT NOT NULL);")?;
            transaction.pragma_update(None, "user_version", 9)?;
            transaction.commit()?;
        }
        if schema < 10 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch("CREATE INDEX candidates_state ON candidates(json_extract(data,'$.state')); CREATE INDEX candidates_source_state ON candidates(source_task_id,json_extract(data,'$.state'));")?;
            transaction.pragma_update(None, "user_version", 10)?;
            transaction.commit()?;
        }
        if schema < 11 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch("CREATE TABLE candidate_checks (id TEXT PRIMARY KEY REFERENCES attempts(id), candidate_id TEXT NOT NULL REFERENCES candidates(id), task_id TEXT NOT NULL REFERENCES tasks(id), report_id TEXT NOT NULL REFERENCES task_reports(id), state TEXT NOT NULL, data TEXT NOT NULL); CREATE INDEX candidate_checks_state ON candidate_checks(state); CREATE INDEX candidate_checks_candidate ON candidate_checks(candidate_id); CREATE INDEX candidate_checks_task ON candidate_checks(task_id);")?;
            transaction.pragma_update(None, "user_version", 11)?;
            transaction.commit()?;
        }
        if schema < 12 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch("CREATE TABLE completion_requests (id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(id), state TEXT NOT NULL, data TEXT NOT NULL); CREATE INDEX completion_requests_state ON completion_requests(state); CREATE UNIQUE INDEX completion_requests_pending_run ON completion_requests(run_id) WHERE state = 'pending';")?;
            transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            transaction.commit()?;
        }
        connection.pragma_update(None, "journal_mode", "WAL")?;
        Ok(Self { connection })
    }

    fn transact<T: Serialize + DeserializeOwned>(
        &mut self,
        request: RequestId,
        body: Value,
        operation: impl FnOnce(&Connection) -> Result<T>,
    ) -> Result<Reply<T>> {
        let body = encode(&body)?;
        let mut transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let kept: Option<(String, String)> = transaction
            .query_row(
                "SELECT body, response FROM requests WHERE id = ?1",
                [request.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((before, response)) = kept {
            if before != body {
                return Err(conflict("the request ID was reused with different content"));
            }
            return decode(&response)?;
        }
        let savepoint = transaction.savepoint()?;
        let result = match operation(&savepoint) {
            Ok(value) => {
                savepoint.commit()?;
                Ok(Reply {
                    value,
                    event_cursor: cursor(&transaction)?,
                })
            }
            Err(error) => {
                drop(savepoint);
                Err(error)
            }
        };
        transaction.execute(
            "INSERT INTO requests(id, body, response) VALUES (?1, ?2, ?3)",
            params![request.to_string(), body, encode(&result)?],
        )?;
        transaction.commit()?;
        result
    }

    pub fn grant(&mut self, identity: SessionRef) -> Result<String> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        reference(&transaction, identity)?;
        let kept: Option<String> = transaction
            .query_row(
                "SELECT token FROM session_credentials WHERE session_id = ?1 AND generation = ?2",
                params![identity.id.to_string(), identity.generation.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let token = match kept {
            Some(token) => token,
            None => {
                let token = format!("{}{}", RequestId::new(), RequestId::new());
                transaction.execute("INSERT INTO session_credentials(session_id, generation, token) VALUES (?1, ?2, ?3) ON CONFLICT(session_id) DO UPDATE SET generation = excluded.generation, token = excluded.token",
                    params![identity.id.to_string(), identity.generation.to_string(), token])?;
                token
            }
        };
        transaction.commit()?;
        Ok(token)
    }

    pub fn authenticate(&self, token: &str) -> Result<SessionRef> {
        let found: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT session_id, generation FROM session_credentials WHERE token = ?1",
                [token],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (id, generation) = found.ok_or_else(|| {
            ControlError::new(ErrorCode::OwnershipMismatch, "invalid session credential")
        })?;
        let generation = generation.parse().map_err(|_| {
            ControlError::new(ErrorCode::CorruptStore, "invalid credential generation")
        })?;
        let identity = SessionRef {
            id: id.parse().map_err(|_| {
                ControlError::new(ErrorCode::CorruptStore, "invalid credential identity")
            })?,
            generation,
        };
        reference(&self.connection, identity)?;
        Ok(identity)
    }

    pub fn register_leader(
        &mut self,
        request: RequestId,
        id: LogicalSessionId,
        agent: Agent,
        cwd: PathBuf,
    ) -> Result<Reply<ManagedSession>> {
        self.register_leader_launch(request, id, agent, cwd, AgentPermissions::Default)
    }

    pub fn register_leader_launch(
        &mut self,
        request: RequestId,
        id: LogicalSessionId,
        agent: Agent,
        cwd: PathBuf,
        permissions: AgentPermissions,
    ) -> Result<Reply<ManagedSession>> {
        self.transact(
            request,
            json!({"op":"register_leader", "id":id, "agent":agent, "cwd":cwd, "permissions":permissions}),
            |connection| {
                if load::<ManagedSession>(connection, Table::Sessions, id)?.is_some() {
                    return Err(conflict("the logical session already exists"));
                }
                let session = ManagedSession {
                    binding: SessionBinding::leader(id, None),
                    agent,
                    generation: 1,
                    state: ManagedSessionState::Preparing,
                    permissions,
                    cwd: Some(cwd),
                    terminal_id: None,
                    conversation_id: None,
                };
                save_session(connection, &session)?;
                event(connection, None, "leader_registered", &session)?;
                Ok(session)
            },
        )
    }

    pub fn open_run(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        id: RunId,
        brief: RunBrief,
    ) -> Result<Reply<RunRecord>> {
        self.transact(request, json!({"op":"open_run", "caller":identity, "id":id, "brief":brief}), |connection| {
            let mut session = reference(connection, identity)?;
            if session.binding.role() != SessionRole::Leader {
                return Err(ControlError::new(ErrorCode::OwnershipMismatch, "only a leader can open a run"));
            }
            if brief.title.trim().is_empty() || brief.goal.trim().is_empty() { return Err(invalid("a run needs a title and goal")); }
            checks::validate_gate(&brief.quality_gate)?;
            if load::<RunRecord>(connection, Table::Runs, id)?.is_some() { return Err(conflict("the run already exists")); }
            let active: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM runs WHERE leader_id = ?1 AND state IN ('active', 'verifying', 'cancelling'))", [identity.id.to_string()], |row| row.get(0))?;
            if active { return Err(conflict("the leader already has an active run")); }
            let run = RunRecord { id, leader_id: identity.id, brief, plan_revision: 1, candidate_id: None, state: RunState::Active };
            save_run(connection, &run)?;
            session.binding = SessionBinding::leader(identity.id, Some(id));
            save_session(connection, &session)?;
            event(connection, Some(id), "run_opened", &run)?;
            Ok(run)
        })
    }

    pub fn put_task(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        plan: TaskPlan,
    ) -> Result<Reply<TaskRecord>> {
        self.transact(
            request,
            json!({"op":"put_task", "caller":identity, "plan":plan}),
            |connection| {
                let mut run = owner(connection, identity, plan.run_id)?;
                if plan.brief.instructions.trim().is_empty() {
                    return Err(invalid("a task needs instructions"));
                }
                candidates::task_candidate(
                    connection,
                    plan.run_id,
                    &plan.brief,
                    &plan.dependencies,
                    false,
                )?;
                let previous: Option<TaskRecord> = load(connection, Table::Tasks, plan.id)?;
                let (revision, current_attempt, state) = match previous {
                    Some(task) => {
                        if task.run_id != plan.run_id {
                            return Err(ControlError::new(
                                ErrorCode::OwnershipMismatch,
                                "the task belongs to another run",
                            ));
                        }
                        if plan.expected_revision != Some(task.revision) {
                            return Err(ControlError::new(
                                ErrorCode::RevisionConflict,
                                "the task revision is outdated",
                            ));
                        }
                        let current_attempt = match task.current_attempt {
                            Some(id) => {
                                let mut attempt: AttemptRecord =
                                    require(connection, Table::Attempts, id)?;
                                if attempt.state.holds_slot() {
                                    results::request_stop(
                                        connection,
                                        &task,
                                        &mut attempt,
                                        "The task instructions changed",
                                    )?;
                                    Some(id)
                                } else {
                                    None
                                }
                            }
                            None => None,
                        };
                        (
                            task.revision
                                .checked_add(1)
                                .ok_or_else(|| conflict("the task revision is exhausted"))?,
                            current_attempt,
                            TaskState::Planned,
                        )
                    }
                    None => {
                        if plan.expected_revision.is_some() {
                            return Err(ControlError::new(
                                ErrorCode::NotFound,
                                "the task does not exist; use expected_revision null when creating a new task",
                            ));
                        }
                        (1, None, TaskState::Planned)
                    }
                };
                let tasks: Vec<TaskRecord> = all(connection, Table::Tasks)?;
                validate_dependencies(&tasks, &plan).map_err(|error| invalid(error.to_string()))?;
                let task = TaskRecord {
                    id: plan.id,
                    run_id: plan.run_id,
                    revision,
                    brief: plan.brief,
                    dependencies: plan.dependencies,
                    state,
                    current_attempt,
                };
                save_task(connection, &task)?;
                dependencies::invalidate_dependents(connection, task.id)?;
                if let Some(candidate) = task.brief.candidate_id {
                    checks::refresh(connection, candidate)?;
                }
                run.plan_revision = run
                    .plan_revision
                    .checked_add(1)
                    .ok_or_else(|| conflict("the plan revision is exhausted"))?;
                save_run(connection, &run)?;
                event(connection, Some(run.id), "task_updated", &task)?;
                Ok(task)
            },
        )
    }

    pub fn queue_task(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        wanted: TaskVersion,
    ) -> Result<Reply<TaskRecord>> {
        self.transact(
            request,
            json!({"op":"queue_task", "caller":identity, "task":wanted}),
            |connection| {
                let mut task = version(connection, identity, wanted)?;
                if task.current_attempt.is_some()
                    || !matches!(task.state, TaskState::Planned | TaskState::Blocked)
                {
                    return Err(conflict("the task cannot be queued in its current state"));
                }
                task.state = TaskState::Queued;
                save_task(connection, &task)?;
                event(connection, Some(task.run_id), "task_queued", &task)?;
                Ok(task)
            },
        )
    }

    pub fn reserve_attempt(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        wanted: TaskVersion,
        target: TargetSelection,
    ) -> Result<Reply<AttemptRecord>> {
        self.transact(request, json!({"op":"reserve_attempt", "caller":identity, "task":wanted, "target":target}), |connection| {
            let mut task = version(connection, identity, wanted)?;
            if task.state != TaskState::Queued || task.current_attempt.is_some() { return Err(conflict("the task already has an execution or is not queued")); }
            candidates::task_candidate(connection,task.run_id,&task.brief,&task.dependencies,true)?;
            let mut input_versions = Vec::new();
            for dependency in &task.dependencies {
                let dependency: TaskRecord = require(connection, Table::Tasks, dependency)?;
                if dependency.state != TaskState::Succeeded { return Err(conflict("a task dependency has not succeeded")); }
                input_versions.push(TaskVersion {task_id:dependency.id,revision:dependency.revision});
            }
            let id = AttemptId::new();
            let session_id = LogicalSessionId::new();
            let session = ManagedSession {
                binding: SessionBinding::worker(session_id, identity.id, task.run_id, task.id, id)
                    .map_err(|error| invalid(error.to_string()))?,
                agent: target.agent, generation: 1, state: ManagedSessionState::Preparing, permissions: target.permissions.clone(), cwd: None, terminal_id: None, conversation_id: None,
            };
            save_session(connection, &session)?;
            let attempt = AttemptRecord { id, task_id: task.id, task_revision: task.revision, input_versions, session_id,
                target, actual_model: None, candidate_input: None, work_area_id: None, base_revision: None, state: AttemptState::Preparing };
            connection.execute("INSERT INTO attempts(id, task_id, session_id, active, data) VALUES (?1, ?2, ?3, 1, ?4)",
                params![id.to_string(), task.id.to_string(), session_id.to_string(), encode(&attempt)?])?;
            task.current_attempt = Some(id);
            save_task(connection, &task)?;
            event(connection, Some(task.run_id), "attempt_reserved", &attempt)?;
            Ok(attempt)
        })
    }

    pub fn attach_session(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        cwd: PathBuf,
        terminal_id: String,
        conversation_id: String,
    ) -> Result<Reply<ManagedSession>> {
        self.transact(request, json!({"op":"attach_session", "caller":identity, "cwd":cwd, "terminal_id":terminal_id, "conversation_id":conversation_id}), |connection| {
            let mut session = reference(connection, identity)?;
            if crate::sessions::id_of(&terminal_id).is_none() || (conversation_id.trim().is_empty() && session.agent != Agent::Codex) {
                return Err(invalid("a started session needs a terminal ID and conversation"));
            }
            let conversation_id = (!conversation_id.is_empty()).then_some(conversation_id);

            if session.state != ManagedSessionState::Preparing {
                if session.state == ManagedSessionState::Running && session.terminal_id.as_ref() == Some(&terminal_id)
                    && session.conversation_id == conversation_id && session.cwd.as_ref() == Some(&cwd) {
                    return Ok(session);
                }
                return Err(conflict("the session is already attached or cannot be started"));
            }
            if all::<ManagedSession>(connection, Table::Sessions)?.iter().any(|s| s.terminal_id.as_ref() == Some(&terminal_id) && s.binding.logical_id() != identity.id) {
                return Err(conflict("the terminal already belongs to another session"));
            }
            if let Some((task_id, attempt_id)) = session.binding.work() {
                let mut task: TaskRecord = require(connection, Table::Tasks, task_id)?;
                let mut attempt: AttemptRecord = require(connection, Table::Attempts, attempt_id)?;
                if task.current_attempt != Some(attempt_id) || task.revision != attempt.task_revision
                    || attempt.state != AttemptState::Starting {
                    return Err(conflict("the worker assignment changed before startup completed"));
                }
                let area_id = attempt.work_area_id.ok_or_else(|| conflict("the worker has no working area"))?;
                let area: WorkArea = require(connection, Table::WorkAreas, area_id)?;
                if area.state != WorkAreaState::Ready || !cwd.starts_with(&area.path)
                    || cwd.components().any(|component| matches!(component, std::path::Component::ParentDir)) {
                    return Err(conflict("the worker must start inside its prepared working area"));
                }
                attempt.state = AttemptState::Running;
                task.state = TaskState::Running;
                save_attempt(connection, &attempt)?;
                save_task(connection, &task)?;
                event(connection, session.binding.run_id(), "attempt_started", &attempt)?;
                event(connection, session.binding.run_id(), "task_running", &task)?;
            }
            session.cwd = Some(cwd);
            session.terminal_id = Some(terminal_id);
            session.conversation_id = conversation_id;
            session.state = ManagedSessionState::Running;
            save_session(connection, &session)?;
            event(connection, session.binding.run_id(), "session_attached", &session)?;
            Ok(session)
        })
    }

    pub fn start_uncertain(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        reason: String,
    ) -> Result<Reply<ManagedSession>> {
        self.transact(
            request,
            json!({"op":"start_uncertain", "caller":identity, "reason":reason}),
            |connection| {
                if reason.trim().is_empty() {
                    return Err(invalid("uncertain startup needs a reason"));
                }
                let mut session = reference(connection, identity)?;
                if matches!(
                    session.state,
                    ManagedSessionState::NeedsReconciliation
                        | ManagedSessionState::Exited
                        | ManagedSessionState::Failed
                ) {
                    return Ok(session);
                }
                let Some((task_id, attempt_id)) = session.binding.work() else {
                    session.state = ManagedSessionState::NeedsReconciliation;
                    save_session(connection, &session)?;
                    event(
                        connection,
                        session.binding.run_id(),
                        "session_start_uncertain",
                        &json!({"session":session,"reason":reason}),
                    )?;
                    return Ok(session);
                };
                let task: TaskRecord = require(connection, Table::Tasks, task_id)?;
                let mut attempt: AttemptRecord = require(connection, Table::Attempts, attempt_id)?;
                if !attempt.state.holds_slot() {
                    return Err(conflict("the attempt already finished"));
                }
                if attempt.state != AttemptState::CancelRequested {
                    attempt.state = AttemptState::Uncertain;
                }
                save_attempt(connection, &attempt)?;
                session.state = ManagedSessionState::NeedsReconciliation;
                save_session(connection, &session)?;
                store_message(
                    connection,
                    &AgentMessage {
                        id: MessageId::new(),
                        run_id: task.run_id,
                        from: identity.id,
                        to: session.binding.leader_id(),
                        task_id: Some(task_id),
                        attempt_id: Some(attempt_id),
                        revision: Some(attempt.task_revision),
                        kind: MessageKind::Question,
                        body: format!("Worker startup needs reconciliation: {reason}"),
                        reply_to: None,
                        blocks_work: true,
                    },
                )?;
                event(
                    connection,
                    Some(task.run_id),
                    "session_start_uncertain",
                    &json!({"session":session,"reason":reason}),
                )?;
                Ok(session)
            },
        )
    }

    pub fn fail_start(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        reason: String,
    ) -> Result<Reply<ManagedSession>> {
        self.transact(
            request,
            json!({"op":"fail_start", "caller":identity, "reason":reason}),
            |connection| lifecycle::fail_start_record(connection, identity, &reason, None),
        )
    }

    pub fn send_message(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        message: AgentMessage,
    ) -> Result<Reply<AgentMessage>> {
        self.transact(
            request,
            json!({"op":"send_message", "caller":identity, "message":message}),
            |connection| {
                let sender = reference(connection, identity)?;
                let recipient: ManagedSession = require(connection, Table::Sessions, message.to)?;
                if message.from != identity.id
                    || message.from == message.to
                    || sender.binding.run_id() != Some(message.run_id)
                    || recipient.binding.run_id() != Some(message.run_id)
                {
                    return Err(ControlError::new(
                        ErrorCode::OwnershipMismatch,
                        "messages must stay within their assigned run",
                    ));
                }
                if sender.binding.role() == SessionRole::Worker
                    && sender.binding.leader_id() != message.to
                {
                    return Err(ControlError::new(
                        ErrorCode::OwnershipMismatch,
                        "a worker reports to its own leader",
                    ));
                }
                for endpoint in [&sender, &recipient] {
                    if let Some((task, attempt)) = endpoint.binding.work()
                        && (message.task_id.is_some_and(|id| id != task)
                            || message.attempt_id.is_some_and(|id| id != attempt))
                    {
                        return Err(ControlError::new(
                            ErrorCode::OwnershipMismatch,
                            "the message does not match the worker assignment",
                        ));
                    }
                }
                if sender.binding.role() == SessionRole::Worker
                    && matches!(message.kind, MessageKind::Report | MessageKind::Question)
                    && (message.task_id.is_none()
                        || message.attempt_id.is_none()
                        || message.revision.is_none())
                {
                    return Err(invalid(
                        "a worker report or question needs its task, attempt and revision",
                    ));
                }
                if message.body.trim().is_empty() {
                    return Err(invalid("the message is empty"));
                }
                if message.body.len() > 64 * 1024 {
                    return Err(invalid(
                        "a message body exceeds 64 KiB; reference a file for large artifacts",
                    ));
                }
                if let Some(task_id) = message.task_id {
                    let task: TaskRecord = require(connection, Table::Tasks, task_id)?;
                    if task.run_id != message.run_id {
                        return Err(invalid("the message task belongs to another run"));
                    }
                    if let Some(attempt_id) = message.attempt_id {
                        let attempt: AttemptRecord =
                            require(connection, Table::Attempts, attempt_id)?;
                        if attempt.task_id != task_id
                            || message.revision.is_some_and(|r| r != attempt.task_revision)
                        {
                            return Err(invalid(
                                "the message attempt and revision do not match its task",
                            ));
                        }
                    }
                } else if message.attempt_id.is_some() {
                    return Err(invalid("an attempt message needs its task"));
                }
                if let Some(reply_to) = message.reply_to {
                    let parent: AgentMessage = require(connection, Table::Messages, reply_to)?;
                    if parent.run_id != message.run_id
                        || parent.from != message.to
                        || parent.to != message.from
                    {
                        return Err(invalid(
                            "a reply must address the original sender and recipient",
                        ));
                    }
                    if message.kind == MessageKind::Answer && parent.kind != MessageKind::Question {
                        return Err(invalid("an answer must reply to a question"));
                    }
                } else if message.kind == MessageKind::Answer {
                    return Err(invalid("an answer needs the question message ID"));
                }
                if let Some(existing) =
                    load::<AgentMessage>(connection, Table::Messages, message.id)?
                {
                    if existing != message {
                        return Err(conflict("the message ID was reused with different content"));
                    }
                    return Ok(existing);
                }
                store_message(connection, &message)?;
                Ok(message)
            },
        )
    }

    pub fn claim_delivery(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        message_id: MessageId,
    ) -> Result<Reply<Option<DeliveryRecord>>> {
        self.transact(
            request,
            json!({"op":"claim_delivery", "caller":identity, "message_id":message_id}),
            |connection| {
                reference(connection, identity)?;
                let message: AgentMessage = require(connection, Table::Messages, message_id)?;
                let mut delivery = delivery(connection, &message, identity.id)?;
                if delivery.state != DeliveryState::Stored {
                    return Ok(None);
                }
                delivery.state = DeliveryState::Dispatching;
                delivery.attempts = delivery
                    .attempts
                    .checked_add(1)
                    .ok_or_else(|| conflict("the delivery attempt count is exhausted"))?;
                save_delivery(connection, &delivery)?;
                event(
                    connection,
                    Some(message.run_id),
                    "delivery_dispatching",
                    &delivery,
                )?;
                Ok(Some(delivery))
            },
        )
    }

    pub fn retry_delivery(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        message_id: MessageId,
    ) -> Result<Reply<DeliveryRecord>> {
        self.transact(
            request,
            json!({"op":"retry_delivery", "caller":identity, "message_id":message_id}),
            |connection| {
                let message: AgentMessage = require(connection, Table::Messages, message_id)?;
                run_owner(connection, identity, message.run_id)?;
                let mut delivery = delivery(connection, &message, message.to)?;
                match delivery.state {
                    DeliveryState::Accepted | DeliveryState::Uncertain => {
                        delivery.state = DeliveryState::Stored;
                        delivery.last_error = None;
                        save_delivery(connection, &delivery)?;
                        event(
                            connection,
                            Some(message.run_id),
                            "delivery_retried",
                            &delivery,
                        )?;
                    }
                    DeliveryState::Stored => {}
                    _ => {
                        return Err(conflict(
                            "an active or acknowledged delivery cannot be retried",
                        ));
                    }
                }
                Ok(delivery)
            },
        )
    }

    pub fn recover_deliveries(&mut self) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut statement = transaction.prepare("SELECT data FROM deliveries")?;
        let deliveries = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|row| decode::<DeliveryRecord>(&row?))
            .collect::<Result<Vec<_>>>()?;
        drop(statement);
        for mut delivery in deliveries {
            if delivery.state == DeliveryState::Dispatching {
                delivery.state = DeliveryState::Uncertain;
                delivery.last_error =
                    Some("the daemon restarted before transport acceptance was recorded".into());
                let message: AgentMessage =
                    require(&transaction, Table::Messages, delivery.message_id)?;
                save_delivery(&transaction, &delivery)?;
                event(
                    &transaction,
                    Some(message.run_id),
                    "delivery_uncertain",
                    &delivery,
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn accept_delivery(
        &mut self,
        request: RequestId,
        message_id: MessageId,
        identity: SessionRef,
    ) -> Result<Reply<DeliveryRecord>> {
        self.transact(
            request,
            json!({"op":"accept_delivery", "message_id":message_id, "caller":identity}),
            |connection| {
                reference(connection, identity)?;
                let message: AgentMessage = require(connection, Table::Messages, message_id)?;
                let mut delivery = delivery(connection, &message, identity.id)?;
                if matches!(
                    delivery.state,
                    DeliveryState::Acknowledged | DeliveryState::Accepted
                ) {
                    return Ok(delivery);
                }
                if delivery.state == DeliveryState::Stored {
                    delivery.attempts = delivery
                        .attempts
                        .checked_add(1)
                        .ok_or_else(|| conflict("the delivery attempt count is exhausted"))?;
                }
                delivery.state = DeliveryState::Accepted;
                delivery.last_error = None;
                save_delivery(connection, &delivery)?;
                event(
                    connection,
                    Some(message.run_id),
                    "delivery_accepted",
                    &delivery,
                )?;
                Ok(delivery)
            },
        )
    }

    pub fn acknowledge_message(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        message_id: MessageId,
    ) -> Result<Reply<DeliveryRecord>> {
        self.transact(
            request,
            json!({"op":"acknowledge_message", "caller":identity, "message_id":message_id}),
            |connection| {
                reference(connection, identity)?;
                let message: AgentMessage = require(connection, Table::Messages, message_id)?;
                let mut delivery = delivery(connection, &message, identity.id)?;
                if delivery.state == DeliveryState::Acknowledged {
                    return Ok(delivery);
                }
                delivery.state = DeliveryState::Acknowledged;
                delivery.last_error = None;
                save_delivery(connection, &delivery)?;
                event(
                    connection,
                    Some(message.run_id),
                    "message_acknowledged",
                    &delivery,
                )?;
                Ok(delivery)
            },
        )
    }

    pub fn mailbox(&mut self, identity: SessionRef, after: u64, limit: u32) -> Result<MailboxPage> {
        let transaction = self.connection.transaction()?;
        reference(&transaction, identity)?;
        let after_sql: i64 = after
            .try_into()
            .map_err(|_| invalid("the mailbox cursor is too large"))?;
        let mut statement = transaction.prepare("SELECT rowid, data FROM messages WHERE recipient = ?1 AND rowid > ?2 ORDER BY rowid LIMIT ?3")?;
        let rows = statement.query_map(
            params![identity.id.to_string(), after_sql, limit.min(100)],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )?;
        let mut messages = Vec::new();
        let mut bytes = 0;
        for row in rows {
            let (sequence, text) = row?;
            let sequence = sequence.try_into().map_err(|_| {
                ControlError::new(ErrorCode::CorruptStore, "negative mailbox sequence")
            })?;
            let message: AgentMessage = decode(&text)?;
            let delivery = delivery(&transaction, &message, identity.id)?;
            let entry = MailboxEntry {
                sequence,
                message,
                delivery,
            };
            let size = encode(&entry)?.len();
            if bytes + size > 512 * 1024 {
                if messages.is_empty() {
                    return Err(invalid("a mailbox message exceeds the page limit"));
                }
                break;
            }
            bytes += size;
            messages.push(entry);
        }
        drop(statement);
        transaction.commit()?;
        Ok(MailboxPage {
            next_cursor: messages.last().map_or(after, |entry| entry.sequence),
            messages,
        })
    }

    pub fn snapshot(&mut self) -> Result<ControlSnapshot> {
        let transaction = self.connection.transaction()?;
        let event_cursor = cursor(&transaction)?;
        let mut statement = transaction.prepare("SELECT data FROM deliveries ORDER BY rowid")?;
        let deliveries = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|row| decode(&row?))
            .collect::<Result<Vec<_>>>()?;
        drop(statement);
        let snapshot = ControlSnapshot {
            event_cursor,
            sessions: all(&transaction, Table::Sessions)?,
            runs: all(&transaction, Table::Runs)?,
            tasks: all(&transaction, Table::Tasks)?,
            attempts: all(&transaction, Table::Attempts)?,
            messages: all(&transaction, Table::Messages)?,
            deliveries,
            reports: all(&transaction, Table::Reports)?,
            completions: all(&transaction, Table::Completions)?,
            candidates: all::<CandidateRecord>(&transaction, Table::Candidates)?
                .iter()
                .map(CandidateRecord::summary)
                .collect(),
            candidate_checks: all(&transaction, Table::CandidateChecks)?,
            completion_requests: all(&transaction, Table::CompletionRequests)?,
            coordination: all(&transaction, Table::Coordination)?,
            coordination_replies: all(&transaction, Table::CoordinationReplies)?,
            work_areas: all(&transaction, Table::WorkAreas)?,
            run_work_areas: all(&transaction, Table::RunWorkAreas)?,
        };
        transaction.commit()?;
        Ok(snapshot)
    }

    pub fn events_after(&self, after: u64, limit: u32) -> Result<Vec<ControlEvent>> {
        let after: i64 = after
            .try_into()
            .map_err(|_| invalid("the event cursor is too large"))?;
        let mut statement = self.connection.prepare("SELECT cursor, run_id, kind, data FROM events WHERE cursor > ?1 ORDER BY cursor LIMIT ?2")?;
        let rows = statement.query_map(params![after, limit.min(1000)], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (cursor, run, kind, data) = row?;
            let cursor = cursor.try_into().map_err(|_| {
                ControlError::new(ErrorCode::CorruptStore, "the event cursor is negative")
            })?;
            let run_id = run
                .map(|id| {
                    id.parse().map_err(|_| {
                        ControlError::new(ErrorCode::CorruptStore, "an event has an invalid run ID")
                    })
                })
                .transpose()?;
            Ok(ControlEvent {
                cursor,
                run_id,
                kind,
                data: decode(&data)?,
            })
        })
        .collect()
    }
}
