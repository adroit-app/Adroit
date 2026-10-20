use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! ids {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
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
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRole {
    Leader,
    Worker,
}

/// Ownership assigned when an agent joins a request; never inferred from cwd.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionBinding {
    logical_id: LogicalSessionId,
    assignment: Assignment,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BindingError {
    #[error("a worker cannot be its own leader")]
    OwnLeader,
}

impl SessionBinding {
    pub fn leader(logical_id: LogicalSessionId, run_id: Option<RunId>) -> Self {
        Self {
            logical_id,
            assignment: Assignment::Leader { run_id },
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
            assignment: Assignment::Worker {
                leader_id,
                run_id,
                task_id,
                attempt_id,
            },
        })
    }

    pub fn logical_id(&self) -> LogicalSessionId {
        self.logical_id
    }

    pub fn role(&self) -> SessionRole {
        match self.assignment {
            Assignment::Leader { .. } => SessionRole::Leader,
            Assignment::Worker { .. } => SessionRole::Worker,
        }
    }

    /// The owner of this request, including the session itself for a leader.
    pub fn leader_id(&self) -> LogicalSessionId {
        match self.assignment {
            Assignment::Leader { .. } => self.logical_id,
            Assignment::Worker { leader_id, .. } => leader_id,
        }
    }

    pub fn run_id(&self) -> Option<RunId> {
        match self.assignment {
            Assignment::Leader { run_id } => run_id,
            Assignment::Worker { run_id, .. } => Some(run_id),
        }
    }

    pub fn work(&self) -> Option<(TaskId, AttemptId)> {
        match self.assignment {
            Assignment::Leader { .. } => None,
            Assignment::Worker {
                task_id,
                attempt_id,
                ..
            } => Some((task_id, attempt_id)),
        }
    }
}
