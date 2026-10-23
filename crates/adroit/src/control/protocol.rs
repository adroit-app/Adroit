//! Versioned, instance-local requests. Credentials never appear in events.
use std::path::PathBuf;

use adroit_model::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::store::ControlError;

pub const VERSION: u32 = 1;
pub const MAX_FRAME: usize = 1_048_576;
pub const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub id: RequestId,
    pub credential: String,
    /// Human clients may act as a selected session. Agent credentials cannot.
    pub actor: Option<SessionRef>,
    pub command: Command,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    #[schemars(skip)]
    RegisterLeader {
        id: LogicalSessionId,
        agent: Agent,
        cwd: PathBuf,
        #[serde(default)]
        permissions: AgentPermissions,
    },
    #[schemars(skip)]
    RememberLaunch {
        identity: SessionRef,
        runtime: Box<super::runtime::RuntimeLaunch>,
    },
    #[schemars(skip)]
    RegisterDefaultArea {
        identity: SessionRef,
        run_id: RunId,
        area: WorkArea,
    },
    #[schemars(skip)]
    AttachSession {
        identity: SessionRef,
        cwd: PathBuf,
        terminal_id: String,
        conversation_id: String,
    },
    #[schemars(skip)]
    CloseSession {
        identity: SessionRef,
        terminal_id: String,
    },
    #[schemars(skip)]
    ConfirmTerminalExit {
        identity: SessionRef,
        terminal_id: String,
    },
    #[schemars(skip)]
    RestartLeader {
        identity: SessionRef,
    },
    #[schemars(skip)]
    AbortPreparedStart {
        identity: SessionRef,
        terminal_id: String,
        reason: String,
    },
    #[schemars(skip)]
    StartUncertain {
        identity: SessionRef,
        reason: String,
    },
    #[schemars(skip)]
    FailStart {
        identity: SessionRef,
        reason: String,
    },
    #[schemars(skip)]
    Grant {
        identity: SessionRef,
    },
    Context,
    Models,
    ResolveIds {
        ids: Vec<String>,
    },
    #[schemars(skip)]
    NativeArguments {
        name: String,
        arguments: Value,
    },
    #[schemars(skip)]
    NativeCall {
        name: String,
        arguments: Value,
    },
    #[schemars(skip)]
    NativeEnvelope {
        message_id: MessageId,
    },
    Snapshot,
    #[schemars(skip)]
    UiPage {
        table: UiTable,
        after: u64,
        epoch: Option<u64>,
    },
    #[schemars(skip)]
    UiTaskHistory {
        task_id: TaskId,
        after: u64,
        epoch: Option<u64>,
    },
    #[schemars(skip)]
    UiRecord {
        table: UiTable,
        rowid: u64,
        offset: u64,
        epoch: u64,
    },
    #[schemars(skip)]
    UiDetail {
        table: UiTable,
        id: String,
        offset: u32,
        epoch: Option<u64>,
    },
    ReadMessage {
        message_id: MessageId,
    },
    Mailbox {
        after: u64,
        limit: u32,
    },
    #[schemars(skip)]
    ClaimDelivery {
        message_id: MessageId,
    },
    RetryDelivery {
        message_id: MessageId,
    },
    #[schemars(skip)]
    AcceptDelivery {
        message_id: MessageId,
    },
    Events {
        after: u64,
        limit: u32,
    },
    OpenRun {
        id: RunId,
        brief: RunBrief,
    },
    #[schemars(skip)]
    AuthorizeIndependentRun {
        leader: SessionRef,
        workspace: PathBuf,
        brief: RunBrief,
        agent: Agent,
        model: Option<String>,
        #[serde(default)]
        permissions: AgentPermissions,
    },
    CreateIndependentRun {
        authorization_id: RequestId,
    },
    UpdateRun {
        run_id: RunId,
        expected_revision: u64,
        brief: RunBrief,
        #[serde(default)]
        affected: Vec<TaskVersion>,
    },
    CancelRun {
        run_id: RunId,
    },
    RequestChangeCompletion {
        run_id: RunId,
        expected_revision: u64,
        candidate_id: CandidateId,
        summary: String,
    },
    CompleteRun {
        run_id: RunId,
        expected_revision: u64,
        summary: String,
    },
    Candidate {
        id: CandidateId,
    },
    ProposeCandidate {
        expected_revision: u64,
        plan: CandidatePlan,
    },
    AnnounceChange {
        intent: CoordinationIntent,
    },
    DiscussChange {
        reply: CoordinationReply,
    },
    FinishChange {
        notice_id: CoordinationId,
        outcome: CoordinationOutcome,
    },
    PutTask {
        plan: TaskPlan,
    },
    QueueTask {
        version: TaskVersion,
    },
    ReportTask {
        report: TaskReport,
    },
    CancelTask {
        version: TaskVersion,
    },
    SendMessage {
        message: AgentMessage,
    },
    Acknowledge {
        message_id: MessageId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub version: u32,
    pub id: Option<RequestId>,
    pub result: Result<Value, ControlError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPage {
    /// Advances over filtered events as well as visible ones.
    pub next_cursor: u64,
    pub events: Vec<ControlEvent>,
}

/// UI previews are read separately from durable evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UiTable {
    Sessions,
    Runs,
    Tasks,
    Attempts,
    Messages,
    Deliveries,
    Reports,
    Completions,
    CompletionRequests,
    Candidates,
    CandidateChecks,
    Coordination,
    CoordinationReplies,
    WorkAreas,
    RunWorkAreas,
}
impl UiTable {
    pub const ALL: [Self; 15] = [
        Self::Sessions,
        Self::Runs,
        Self::Tasks,
        Self::Attempts,
        Self::Messages,
        Self::Deliveries,
        Self::Reports,
        Self::Completions,
        Self::CompletionRequests,
        Self::Candidates,
        Self::CandidateChecks,
        Self::Coordination,
        Self::CoordinationReplies,
        Self::WorkAreas,
        Self::RunWorkAreas,
    ];
    pub fn field(self) -> &'static str {
        match self {
            Self::Sessions => "sessions",
            Self::Runs => "runs",
            Self::Tasks => "tasks",
            Self::Attempts => "attempts",
            Self::Messages => "messages",
            Self::Deliveries => "deliveries",
            Self::Reports => "reports",
            Self::Completions => "completions",
            Self::CompletionRequests => "completion_requests",
            Self::Candidates => "candidates",
            Self::CandidateChecks => "candidate_checks",
            Self::Coordination => "coordination",
            Self::CoordinationReplies => "coordination_replies",
            Self::WorkAreas => "work_areas",
            Self::RunWorkAreas => "run_work_areas",
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct UiPage {
    pub epoch: u64,
    pub next_cursor: Option<u64>,
    pub rows: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<UiRecordRef>,
}

/// A large projected row is read separately at the page's epoch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiRecordRef {
    pub rowid: u64,
    pub bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UiRecordChunk {
    pub epoch: u64,
    pub rowid: u64,
    pub offset: u64,
    pub text: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct UiDetailPage {
    pub epoch: u64,
    pub next_offset: Option<u32>,
    pub text: String,
}

impl UiRecordRef {
    pub(crate) fn validate_page(
        &self,
        after: u64,
        page: &UiPage,
    ) -> Result<(), super::store::ControlError> {
        if self.rowid <= after
            || self.bytes == 0
            || !page.rows.is_empty()
            || page.next_cursor != Some(self.rowid)
        {
            return Err(ui_record_error("invalid UI record reference"));
        }
        Ok(())
    }
}

fn ui_record_error(message: &str) -> super::store::ControlError {
    super::store::ControlError::new(super::store::ErrorCode::StorageFailure, message)
}

/// Owns an incomplete projected row until every declared byte has arrived.
/// Both socket and compatibility readers use the same reconstruction rules.
pub(crate) struct UiRecordReader {
    pub reference: UiRecordRef,
    epoch: u64,
    text: String,
}
impl UiRecordReader {
    pub fn new(reference: UiRecordRef, epoch: u64) -> Self {
        Self {
            reference,
            epoch,
            text: String::new(),
        }
    }
    pub fn offset(&self) -> u64 {
        self.text.len() as u64
    }
    pub fn complete(&self) -> bool {
        self.offset() == self.reference.bytes
    }
    pub fn push(&mut self, chunk: UiRecordChunk) -> Result<(), super::store::ControlError> {
        if chunk.epoch != self.epoch
            || chunk.rowid != self.reference.rowid
            || chunk.offset != self.offset()
            || chunk.text.is_empty()
            || chunk.text.len() > 32 * 1024
            || chunk.text.len() as u64 > self.reference.bytes - self.offset()
        {
            return Err(ui_record_error("inconsistent UI record fragment"));
        }
        self.text.push_str(&chunk.text);
        Ok(())
    }
    pub fn finish(self) -> Result<Value, super::store::ControlError> {
        if !self.complete() {
            return Err(ui_record_error("incomplete UI record"));
        }
        serde_json::from_str(&self.text).map_err(|_| ui_record_error("invalid UI record JSON"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiHistoricalReport {
    pub id: MessageId,
    pub task_revision: u64,
    pub applied: bool,
    pub status: ReportStatus,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiAttemptHistory {
    pub attempt_id: AttemptId,
    pub task_revision: u64,
    pub session_id: LogicalSessionId,
    pub state: AttemptState,
    pub report: Option<UiHistoricalReport>,
}
impl UiAttemptHistory {
    pub fn is_historical(&self, task: &TaskRecord) -> bool {
        task.current_attempt != Some(self.attempt_id)
            || task.revision != self.task_revision
            || self
                .report
                .as_ref()
                .is_some_and(|r| r.task_revision != task.revision || !r.applied)
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct UiTaskHistoryPage {
    pub epoch: u64,
    pub task_id: TaskId,
    pub next_cursor: Option<u64>,
    pub entries: Vec<UiAttemptHistory>,
}

#[cfg(test)]
mod ui_record_tests {
    use super::*;

    #[test]
    fn history_marks_cleared_replaced_old_revision_and_unapplied_results() {
        let entry = UiAttemptHistory {
            attempt_id: AttemptId::new(),
            task_revision: 7,
            session_id: LogicalSessionId::new(),
            state: AttemptState::Succeeded,
            report: Some(UiHistoricalReport {
                id: MessageId::new(),
                task_revision: 7,
                applied: true,
                status: ReportStatus::Succeeded,
            }),
        };
        let mut task = TaskRecord {
            id: TaskId::new(),
            run_id: RunId::new(),
            revision: 7,
            brief: TaskBrief::default(),
            dependencies: vec![],
            state: TaskState::Succeeded,
            current_attempt: Some(entry.attempt_id),
        };
        assert!(!entry.is_historical(&task));
        task.revision = 8;
        assert!(entry.is_historical(&task));
        task.revision = 7;
        task.current_attempt = None;
        assert!(entry.is_historical(&task));
        task.current_attempt = Some(AttemptId::new());
        assert!(entry.is_historical(&task));
        task.current_attempt = Some(entry.attempt_id);
        let mut unapplied = entry.clone();
        unapplied.report.as_mut().unwrap().applied = false;
        assert!(unapplied.is_historical(&task));
        let mut older_report = entry;
        older_report.report.as_mut().unwrap().task_revision = 6;
        assert!(older_report.is_historical(&task));
    }

    #[test]
    fn an_oversized_reference_has_a_unique_advancing_page_position() {
        let reference = UiRecordRef {
            rowid: 8,
            bytes: 300_000,
        };
        let mut page = UiPage {
            epoch: 42,
            next_cursor: Some(8),
            rows: vec![],
            record: None,
        };
        assert!(reference.validate_page(7, &page).is_ok());
        assert!(reference.validate_page(8, &page).is_err());
        page.next_cursor = None;
        assert!(reference.validate_page(7, &page).is_err());
        page.next_cursor = Some(8);
        page.rows.push(serde_json::json!({}));
        assert!(reference.validate_page(7, &page).is_err());
    }

    fn reader() -> UiRecordReader {
        UiRecordReader::new(
            UiRecordRef {
                rowid: 8,
                bytes: 10,
            },
            42,
        )
    }
    fn chunk(offset: u64, text: &str) -> UiRecordChunk {
        UiRecordChunk {
            epoch: 42,
            rowid: 8,
            offset,
            text: text.into(),
        }
    }

    #[test]
    fn record_fragments_require_exact_identity_epoch_byte_order_and_completion() {
        let mut r = reader();
        assert!(r.push(chunk(1, "{}")).is_err());
        assert!(
            r.push(UiRecordChunk {
                rowid: 9,
                ..chunk(0, "{}")
            })
            .is_err()
        );
        assert!(
            r.push(UiRecordChunk {
                epoch: 43,
                ..chunk(0, "{}")
            })
            .is_err()
        );
        assert!(r.push(chunk(0, "")).is_err());
        assert!(r.push(chunk(0, "01234567890")).is_err());
        assert!(reader().finish().is_err());
        r.push(chunk(0, "{\"a\":")).unwrap();
        assert!(r.push(chunk(0, "{\"a\":")).is_err());
        assert!(r.push(chunk(6, "null}")).is_err());
        r.push(chunk(5, "null}")).unwrap();
        assert!(r.complete());
        assert!(r.push(chunk(10, " ")).is_err());
        assert_eq!(r.finish().unwrap(), serde_json::json!({"a": null}));
    }

    #[test]
    fn record_reconstruction_counts_utf8_bytes_and_rejects_missing_or_invalid_json() {
        let text = "{\"🦀\":42}";
        let mut r = UiRecordReader::new(
            UiRecordRef {
                rowid: 8,
                bytes: text.len() as u64,
            },
            42,
        );
        r.push(chunk(0, "{\"🦀\"")).unwrap();
        assert!(!r.complete());
        r.push(chunk(7, ":42}")).unwrap();
        assert_eq!(r.finish().unwrap(), serde_json::json!({"🦀":42}));
        let mut r = reader();
        r.push(chunk(0, "invalid!!!")).unwrap();
        assert!(r.finish().is_err());
    }
}
