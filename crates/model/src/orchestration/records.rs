use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    Agent, AgentPermissions, AttemptId, LogicalSessionId, MessageId, RunId, SessionBinding, TaskId,
    WorkAreaId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRef {
    pub id: LogicalSessionId,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedSessionState {
    #[default]
    Preparing,
    Running,
    NeedsReconciliation,
    Exited,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionKind {
    #[default]
    Report,
    Change,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RunBrief {
    pub title: String,
    pub goal: String,
    pub constraints: Vec<String>,
    pub completion_kind: CompletionKind,
    pub decisions: Vec<String>,
    pub unresolved: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Active,
    Verifying,
    Succeeded,
    Failed,
    Cancelled,
}

impl RunState {
    pub fn word(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Verifying => "verifying",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    pub id: RunId,
    pub leader_id: LogicalSessionId,
    pub brief: RunBrief,
    pub plan_revision: u64,
    pub state: RunState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TargetPreference {
    pub agent: Option<Agent>,
    pub model: Option<String>,
    pub permissions: AgentPermissions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TaskBrief {
    pub instructions: String,
    pub scope: Vec<String>,
    pub inputs: Vec<String>,
    pub purpose: TaskPurpose,
    pub target: TargetPreference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPlan {
    pub id: TaskId,
    pub run_id: RunId,
    pub expected_revision: Option<u64>,
    pub brief: TaskBrief,
    pub dependencies: Vec<TaskId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskVersion {
    pub task_id: TaskId,
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Planned,
    Queued,
    Running,
    Blocked,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelSource {
    Explicit,
    Settings,
    #[default]
    AgentDefault,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetSelection {
    pub agent: Agent,
    pub requested_model: Option<String>,
    pub model: Option<String>,
    pub model_source: ModelSource,
    pub permissions: AgentPermissions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptRecord {
    pub id: AttemptId,
    pub task_id: TaskId,
    pub task_revision: u64,
    pub session_id: LogicalSessionId,
    pub target: TargetSelection,
    pub actual_model: Option<String>,
    pub work_area_id: Option<WorkAreaId>,
    pub base_revision: Option<String>,
    pub state: AttemptState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Note,
    Question,
    Answer,
    Report,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Stored,
    Accepted,
    Acknowledged,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryRecord {
    pub message_id: MessageId,
    pub recipient: LogicalSessionId,
    pub state: DeliveryState,
    pub attempts: u64,
    pub last_error: Option<String>,
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
