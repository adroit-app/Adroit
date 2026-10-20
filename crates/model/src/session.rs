use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{AgentPermissions, SessionBinding};

/// The program that owns a session's conversation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    #[default]
    Claude,
    Codex,
}

impl Agent {
    pub const ALL: [Self; 2] = [Self::Claude, Self::Codex];
    pub const fn key(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    /// The agent after this one, going round to the first after the last.
    pub fn next(self) -> Self {
        let at = Self::ALL.iter().position(|a| *a == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }
}

/// An agent session: one adroit runs, or one running in another
/// terminal that Claude Code lists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Session {
    /// Explicit task ownership, independent of terminal and conversation IDs.
    pub binding: Option<SessionBinding>,
    pub agent: Agent,
    /// Which session this is: the daemon's id for one adroit runs, the
    /// conversation's for one in another terminal.
    pub id: String,
    /// The conversation's id, which stays when a session is taken over and
    /// runs under another id; empty until the conversation tells it.
    pub session_id: String,
    /// The process the session runs in, as Claude Code lists it; for one
    /// adroit runs, unknown until it is listed.
    pub pid: Option<u32>,
    /// Whether Claude Code named the session idle, rather than adroit taking
    /// it as idle for want of a state it knows.
    pub said_idle: bool,
    pub name: String,
    pub cwd: PathBuf,
    pub kind: Kind,
    pub state: State,
    /// When the session started, in milliseconds since the Unix epoch.
    pub started_ms: Option<u64>,
    /// The most recent input sent through Adroit, kept on the daemon session.
    pub last_input_ms: Option<u64>,
    pub permissions: AgentPermissions,
    /// The conversation of the session this one was started from; empty for
    /// one the user started.
    pub started_from: String,
}

impl Session {
    /// Whether the session was started to read only.
    pub fn reads_only(&self) -> bool {
        self.permissions == AgentPermissions::ReadOnly
    }

    /// Whether adroit may stop the session's process and resume it in a
    /// session of its own: one in another terminal, between turns.
    pub fn can_take_over(&self) -> bool {
        self.kind == Kind::Interactive
            && self.pid.is_some()
            && self.state == State::Idle
            && self.said_idle
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Kind {
    /// Runs in another terminal, where adroit cannot show it.
    Interactive,
    /// Runs in adroit's daemon.
    #[default]
    Own,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum State {
    Waiting(Wait),
    Working,
    #[default]
    Idle,
    Done,
    Failed,
}

impl State {
    /// The state in one word.
    pub fn word(&self) -> &'static str {
        match self {
            State::Waiting(_) => "waiting",
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    name: Option<String>,
    cwd: PathBuf,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    started_at: Option<u64>,
}

/// The sessions running in other terminals, from the output of
/// `claude agents --json`.
pub fn parse_agents(json: &str) -> Result<Vec<Session>, ParseError> {
    let raws: Vec<Raw> = serde_json::from_str(json)?;
    Ok(raws
        .into_iter()
        .filter(|r| r.kind.as_deref() == Some("interactive") && r.pid.is_some())
        .map(|raw| {
            let status = raw.status.unwrap_or_default();
            Session {
                binding: None,
                agent: Agent::Claude,
                id: raw.session_id.clone(),
                name: raw.name.unwrap_or_else(|| raw.session_id.clone()),
                session_id: raw.session_id,
                pid: raw.pid,
                said_idle: status == "idle",
                cwd: raw.cwd,
                kind: Kind::Interactive,
                state: match status.as_str() {
                    "busy" => State::Working,
                    _ => State::Idle,
                },
                started_ms: raw.started_at,
                last_input_ms: None,
                permissions: AgentPermissions::default(),
                started_from: String::new(),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(json: &str) -> Session {
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
    fn a_session_in_another_terminal_is_read_from_claude_s_listing() {
        let s = one(r#"[{"pid":1,"cwd":"/r","kind":"interactive","startedAt":5,
            "sessionId":"abc","name":"r-6a","status":"idle"}]"#);
        assert_eq!(s.id, "abc");
        assert_eq!(s.session_id, "abc");
        assert_eq!(s.name, "r-6a");
        assert_eq!(s.cwd, PathBuf::from("/r"));
        assert_eq!(s.kind, Kind::Interactive);
        assert_eq!(s.state, State::Idle);
        assert_eq!(s.started_ms, Some(5));
        assert!(s.can_take_over());
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
    fn a_busy_session_is_working_and_cannot_be_taken_over() {
        let s =
            one(r#"[{"pid":1,"cwd":"/r","kind":"interactive","sessionId":"a","status":"busy"}]"#);
        assert_eq!(s.state, State::Working);
        assert!(!s.can_take_over());
    }

    #[test]
    fn a_session_whose_state_claude_code_did_not_name_is_not_taken_over() {
        let s = one(r#"[{"pid":1,"cwd":"/r","kind":"interactive","sessionId":"a"}]"#);
        assert_eq!(s.state, State::Idle);
        assert!(!s.can_take_over());
    }

    #[test]
    fn a_session_without_a_name_is_named_by_its_id() {
        let s = one(r#"[{"pid":1,"cwd":"/r","kind":"interactive","sessionId":"abc"}]"#);
        assert_eq!(s.name, "abc");
    }

    #[test]
    fn one_adroit_runs_is_not_taken_over() {
        let s = Session {
            kind: Kind::Own,
            pid: Some(1),
            said_idle: true,
            ..Session::default()
        };
        assert!(!s.can_take_over());
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
            State::Waiting(Wait::Other("auth".into()))
        );
        assert_eq!(State::reported("done", None), State::Done);
        assert_eq!(State::reported("error", None), State::Failed);
        assert_eq!(State::reported("idle", None), State::Idle);
        assert_eq!(State::reported("pondering", None), State::Idle);
    }

    #[test]
    fn malformed_output_is_an_error() {
        assert!(parse_agents("not json").is_err());
    }
}
