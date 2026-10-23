//! What a session is launched with, and the identities that outlive its terminal.
use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Agent;

macro_rules! ids {
    ($($(#[$meta:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema)]
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
    /// A session Adroit started, for as long as Adroit keeps its record.
    SessionId,
    MessageId,
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

/// Where a launch's model came from.
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

/// What an agent's screen says of it while it starts.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StartupReadiness {
    #[default]
    Spawned,
    Ready,
    AwaitingTrust,
    AwaitingPermission,
    AwaitingInput,
    Stalled,
}
