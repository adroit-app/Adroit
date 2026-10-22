use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    Agent, AgentPermissions, AttemptId, CandidateId, CoordinationId, LogicalSessionId, MessageId,
    RunId, SessionBinding, TaskId, WorkAreaId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionRef {
    pub id: LogicalSessionId,
    pub generation: u64,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ManagedSessionState {
    #[default]
    Preparing,
    Running,
    NeedsReconciliation,
    Exited,
    Closed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManagedSession {
    pub binding: SessionBinding,
    pub agent: Agent,
    pub generation: u64,
    #[serde(default)]
    pub state: ManagedSessionState,
    #[serde(default)]
    pub permissions: AgentPermissions,
    pub cwd: Option<PathBuf>,
    pub terminal_id: Option<String>,
    pub conversation_id: Option<String>,
}

impl ManagedSession {
    pub fn reference(&self) -> SessionRef {
        SessionRef {
            id: self.binding.logical_id(),
            generation: self.generation,
        }
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CompletionKind {
    #[default]
    Report,
    Change,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RunBrief {
    pub title: String,
    pub goal: String,
    pub constraints: Vec<String>,
    pub completion_kind: CompletionKind,
    pub quality_gate: QualityGate,
    pub decisions: Vec<String>,
    pub unresolved: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Active,
    Verifying,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
}

impl RunState {
    pub fn word(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Verifying => "verifying",
            Self::Cancelling => "cancelling",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    pub id: RunId,
    pub leader_id: LogicalSessionId,
    pub brief: RunBrief,
    pub plan_revision: u64,
    #[serde(default)]
    pub candidate_id: Option<CandidateId>,
    pub state: RunState,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TaskPurpose {
    Investigate,
    Implement,
    Review,
    Classify,
    Integrate,
    Verify,
    #[default]
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct TargetPreference {
    pub agent: Option<Agent>,
    pub model: Option<String>,
    pub permissions: AgentPermissions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct TaskBrief {
    pub instructions: String,
    pub required: bool,
    pub candidate_id: Option<CandidateId>,
    pub scope: Vec<String>,
    pub inputs: Vec<String>,
    pub purpose: TaskPurpose,
    pub source_dir: Option<PathBuf>,
    pub base_revision: Option<String>,
    pub workdir: WorkdirPolicy,
    pub target: TargetPreference,
}

impl Default for TaskBrief {
    fn default() -> Self {
        Self {
            instructions: String::new(),
            required: true,
            candidate_id: None,
            scope: Vec::new(),
            inputs: Vec::new(),
            purpose: TaskPurpose::default(),
            source_dir: None,
            base_revision: None,
            workdir: WorkdirPolicy::default(),
            target: TargetPreference::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskPlan {
    pub id: TaskId,
    pub run_id: RunId,
    #[schemars(
        description = "Use null for a new task. To update an existing task, supply its current revision from context or snapshot; this is a concurrency check, not the revision to assign."
    )]
    pub expected_revision: Option<u64>,
    pub brief: TaskBrief,
    pub dependencies: Vec<TaskId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskVersion {
    pub task_id: TaskId,
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Planned,
    Queued,
    Running,
    Verifying,
    Blocked,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskRecord {
    pub id: TaskId,
    pub run_id: RunId,
    pub revision: u64,
    pub brief: TaskBrief,
    pub dependencies: Vec<TaskId>,
    pub state: TaskState,
    pub current_attempt: Option<AttemptId>,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ModelSource {
    Explicit,
    Settings,
    #[default]
    AgentDefault,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetSelection {
    pub agent: Agent,
    pub requested_model: Option<String>,
    pub model: Option<String>,
    pub model_source: ModelSource,
    pub permissions: AgentPermissions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AttemptState {
    Preparing,
    Starting,
    Running,
    CancelRequested,
    Succeeded,
    Failed,
    Cancelled,
    Uncertain,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WorkerReadiness {
    #[default]
    Spawned,
    Ready,
    AwaitingTrust,
    AwaitingPermission,
    AwaitingInput,
    Stalled,
}

impl AttemptState {
    pub fn holds_slot(self) -> bool {
        matches!(
            self,
            Self::Preparing
                | Self::Starting
                | Self::Running
                | Self::CancelRequested
                | Self::Uncertain
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttemptRecord {
    pub id: AttemptId,
    pub task_id: TaskId,
    pub task_revision: u64,
    #[serde(default)]
    pub input_versions: Vec<TaskVersion>,
    pub session_id: LogicalSessionId,
    pub target: TargetSelection,
    pub actual_model: Option<String>,
    #[serde(default)]
    pub candidate_input: Option<CandidateProofSummary>,
    pub work_area_id: Option<WorkAreaId>,
    pub base_revision: Option<String>,
    pub state: AttemptState,
    #[serde(default)]
    pub readiness: WorkerReadiness,
    #[serde(default)]
    pub spawned_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Note,
    Question,
    Answer,
    Report,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentMessage {
    pub id: MessageId,
    pub run_id: RunId,
    pub from: LogicalSessionId,
    pub to: LogicalSessionId,
    pub task_id: Option<TaskId>,
    pub attempt_id: Option<AttemptId>,
    pub revision: Option<u64>,
    pub kind: MessageKind,
    pub body: String,
    pub reply_to: Option<MessageId>,
    pub blocks_work: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Stored,
    Dispatching,
    Accepted,
    Acknowledged,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeliveryRecord {
    pub message_id: MessageId,
    pub recipient: LogicalSessionId,
    pub state: DeliveryState,
    pub attempts: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailboxEntry {
    pub sequence: u64,
    pub message: AgentMessage,
    pub delivery: DeliveryRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailboxPage {
    pub next_cursor: u64,
    pub messages: Vec<MailboxEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlSnapshot {
    pub event_cursor: u64,
    pub sessions: Vec<ManagedSession>,
    pub runs: Vec<RunRecord>,
    pub tasks: Vec<TaskRecord>,
    pub attempts: Vec<AttemptRecord>,
    pub messages: Vec<AgentMessage>,
    pub deliveries: Vec<DeliveryRecord>,
    pub reports: Vec<TaskReportRecord>,
    pub completions: Vec<RunCompletion>,
    pub completion_requests: Vec<ChangeCompletionRequest>,
    pub candidates: Vec<CandidateSummary>,
    pub candidate_checks: Vec<CandidateCheckRecord>,
    pub coordination: Vec<CoordinationNotice>,
    pub coordination_replies: Vec<CoordinationReply>,
    pub work_areas: Vec<WorkArea>,
    pub run_work_areas: Vec<RunWorkArea>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlEvent {
    pub cursor: u64,
    pub run_id: Option<RunId>,
    pub kind: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DependencyError {
    #[error("task {0} cannot depend on itself")]
    SelfDependency(TaskId),
    #[error("dependency {0} is listed more than once")]
    Duplicate(TaskId),
    #[error("dependency {0} does not exist")]
    Missing(TaskId),
    #[error("dependency {0} belongs to another run")]
    ForeignRun(TaskId),
    #[error("task {0} would form a dependency cycle")]
    Cycle(TaskId),
}

pub fn validate_dependencies(tasks: &[TaskRecord], plan: &TaskPlan) -> Result<(), DependencyError> {
    use std::collections::{HashMap, HashSet};
    let by_id: HashMap<_, _> = tasks.iter().map(|task| (task.id, task)).collect();
    let mut seen = HashSet::new();
    for id in &plan.dependencies {
        if *id == plan.id {
            return Err(DependencyError::SelfDependency(*id));
        }
        if !seen.insert(*id) {
            return Err(DependencyError::Duplicate(*id));
        }
        let task = by_id.get(id).ok_or(DependencyError::Missing(*id))?;
        if task.run_id != plan.run_id {
            return Err(DependencyError::ForeignRun(*id));
        }
    }
    let mut pending = plan.dependencies.clone();
    seen.clear();
    while let Some(id) = pending.pop() {
        if id == plan.id {
            return Err(DependencyError::Cycle(id));
        }
        if seen.insert(id)
            && let Some(task) = by_id.get(&id)
        {
            pending.extend(task.dependencies.iter().copied());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkAreaBackend {
    Jj,
    Git,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkAreaState {
    Preparing,
    Ready,
    Failed,
    NeedsReconciliation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkArea {
    pub id: WorkAreaId,
    pub backend: WorkAreaBackend,
    pub path: PathBuf,
    pub source: PathBuf,
    pub source_root: PathBuf,
    pub repository: Option<PathBuf>,
    pub base_revision: Option<String>,
    pub native_name: Option<String>,
    pub description: String,
    pub state: WorkAreaState,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "policy", content = "work_area_id", rename_all = "snake_case")]
pub enum WorkdirPolicy {
    #[default]
    Auto,
    Existing(WorkAreaId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunWorkArea {
    pub run_id: RunId,
    pub work_area_id: WorkAreaId,
    pub is_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskArtifact {
    Review {
        decision: ReviewDecision,
        #[schemars(schema_with = "json_evidence_schema")]
        data: serde_json::Value,
        files: Vec<PathBuf>,
        reviewed_revision: Option<String>,
    },
    Report {
        #[schemars(schema_with = "json_evidence_schema")]
        data: serde_json::Value,
        files: Vec<PathBuf>,
        reviewed_revision: Option<String>,
    },
    Change {
        backend: WorkAreaBackend,
        work_area_id: WorkAreaId,
        revision: Option<String>,
        manifest: Vec<String>,
        candidate_fingerprint: Option<String>,
    },
}

fn json_evidence_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    // Native clients may lower a boolean accept-any schema to a string parameter.
    schemars::json_schema!({
        "description":"JSON evidence. Supply objects and arrays directly; do not JSON-encode structured evidence into a string.",
        "anyOf":[
            {"type":"object","additionalProperties":true},
            {"type":"array","items":{}},
            {"type":"string"},
            {"type":"number"},
            {"type":"boolean"},
            {"type":"null"}
        ]
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultCheck {
    pub command: Vec<String>,
    pub exit_code: i32,
    pub candidate: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskResult {
    pub status: ReportStatus,
    pub summary: String,
    pub artifact: Option<TaskArtifact>,
    pub checks: Vec<ResultCheck>,
    pub unresolved: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskReport {
    pub message_id: MessageId,
    pub task: TaskVersion,
    pub attempt_id: AttemptId,
    pub result: TaskResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskReportRecord {
    pub report: TaskReport,
    pub run_id: RunId,
    /// False when the assignment was revised or cancellation won the race.
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunCompletion {
    pub run_id: RunId,
    pub plan_revision: u64,
    pub summary: String,
    pub reports: Vec<MessageId>,
    #[serde(default)]
    pub candidate_id: Option<CandidateId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompletionRequestState {
    Pending,
    Succeeded,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangeCompletionRequest {
    pub id: crate::RequestId,
    pub leader: SessionRef,
    pub run_id: RunId,
    pub plan_revision: u64,
    pub candidate_id: CandidateId,
    pub summary: String,
    pub state: CompletionRequestState,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoordinationOperation {
    Merge,
    Apply,
    Edit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoordinationIntent {
    pub id: CoordinationId,
    pub run_id: RunId,
    pub work_area_id: WorkAreaId,
    pub operation: CoordinationOperation,
    pub purpose: String,
    pub scope: Vec<String>,
    pub base: Option<String>,
    pub candidate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoordinationOutcome {
    pub summary: String,
    pub scope: Vec<String>,
    pub revision: Option<String>,
    pub fingerprint: Option<String>,
    pub unresolved: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoordinationParticipant {
    pub leader: SessionRef,
    pub run_id: RunId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoordinationNotice {
    pub intent: CoordinationIntent,
    pub leader: SessionRef,
    pub participants: Vec<CoordinationParticipant>,
    pub outcome: Option<CoordinationOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoordinationReply {
    pub id: MessageId,
    pub notice_id: CoordinationId,
    pub from: SessionRef,
    pub run_id: RunId,
    pub task_id: Option<TaskId>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CandidateFileState {
    File { sha256: String, executable: bool },
    Symlink { target: String },
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateFile {
    pub path: String,
    pub state: CandidateFileState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateFiles {
    pub version: u32,
    pub fingerprint: String,
    pub files: Vec<CandidateFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateProof {
    pub work_area_id: WorkAreaId,
    pub backend: WorkAreaBackend,
    pub revision: Option<String>,
    pub files: CandidateFiles,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentContext {
    pub session: ManagedSession,
    pub run: Option<RunRecord>,
    pub task: Option<TaskRecord>,
    pub attempt: Option<AttemptRecord>,
    pub work_area: Option<WorkArea>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct QualityGate {
    pub commands: Vec<Vec<String>>,
    pub review_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidatePlan {
    pub id: CandidateId,
    pub run_id: RunId,
    pub source: TaskVersion,
    pub attempt_id: AttemptId,
    #[serde(default)]
    pub inputs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CandidateState {
    Requested,
    Prepared,
    Verified,
    Superseded,
    Stale,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateRecord {
    pub plan: CandidatePlan,
    pub source_report: MessageId,
    pub artifact: TaskArtifact,
    pub state: CandidateState,
    pub proof: Option<CandidateProof>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateProofSummary {
    pub work_area_id: WorkAreaId,
    pub backend: WorkAreaBackend,
    pub revision: Option<String>,
    pub fingerprint: String,
    pub file_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateSummary {
    pub plan: CandidatePlan,
    pub source_report: MessageId,
    pub artifact: TaskArtifact,
    pub state: CandidateState,
    pub proof: Option<CandidateProofSummary>,
    pub error: Option<String>,
}

impl CandidateRecord {
    pub fn summary(&self) -> CandidateSummary {
        CandidateSummary {
            plan: self.plan.clone(),
            source_report: self.source_report,
            artifact: self.artifact.clone(),
            state: self.state,
            error: self.error.clone(),
            proof: self.proof.as_ref().map(CandidateProof::summary),
        }
    }
}

impl CandidateProof {
    pub fn summary(&self) -> CandidateProofSummary {
        CandidateProofSummary {
            work_area_id: self.work_area_id,
            backend: self.backend,
            revision: self.revision.clone(),
            fingerprint: self.files.fingerprint.clone(),
            file_count: self.files.files.len(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CandidateCheckState {
    Pending,
    Validated,
    Rejected,
    Obsolete,
}

impl CandidateCheckState {
    pub fn word(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Validated => "validated",
            Self::Rejected => "rejected",
            Self::Obsolete => "obsolete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateCheckRecord {
    pub attempt_id: AttemptId,
    pub candidate_id: CandidateId,
    pub task: TaskVersion,
    pub report_id: MessageId,
    pub state: CandidateCheckState,
    pub observed: Option<CandidateProofSummary>,
    pub error: Option<String>,
}
