use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

mod records;
pub use records::*;

macro_rules! ids {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
        #[serde(transparent)]
        #[schemars(description = "A typed orchestration UUID.")]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(value).map(Self)
            }
        }
    )+};
}

ids!(
    LogicalSessionId,
    RunId,
    TaskId,
    AttemptId,
    MessageId,
    RequestId,
    WorkAreaId,
    CoordinationId,
    CandidateId,
    DraftId,
    TerminalResourceId,
);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(
    tag = "profile",
    content = "options",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AgentPermissions {
    #[default]
    Default,
    ReadOnly,
    Custom(Box<CustomPermissions>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "agent", rename_all = "lowercase", deny_unknown_fields)]
pub enum CustomPermissions {
    Claude {
        permission_mode: String,
        disallowed_tools: Vec<String>,
    },
    Codex {
        approval_policy: String,
        sandbox_mode: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRole {
    Leader,
    Worker,
}

/// Ownership assigned when an agent joins a request; never inferred from cwd.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "BindingData", into = "BindingData")]
pub struct SessionBinding {
    logical_id: LogicalSessionId,
    assignment: Box<Assignment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Assignment {
    Leader {
        run_id: Option<RunId>,
    },
    Worker {
        leader_id: LogicalSessionId,
        run_id: RunId,
        task_id: TaskId,
        attempt_id: AttemptId,
    },
}

#[derive(Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "role", rename_all = "snake_case", deny_unknown_fields)]
enum BindingData {
    Leader {
        logical_id: LogicalSessionId,
        run_id: Option<RunId>,
    },
    Worker {
        logical_id: LogicalSessionId,
        leader_id: LogicalSessionId,
        run_id: RunId,
        task_id: TaskId,
        attempt_id: AttemptId,
    },
}

impl From<SessionBinding> for BindingData {
    fn from(binding: SessionBinding) -> Self {
        match *binding.assignment {
            Assignment::Leader { run_id } => Self::Leader {
                logical_id: binding.logical_id,
                run_id,
            },
            Assignment::Worker {
                leader_id,
                run_id,
                task_id,
                attempt_id,
            } => Self::Worker {
                logical_id: binding.logical_id,
                leader_id,
                run_id,
                task_id,
                attempt_id,
            },
        }
    }
}

impl TryFrom<BindingData> for SessionBinding {
    type Error = BindingError;

    fn try_from(binding: BindingData) -> Result<Self, Self::Error> {
        match binding {
            BindingData::Leader { logical_id, run_id } => Ok(Self::leader(logical_id, run_id)),
            BindingData::Worker {
                logical_id,
                leader_id,
                run_id,
                task_id,
                attempt_id,
            } => Self::worker(logical_id, leader_id, run_id, task_id, attempt_id),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BindingError {
    #[error("a worker cannot be its own leader")]
    OwnLeader,
}

impl SessionBinding {
    pub fn leader(logical_id: LogicalSessionId, run_id: Option<RunId>) -> Self {
        Self {
            logical_id,
            assignment: Box::new(Assignment::Leader { run_id }),
        }
    }

    pub fn worker(
        logical_id: LogicalSessionId,
        leader_id: LogicalSessionId,
        run_id: RunId,
        task_id: TaskId,
        attempt_id: AttemptId,
    ) -> Result<Self, BindingError> {
        if logical_id == leader_id {
            return Err(BindingError::OwnLeader);
        }
        Ok(Self {
            logical_id,
            assignment: Box::new(Assignment::Worker {
                leader_id,
                run_id,
                task_id,
                attempt_id,
            }),
        })
    }

    pub fn logical_id(&self) -> LogicalSessionId {
        self.logical_id
    }

    pub fn role(&self) -> SessionRole {
        match *self.assignment {
            Assignment::Leader { .. } => SessionRole::Leader,
            Assignment::Worker { .. } => SessionRole::Worker,
        }
    }

    /// The owner of this request, including the session itself for a leader.
    pub fn leader_id(&self) -> LogicalSessionId {
        match *self.assignment {
            Assignment::Leader { .. } => self.logical_id,
            Assignment::Worker { leader_id, .. } => leader_id,
        }
    }

    pub fn run_id(&self) -> Option<RunId> {
        match *self.assignment {
            Assignment::Leader { run_id } => run_id,
            Assignment::Worker { run_id, .. } => Some(run_id),
        }
    }

    pub fn work(&self) -> Option<(TaskId, AttemptId)> {
        match *self.assignment {
            Assignment::Leader { .. } => None,
            Assignment::Worker {
                task_id,
                attempt_id,
                ..
            } => Some((task_id, attempt_id)),
        }
    }
}
