//! Session-local MCP settings. Credentials travel through the environment only.
use std::{
    fmt,
    path::{Path, PathBuf},
};

use adroit_model::{Agent, SessionBinding, SessionRef};
use serde_json::json;

/// The development channel Claude Code loads Adroit's control server as.
pub const CLAUDE_CHANNEL: &str = "server:adroit_control";

#[derive(Clone)]
pub struct ControlContext {
    pub identity: SessionRef,
    pub root: PathBuf,
    token: String,
}

impl fmt::Debug for ControlContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ControlContext")
            .field("identity", &self.identity)
            .field("root", &self.root)
            .field("token", &"[redacted]")
            .finish()
    }
}

impl ControlContext {
    pub fn new(identity: SessionRef, root: PathBuf, token: String) -> Self {
        Self {
            identity,
            root,
            token,
        }
    }

    pub(super) fn validate(&self, binding: Option<&SessionBinding>) -> Result<(), String> {
        if binding.is_none_or(|binding| binding.logical_id() != self.identity.id) {
            return Err(
                "the control identity does not match the explicit session assignment".into(),
            );
        }
        if self.identity.generation == 0 || self.token.is_empty() || !self.root.is_absolute() {
            return Err("the control context needs a generation, credential and absolute endpoint directory".into());
        }
        Ok(())
    }

    pub(super) fn args(&self, agent: Agent, program: &Path) -> Vec<String> {
        match agent {
            Agent::Claude => vec![
                "--mcp-config".into(),
                json!({"mcpServers":{"adroit_control":{
                    "type":"stdio", "command": program, "args":["control","mcp","--channel"],
                    "env":{
                        "ADROIT_CONTROL_TOKEN":"${ADROIT_CONTROL_TOKEN}",
                        "ADROIT_CONTROL_ROOT":"${ADROIT_CONTROL_ROOT}",
                        "ADROIT_SESSION_ID":"${ADROIT_SESSION_ID}",
                        "ADROIT_SESSION_GENERATION":"${ADROIT_SESSION_GENERATION}"
                    }
                }}})
                .to_string(),
                "--dangerously-load-development-channels".into(),
                CLAUDE_CHANNEL.into(),
                "--allowedTools".into(),
                "mcp__adroit_control__*".into(),
            ],
            Agent::Codex => {
                let variables = [
                    "ADROIT_CONTROL_TOKEN",
                    "ADROIT_CONTROL_ROOT",
                    "ADROIT_SESSION_ID",
                    "ADROIT_SESSION_GENERATION",
                    "ADROIT_INSTANCE",
                    "XDG_STATE_HOME",
                    "XDG_CONFIG_HOME",
                    "XDG_DATA_HOME",
                ];
                [
                    format!("mcp_servers.adroit_control.command={}", json!(program)),
                    format!(
                        "mcp_servers.adroit_control.args={}",
                        json!(["control", "mcp"])
                    ),
                    format!("mcp_servers.adroit_control.env_vars={}", json!(variables)),
                    "mcp_servers.adroit_control.enabled=true".into(),
                    "mcp_servers.adroit_control.required=true".into(),
                    "mcp_servers.adroit_control.default_tools_approval_mode=\"approve\"".into(),
                    "mcp_servers.adroit_control.tool_input_schema_max_bytes=65536".into(),
                ]
                .into_iter()
                .flat_map(|value| ["-c".into(), value])
                .collect()
            }
        }
    }

    pub(super) fn environment(&self, binding: &SessionBinding) -> Vec<(String, String)> {
        let mut env = vec![
            ("ADROIT_CONTROL_TOKEN".into(), self.token.clone()),
            (
                "ADROIT_CONTROL_ROOT".into(),
                self.root.to_string_lossy().into_owned(),
            ),
            ("ADROIT_SESSION_ID".into(), self.identity.id.to_string()),
            (
                "ADROIT_SESSION_GENERATION".into(),
                self.identity.generation.to_string(),
            ),
            ("ADROIT_LEADER_ID".into(), binding.leader_id().to_string()),
        ];
        if let Some(run) = binding.run_id() {
            env.push(("ADROIT_RUN_ID".into(), run.to_string()));
        }
        if let Some((task, attempt)) = binding.work() {
            env.push(("ADROIT_TASK_ID".into(), task.to_string()));
            env.push(("ADROIT_ATTEMPT_ID".into(), attempt.to_string()));
        }
        env
    }
}
