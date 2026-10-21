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
    Snapshot,
    #[schemars(skip)]
    UiPage {
        table: UiTable,
        after: u64,
        epoch: Option<u64>,
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
}
#[derive(Debug, Serialize, Deserialize)]
pub struct UiDetailPage {
    pub epoch: u64,
    pub next_offset: Option<u32>,
    pub text: String,
}
