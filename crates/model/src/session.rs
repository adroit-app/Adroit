use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{AgentPermissions, SessionId};

/// The program that owns a session's conversation.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    #[default]
    Claude,
    Codex,
    Pi,
    OpenCode,
}

impl Agent {
    pub const ALL: [Self; 4] = [Self::Claude, Self::Codex, Self::Pi, Self::OpenCode];
    pub const fn key(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Pi => "pi",
            Self::OpenCode => "opencode",
        }
    }

    /// What the user calls the agent.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Pi => "Pi",
            Self::OpenCode => "OpenCode",
        }
    }

    /// The agent after this one, going round to the first after the last.
    pub fn next(self) -> Self {
        let at = Self::ALL.iter().position(|a| *a == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    /// The agent before this one, going round to the last before the first.
    pub fn prev(self) -> Self {
        let at = Self::ALL.iter().position(|a| *a == self).unwrap_or(0);
        Self::ALL[(at + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "pi" => Some(Self::Pi),
            "opencode" => Some(Self::OpenCode),
            _ => None,
        }
    }
}

/// An agent session adroit runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Adroit's record of the session, for one Adroit started.
    pub delegation: Option<SessionId>,
    /// The session that started this one; none for one the user started.
    pub parent: Option<SessionId>,
    pub agent: Agent,
    /// The profile it started from, and the connection that runs in; the
    /// agent's own for one started before sessions were.
    pub profile: crate::profile::ProfileId,
    pub connection: crate::profile::ConnectionId,
    /// Which session this is: the daemon's id.
    pub id: String,
    /// The conversation's id; empty until the conversation tells it.
    pub session_id: String,
    /// The process the session runs in, as Claude Code lists it; unknown
    /// until it is listed.
    pub pid: Option<u32>,
    pub name: String,
    pub cwd: PathBuf,
    pub state: State,
    /// When the session started, in milliseconds since the Unix epoch.
    pub started_ms: Option<u64>,
    /// The most recent input sent through Adroit, kept on the daemon session.
    pub last_input_ms: Option<u64>,
    pub permissions: AgentPermissions,
    /// Whether the session runs with adroit's editor as its external editor,
    /// so that asking it for its input hands the input to the window; false
    /// when that is not known, as for one started by an earlier version.
    pub adroit_editor: bool,
}

impl Session {
    /// Whether the session was started to read only.
    pub fn reads_only(&self) -> bool {
        self.permissions == AgentPermissions::ReadOnly
    }
}

/// A conversation Claude Code lists as held by a process running now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    pub session_id: String,
    pub pid: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Waiting(Wait),
    /// Started by adroit and not yet reported on: Claude Code runs no hooks
    /// before the folder is trusted, so it may be asking that.
    Starting,
    Working,
    #[default]
    Idle,
    Done,
    Failed,
}

impl State {
    /// Whether every key typed goes to the session as an answer.
    pub fn takes_every_key(&self) -> bool {
        matches!(self, State::Waiting(_) | State::Starting)
    }

    /// The state in one word.
    pub fn word(&self) -> &'static str {
        match self {
            State::Waiting(_) => "waiting",
            State::Starting => "starting",
            State::Working => "working",
            State::Idle => "idle",
            State::Done => "done",
            State::Failed => "failed",
        }
    }

    /// The state a program reported of itself in OSC 7501: its `state`,
    /// and for `blocked` what it waits on.
    pub fn reported(state: &str, kind: Option<&str>) -> State {
        match state {
            "working" => State::Working,
            "blocked" => State::Waiting(match kind {
                Some("permission") => Wait::Permission,
                Some("question") => Wait::Input,
                Some("auth") => Wait::Other("a login".into()),
                Some(other) => Wait::Other(other.to_string()),
                None => Wait::Other("an answer".into()),
            }),
            "done" => State::Done,
            "error" => State::Failed,
            _ => State::Idle,
        }
    }
}

/// What a waiting session is waiting for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Wait {
    Permission,
    Input,
    Other(String),
}

impl std::fmt::Display for Wait {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Wait::Permission => f.write_str("permission prompt"),
            Wait::Input => f.write_str("waiting for input"),
            Wait::Other(s) => f.write_str(s),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("cannot read the session list: {0}")]
pub struct ParseError(#[from] serde_json::Error);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Raw {
    session_id: String,
    #[serde(default)]
    pid: Option<u32>,
    #[serde(default)]
    kind: Option<String>,
}

/// The conversations running in a terminal of their own, adroit's sessions
/// among them, from the output of `claude agents --json`.
pub fn parse_agents(json: &str) -> Result<Vec<Running>, ParseError> {
    let raws: Vec<Raw> = serde_json::from_str(json)?;
    Ok(raws
        .into_iter()
        .filter(|r| r.kind.as_deref() == Some("interactive"))
        .filter_map(|raw| {
            Some(Running {
                session_id: raw.session_id,
                pid: raw.pid?,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_agent_is_called_by_its_program_s_own_name() {
        assert_eq!(Agent::Claude.label(), "Claude Code");
        assert_eq!(Agent::Codex.label(), "Codex");
        assert_eq!(Agent::Pi.label(), "Pi");
        assert_eq!(Agent::parse("pi"), Some(Agent::Pi));
        assert_eq!(Agent::OpenCode.label(), "OpenCode");
        assert_eq!(Agent::parse("opencode"), Some(Agent::OpenCode));
        assert_eq!(
            serde_json::to_value(Agent::OpenCode).unwrap(),
            Agent::OpenCode.key()
        );
    }

    fn one(json: &str) -> Running {
        parse_agents(json).unwrap().remove(0)
    }

    #[test]
    fn switching_the_agent_goes_through_every_one_and_round_again() {
        let mut seen = vec![Agent::Claude];
        for _ in 1..Agent::ALL.len() {
            seen.push(seen.last().unwrap().next());
        }
        assert_eq!(seen, Agent::ALL);
        assert_eq!(seen.last().unwrap().next(), Agent::Claude);
    }

    #[test]
    fn switching_the_agent_back_undoes_switching_it() {
        for agent in Agent::ALL {
            assert_eq!(agent.next().prev(), agent);
        }
        assert_eq!(Agent::Claude.prev(), Agent::ALL[Agent::ALL.len() - 1]);
    }

    #[test]
    fn a_running_conversation_is_read_from_claude_s_listing_with_its_process() {
        let s = one(r#"[{"pid":1,"cwd":"/r","kind":"interactive","startedAt":5,
            "sessionId":"abc","name":"r-6a","status":"idle"}]"#);
        assert_eq!(
            s,
            Running {
                session_id: "abc".into(),
                pid: 1
            }
        );
    }

    #[test]
    fn claude_s_background_sessions_and_those_without_a_process_are_not_listed() {
        let json = r#"[
            {"id":"j","cwd":"/r","kind":"background","sessionId":"a","state":"done"},
            {"pid":7,"id":"k","cwd":"/r","kind":"background","sessionId":"b","status":"idle"},
            {"cwd":"/r","kind":"interactive","sessionId":"c","status":"idle"}
        ]"#;
        assert!(parse_agents(json).unwrap().is_empty());
    }

    #[test]
    fn a_reported_state_is_read_with_what_it_waits_on() {
        assert_eq!(State::reported("working", None), State::Working);
        assert_eq!(
            State::reported("blocked", Some("permission")),
            State::Waiting(Wait::Permission)
        );
        assert_eq!(
            State::reported("blocked", Some("question")),
            State::Waiting(Wait::Input)
        );
        assert_eq!(
            State::reported("blocked", Some("auth")),
            State::Waiting(Wait::Other("a login".into()))
        );
        assert_eq!(State::reported("done", None), State::Done);
        assert_eq!(State::reported("error", None), State::Failed);
        assert_eq!(State::reported("idle", None), State::Idle);
        assert_eq!(State::reported("pondering", None), State::Idle);
    }

    #[test]
    fn keys_answer_a_session_waiting_on_a_choice_or_still_starting() {
        assert!(State::Waiting(Wait::Input).takes_every_key());
        assert!(State::Starting.takes_every_key());
        assert!(!State::Idle.takes_every_key());
        assert!(!State::Working.takes_every_key());
        assert!(!State::Done.takes_every_key());
        assert!(!State::Failed.takes_every_key());
    }

    #[test]
    fn malformed_output_is_an_error() {
        assert!(parse_agents("not json").is_err());
    }
}
