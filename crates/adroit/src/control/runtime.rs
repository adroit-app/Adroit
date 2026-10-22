//! Private routing snapshots; never part of agent-visible orchestration state.
use serde::{Deserialize, Serialize};

use crate::settings::Settings;

#[derive(Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeLaunch {
    pub settings: Settings,
    #[serde(default)]
    pub target: Option<adroit_model::TargetSelection>,
    #[serde(default)]
    pub developer_instructions: Option<String>,
}

impl std::fmt::Debug for RuntimeLaunch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeLaunch")
            .finish_non_exhaustive()
    }
}

impl RuntimeLaunch {
    pub fn new(mut settings: Settings) -> Self {
        settings
            .codex
            .env
            .entry("CODEX_HOME".into())
            .or_insert_with(|| {
                std::env::var("CODEX_HOME").unwrap_or_else(|_| {
                    crate::paths::home()
                        .join(".codex")
                        .to_string_lossy()
                        .into_owned()
                })
            });
        Self {
            settings,
            target: None,
            developer_instructions: None,
        }
    }

    pub fn with_target(mut self, target: adroit_model::TargetSelection) -> Self {
        self.target = Some(target);
        self
    }
}
