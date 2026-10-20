//! Private routing snapshots; never part of agent-visible orchestration state.
use serde::{Deserialize, Serialize};

use crate::settings::Settings;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeLaunch {
    pub settings: Settings,
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
        Self { settings }
    }
}
