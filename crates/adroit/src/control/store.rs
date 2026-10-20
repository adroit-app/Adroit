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

const SCHEMA_VERSION: i64 = 2;

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
}
impl Table {
    fn name(self) -> &'static str {
        match self {
            Self::Sessions => "logical_sessions",
            Self::Runs => "runs",
            Self::Tasks => "tasks",
            Self::Attempts => "attempts",
            Self::Messages => "messages",
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
fn owner(connection: &Connection, identity: SessionRef, run_id: RunId) -> Result<RunRecord> {
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
    if run.state != RunState::Active {
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
        self.transact(
            request,
            json!({"op":"register_leader", "id":id, "agent":agent, "cwd":cwd}),
            |connection| {
                if load::<ManagedSession>(connection, Table::Sessions, id)?.is_some() {
                    return Err(conflict("the logical session already exists"));
                }
                let session = ManagedSession {
                    binding: SessionBinding::leader(id, None),
                    agent,
                    generation: 1,
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
            if load::<RunRecord>(connection, Table::Runs, id)?.is_some() { return Err(conflict("the run already exists")); }
            let active: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM runs WHERE leader_id = ?1 AND state IN ('active', 'verifying'))", [identity.id.to_string()], |row| row.get(0))?;
            if active { return Err(conflict("the leader already has an active run")); }
            let run = RunRecord { id, leader_id: identity.id, brief, plan_revision: 1, state: RunState::Active };
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
                        (
                            task.revision
                                .checked_add(1)
                                .ok_or_else(|| conflict("the task revision is exhausted"))?,
                            task.current_attempt,
                            task.state,
                        )
                    }
                    None => {
                        if plan.expected_revision.is_some() {
                            return Err(ControlError::new(
                                ErrorCode::NotFound,
                                "the task does not exist",
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
            for dependency in &task.dependencies {
                let dependency: TaskRecord = require(connection, Table::Tasks, dependency)?;
                if dependency.state != TaskState::Succeeded { return Err(conflict("a task dependency has not succeeded")); }
            }
            let id = AttemptId::new();
            let session_id = LogicalSessionId::new();
            let session = ManagedSession {
                binding: SessionBinding::worker(session_id, identity.id, task.run_id, task.id, id)
                    .map_err(|error| invalid(error.to_string()))?,
                agent: target.agent, generation: 1, cwd: None, terminal_id: None, conversation_id: None,
            };
            save_session(connection, &session)?;
            let attempt = AttemptRecord { id, task_id: task.id, task_revision: task.revision, session_id,
                target, actual_model: None, work_area_id: None, base_revision: None, state: AttemptState::Preparing };
            connection.execute("INSERT INTO attempts(id, task_id, session_id, active, data) VALUES (?1, ?2, ?3, 1, ?4)",
                params![id.to_string(), task.id.to_string(), session_id.to_string(), encode(&attempt)?])?;
            task.current_attempt = Some(id);
            save_task(connection, &task)?;
            event(connection, Some(task.run_id), "attempt_reserved", &attempt)?;
            Ok(attempt)
        })
    }

    pub fn send_message(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        message: AgentMessage,
    ) -> Result<Reply<AgentMessage>> {
        self.transact(request, json!({"op":"send_message", "caller":identity, "message":message}), |connection| {
            let sender = reference(connection, identity)?;
            let recipient: ManagedSession = require(connection, Table::Sessions, message.to)?;
            if message.from != identity.id || message.from == message.to
                || sender.binding.run_id() != Some(message.run_id) || recipient.binding.run_id() != Some(message.run_id) {
                return Err(ControlError::new(ErrorCode::OwnershipMismatch, "messages must stay within their assigned run"));
            }
            if sender.binding.role() == SessionRole::Worker && sender.binding.leader_id() != message.to {
                return Err(ControlError::new(ErrorCode::OwnershipMismatch, "a worker reports to its own leader"));
            }
            for endpoint in [&sender, &recipient] {
                if let Some((task, attempt)) = endpoint.binding.work()
                    && (message.task_id.is_some_and(|id| id != task)
                        || message.attempt_id.is_some_and(|id| id != attempt))
                {
                    return Err(ControlError::new(ErrorCode::OwnershipMismatch, "the message does not match the worker assignment"));
                }
            }
            if sender.binding.role() == SessionRole::Worker && message.kind == MessageKind::Report
                && (message.task_id.is_none() || message.attempt_id.is_none() || message.revision.is_none())
            {
                return Err(invalid("a worker report needs its task, attempt and revision"));
            }
            if message.body.trim().is_empty() { return Err(invalid("the message is empty")); }
            if let Some(task_id) = message.task_id {
                let task: TaskRecord = require(connection, Table::Tasks, task_id)?;
                if task.run_id != message.run_id { return Err(invalid("the message task belongs to another run")); }
                if let Some(attempt_id) = message.attempt_id {
                    let attempt: AttemptRecord = require(connection, Table::Attempts, attempt_id)?;
                    if attempt.task_id != task_id || message.revision.is_some_and(|r| r != attempt.task_revision) {
                        return Err(invalid("the message attempt and revision do not match its task"));
                    }
                }
            } else if message.attempt_id.is_some() { return Err(invalid("an attempt message needs its task")); }
            if let Some(reply_to) = message.reply_to {
                let parent: AgentMessage = require(connection, Table::Messages, reply_to)?;
                if parent.run_id != message.run_id { return Err(invalid("the reply belongs to another run")); }
            }
            if let Some(existing) = load::<AgentMessage>(connection, Table::Messages, message.id)? {
                if existing != message { return Err(conflict("the message ID was reused with different content")); }
                return Ok(existing);
            }
            connection.execute("INSERT INTO messages(id, run_id, sender, recipient, data) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![message.id.to_string(), message.run_id.to_string(), message.from.to_string(), message.to.to_string(), encode(&message)?])?;
            let delivery = DeliveryRecord { message_id: message.id, recipient: message.to, state: DeliveryState::Stored, attempts: 0, last_error: None };
            connection.execute("INSERT INTO deliveries(message_id, recipient, data) VALUES (?1, ?2, ?3)",
                params![message.id.to_string(), message.to.to_string(), encode(&delivery)?])?;
            event(connection, Some(message.run_id), "message_stored", &message)?;
            Ok(message)
        })
    }

    pub fn accept_delivery(
        &mut self,
        request: RequestId,
        message_id: MessageId,
        recipient: LogicalSessionId,
    ) -> Result<Reply<DeliveryRecord>> {
        self.transact(
            request,
            json!({"op":"accept_delivery", "message_id":message_id, "recipient":recipient}),
            |connection| {
                let message: AgentMessage = require(connection, Table::Messages, message_id)?;
                let mut delivery = delivery(connection, &message, recipient)?;
                if matches!(
                    delivery.state,
                    DeliveryState::Acknowledged | DeliveryState::Accepted
                ) {
                    return Ok(delivery);
                }
                delivery.state = DeliveryState::Accepted;
                delivery.attempts = delivery
                    .attempts
                    .checked_add(1)
                    .ok_or_else(|| conflict("the delivery attempt count is exhausted"))?;
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
