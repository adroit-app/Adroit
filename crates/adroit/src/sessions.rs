//! The sessions adroit runs in its daemon, as the window knows them: what
//! each one's tags say it is.

use std::path::PathBuf;

use adroit_model::{Kind, Session, State, TerminalProgram, TerminalSession};
use adroit_term::tags;

const KIND: &str = "adroit.kind";
pub const NAME: &str = "adroit.name";
/// The directory a session was started in, which the daemon does not list
/// (felis-terminal/felis#7).
const CWD: &str = "adroit.cwd";
const STARTED: &str = "adroit.started";
const LAST_INPUT: &str = "adroit.last-input";
const PERMISSIONS: &str = "adroit.permissions";
/// The conversation a session holds. The tag keeps its original spelling
/// so sessions started by earlier versions still carry their conversation.
const CONVERSATION: &str = "adroit.claude";
/// The process Claude Code runs in, learned from its listing while the
/// session still holds the conversation it was started with.
const PID: &str = "adroit.pid";
/// The conversation of the session this one was started from.
const STARTED_FROM: &str = "adroit.from";

/// A session the daemon holds, as what it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kept {
    Agent(Session),
    Terminal(TerminalSession),
}

/// The daemon's id as the window writes it.
pub fn hex(id: u128) -> String {
    format!("{id:032x}")
}

/// The daemon's id a session of adroit's goes by in the window.
pub fn id_of(hex: &str) -> Option<u128> {
    (hex.len() == 32)
        .then(|| u128::from_str_radix(hex, 16).ok())
        .flatten()
}

/// Keep the conversation reported by the agent's terminal title on its
/// daemon session, including when the agent switches conversations.
pub fn follow_title(session: &mut Session, title: &str) -> Option<(u128, &'static str, String)> {
    let conversation = crate::agents::adapter(session.agent).conversation_title(title)?;
    let id = id_of(&session.id)?;
    if conversation == session.session_id {
        return None;
    }
    if !session.session_id.is_empty() {
        session.binding = None;
    }
    session.session_id = conversation.clone();
    Some((id, CONVERSATION, conversation))
}

/// Keep newer user input on the daemon session so another window can recover it.
pub fn note_input(session: &mut Session, at: u64) -> Option<(u128, &'static str, String)> {
    let id = id_of(&session.id)?;
    if at == 0 || session.last_input_ms.is_some_and(|held| held >= at) {
        return None;
    }
    session.last_input_ms = Some(at);
    Some((id, LAST_INPUT, at.to_string()))
}

/// What `session` is, by its tags; `None` for one adroit did not start, and
/// for one whose program has exited, which is gone but for a moment.
pub fn read(session: &adroit_term::Session) -> Option<Kept> {
    if session.exited {
        return None;
    }
    let t = &session.tags;
    let name = tags::value(t, NAME).unwrap_or_default();
    let cwd = PathBuf::from(tags::value(t, CWD)?);
    match tags::value(t, KIND)?.as_str() {
        kind if adroit_model::Agent::parse(kind).is_some() => Some(Kept::Agent(Session {
            binding: match tags::value(t, "adroit.binding") {
                Some(raw)
                    if tags::value(t, "adroit.binding-conversation").is_some_and(|bound| {
                        bound == tags::value(t, CONVERSATION).unwrap_or_default()
                    }) =>
                {
                    Some(serde_json::from_str(&raw).ok()?)
                }
                _ => None,
            },
            agent: adroit_model::Agent::parse(kind)?,
            id: hex(session.id),
            session_id: tags::value(t, CONVERSATION).unwrap_or_default(),
            pid: tags::value(t, PID).and_then(|p| p.parse().ok()),
            said_idle: false,
            name,
            cwd,
            kind: Kind::Own,
            state: session
                .status
                .as_ref()
                .map(|s| State::reported(&s.state, s.kind.as_deref()))
                .unwrap_or(State::Starting),
            started_ms: tags::value(t, STARTED).and_then(|s| s.parse().ok()),
            last_input_ms: tags::value(t, LAST_INPUT).and_then(|s| s.parse().ok()),
            permissions: match tags::value(t, PERMISSIONS) {
                Some(raw) => serde_json::from_str(&raw).ok()?,
                None => adroit_model::AgentPermissions::Default,
            },
            started_from: tags::value(t, STARTED_FROM).unwrap_or_default(),
        })),
        "terminal" => Some(Kept::Terminal(TerminalSession {
            id: session.id,
            resource_id: tags::value(t, "adroit.resource")?.parse().ok()?,
            name,
            cwd,
            program: match tags::value(t, "adroit.program")?.as_str() {
                "shell" => TerminalProgram::Shell,
                "neovim" => TerminalProgram::Neovim,
                _ => return None,
            },
            binding: match tags::value(t, "adroit.terminal-binding") {
                Some(raw) => Some(serde_json::from_str(&raw).ok()?),
                None => None,
            },
        })),
        _ => None,
    }
}

/// Read Claude Code's `listed` sessions against adroit's `own`: each of `own`
/// learns the process that holds its conversation, and follows that process
/// to the conversation `/clear` or `/resume` moves it to. The sessions of
/// `listed` that run in other terminals, and what was learned, as each
/// session's daemon id, the tag's key and its new value.
pub fn follow(
    own: &mut [Session],
    listed: Vec<Session>,
) -> (Vec<Session>, Vec<(u128, &'static str, String)>) {
    let mut learned = Vec::new();
    for s in own
        .iter_mut()
        .filter(|s| s.agent == adroit_model::Agent::Claude)
    {
        let Some(id) = id_of(&s.id) else { continue };
        match s.pid {
            Some(pid) => {
                let moved = listed
                    .iter()
                    .find(|l| l.pid == Some(pid))
                    .filter(|l| !l.session_id.is_empty() && l.session_id != s.session_id);
                if let Some(l) = moved {
                    s.session_id = l.session_id.clone();
                    s.binding = None;
                    learned.push((id, CONVERSATION, s.session_id.clone()));
                }
            }
            None => {
                let holding = listed
                    .iter()
                    .filter(|_| !s.session_id.is_empty())
                    .find(|l| l.session_id == s.session_id)
                    .and_then(|l| l.pid);
                if let Some(pid) = holding {
                    s.pid = Some(pid);
                    learned.push((id, PID, pid.to_string()));
                }
            }
        }
    }
    let elsewhere = listed
        .into_iter()
        .filter(|l| {
            !own.iter().any(|s| {
                s.agent == l.agent
                    && (s.session_id == l.session_id || (s.pid.is_some() && s.pid == l.pid))
            })
        })
        .collect();
    (elsewhere, learned)
}

/// The tags a Claude Code session adroit starts or resumes carries, holding
/// the conversation `conversation` from the moment it is spawned.
pub fn claude_tags(
    name: &str,
    cwd: &std::path::Path,
    conversation: &str,
    permissions: &adroit_model::AgentPermissions,
    started_ms: u64,
    started_from: Option<&str>,
) -> Vec<String> {
    let mut out = tags::tagged(KIND, "claude");
    out.extend(tags::tagged(NAME, name));
    out.extend(tags::tagged(CWD, &cwd.to_string_lossy()));
    out.extend(tags::tagged(STARTED, &started_ms.to_string()));
    out.extend(tags::tagged(CONVERSATION, conversation));
    out.extend(tags::tagged(
        PERMISSIONS,
        &serde_json::to_string(permissions).expect("serializable permissions"),
    ));
    if let Some(from) = started_from {
        out.extend(tags::tagged(STARTED_FROM, from));
    }
    out
}

/// Tags for any supported agent; Claude retains its original wire format.
#[allow(clippy::too_many_arguments)]
pub fn agent_tags(
    agent: adroit_model::Agent,
    name: &str,
    cwd: &std::path::Path,
    conversation: &str,
    permissions: &adroit_model::AgentPermissions,
    started_ms: u64,
    started_from: Option<&str>,
) -> Vec<String> {
    let mut out = claude_tags(
        name,
        cwd,
        conversation,
        permissions,
        started_ms,
        started_from,
    );
    out.retain(|t| {
        !t.starts_with("adroit.kind=")
            && (!conversation.is_empty() || !t.starts_with("adroit.claude="))
    });
    out.extend(tags::tagged(KIND, agent.key()));
    out
}

/// The tags a shell adroit starts carries, saying what it `runs`.
pub fn terminal_tags(
    name: &str,
    cwd: &std::path::Path,
    program: TerminalProgram,
    resource_id: adroit_model::TerminalResourceId,
    binding: Option<&adroit_model::TerminalBinding>,
) -> Vec<String> {
    let mut out = tags::tagged(KIND, "terminal");
    out.extend(tags::tagged(NAME, name));
    out.extend(tags::tagged(CWD, &cwd.to_string_lossy()));
    out.extend(tags::tagged("adroit.resource", &resource_id.to_string()));
    out.extend(tags::tagged(
        "adroit.program",
        match program {
            TerminalProgram::Shell => "shell",
            TerminalProgram::Neovim => "neovim",
        },
    ));
    if let Some(binding) = binding {
        out.extend(tags::tagged(
            "adroit.terminal-binding",
            &serde_json::to_string(binding).expect("serializable terminal binding"),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_worker_waiting_before_its_first_conversation_remains_openable_under_its_leader() {
        use crate::execution::{StartRequest, resolve};
        use crate::leader_navigation::{RowRole, project};
        use crate::nav::Item;
        use crate::sessions::{Kept, read};
        use crate::settings::Settings;
        use adroit_model::*;
        use std::path::Path;

        let leader_id = LogicalSessionId::new();
        let run_id = RunId::new();
        let bindings = [
            SessionBinding::leader(leader_id, Some(run_id)),
            SessionBinding::worker(
                LogicalSessionId::new(),
                leader_id,
                run_id,
                TaskId::new(),
                AttemptId::new(),
            )
            .unwrap(),
        ];
        let mut snapshot = ControlSnapshot::default();
        let mut sessions = Vec::new();
        for (index, binding) in bindings.into_iter().enumerate() {
            let launch = resolve(
                &Settings::default(),
                StartRequest {
                    binding: Some(binding.clone()),
                    agent: Agent::Codex,
                    model: Some("unavailable-model".into()),
                    cwd: "/repo".into(),
                    ..Default::default()
                },
                Path::new("/adroit"),
            )
            .unwrap();
            let terminal = adroit_term::Session {
                id: index as u128 + 1,
                tags: launch.spawn.tags,
                title: None,
                exited: false,
                status: None,
                sequence: index as u64,
            };
            let mut unmarked = terminal.clone();
            unmarked
                .tags
                .retain(|tag| !tag.starts_with("adroit.binding-conversation="));
            let Some(Kept::Agent(unmarked)) = read(&unmarked) else {
                panic!("native agent terminal");
            };
            assert!(unmarked.binding.is_none());
            let Some(Kept::Agent(session)) = read(&terminal) else {
                panic!("native agent terminal");
            };
            assert!(session.session_id.is_empty());
            assert_eq!(session.binding, Some(binding.clone()));
            snapshot.sessions.push(ManagedSession {
                binding,
                agent: Agent::Codex,
                generation: 1,
                state: ManagedSessionState::Running,
                permissions: AgentPermissions::Default,
                cwd: Some(session.cwd.clone()),
                terminal_id: Some(session.id.clone()),
                conversation_id: None,
            });
            sessions.push(session);
        }
        snapshot.runs.push(RunRecord {
            id: run_id,
            leader_id,
            brief: RunBrief::default(),
            plan_revision: 1,
            candidate_id: None,
            state: RunState::Active,
        });
        let tree = build(&sessions, &[], &Default::default(), &[], None);
        let rows = project(&tree, &snapshot);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].role, RowRole::Leader);
        assert_eq!(rows[1].role, RowRole::Worker);
        assert_eq!(rows[1].item, Item::Session(sessions[1].id.clone()));
        assert!(tree.find(&sessions[1].id).is_some());
    }

    #[test]
    fn changing_conversation_does_not_transfer_a_worker_assignment() {
        use adroit_model::{AttemptId, LogicalSessionId, RunId, SessionBinding, TaskId};
        let binding = SessionBinding::worker(
            LogicalSessionId::new(),
            LogicalSessionId::new(),
            RunId::new(),
            TaskId::new(),
            AttemptId::new(),
        )
        .unwrap();
        let mut tags = agent_tags(
            adroit_model::Agent::Codex,
            "review",
            std::path::Path::new("/repo"),
            "first",
            &adroit_model::AgentPermissions::Default,
            1,
            None,
        );
        tags.extend(adroit_term::tags::tagged(
            "adroit.binding",
            &serde_json::to_string(&binding).unwrap(),
        ));
        tags.extend(adroit_term::tags::tagged(
            "adroit.binding-conversation",
            "first",
        ));
        let Some(Kept::Agent(mut session)) = read(&kept(42, tags.clone())) else {
            panic!("agent")
        };
        assert_eq!(session.binding, Some(binding));
        follow_title(&mut session, "Codex · 11234567-89ab-cdef-0123-456789abcdef");
        assert!(session.binding.is_none());
        tags.retain(|tag| !tag.starts_with(&format!("{CONVERSATION}=")));
        tags.extend(adroit_term::tags::tagged(CONVERSATION, "second"));
        let Some(Kept::Agent(session)) = read(&kept(42, tags)) else {
            panic!("agent")
        };
        assert!(session.binding.is_none());
    }

    #[test]
    fn user_input_times_are_kept_on_the_session_and_older_windows_cannot_replace_them() {
        let mut session = Session {
            id: hex(42),
            kind: Kind::Own,
            ..Session::default()
        };
        let tag = note_input(&mut session, 1000).unwrap();
        assert_eq!(session.last_input_ms, Some(1000));
        assert_eq!(tag, (42, LAST_INPUT, "1000".into()));
        assert!(note_input(&mut session, 999).is_none());
        assert!(note_input(&mut session, 1000).is_none());
        let tags = [
            tags::tagged(KIND, "claude"),
            tags::tagged(CWD, "/tmp"),
            tags::tagged(LAST_INPUT, "1000"),
        ]
        .concat();
        let Some(Kept::Agent(restored)) = read(&kept(42, tags)) else {
            panic!("an agent session")
        };
        assert_eq!(restored.last_input_ms, Some(1000));
    }

    #[test]
    fn codex_titles_keep_separate_conversations_and_follow_switches() {
        use adroit_model::{Agent, Session};
        let first = "01234567-89ab-cdef-0123-456789abcdef";
        let second = "11234567-89ab-cdef-0123-456789abcdef";
        let mut session = Session {
            id: super::hex(1),
            agent: Agent::Codex,
            ..Default::default()
        };
        assert_eq!(
            super::follow_title(&mut session, &format!("Codex · {first}")),
            Some((1, super::CONVERSATION, first.into()))
        );
        assert_eq!(
            super::follow_title(&mut session, &format!("Codex · {first}")),
            None
        );
        assert_eq!(super::follow_title(&mut session, "Codex · project"), None);
        assert_eq!(session.session_id, first);
        let mut other = session.clone();
        other.id = super::hex(2);
        assert_eq!(
            super::follow_title(&mut other, &format!("Codex · {second}")),
            Some((2, super::CONVERSATION, second.into()))
        );
        assert_eq!(session.session_id, first);
        assert_eq!(
            super::follow_title(&mut session, &format!("Codex · {second}")),
            Some((1, super::CONVERSATION, second.into()))
        );
        session.agent = Agent::Claude;
        assert_eq!(
            super::follow_title(&mut session, &format!("Codex · {first}")),
            None
        );
        assert_eq!(session.session_id, second);
    }

    use adroit_model::Wait;
    use adroit_term::Status;

    use super::*;

    fn kept(id: u128, tags: Vec<String>) -> adroit_term::Session {
        adroit_term::Session {
            id,
            tags,
            title: None,
            exited: false,
            status: None,
            sequence: 1,
        }
    }

    #[test]
    fn codex_keeps_its_agent_and_status_without_a_claude_conversation() {
        let mut s = kept(
            8,
            agent_tags(
                adroit_model::Agent::Codex,
                "fix",
                std::path::Path::new("/api"),
                "",
                &adroit_model::AgentPermissions::ReadOnly,
                42,
                None,
            ),
        );
        s.status = Some(Status {
            state: "working".into(),
            kind: None,
            msg: None,
        });
        let Some(Kept::Agent(c)) = read(&s) else {
            panic!("not a session")
        };
        assert_eq!(c.agent, adroit_model::Agent::Codex);
        assert_eq!(c.state, State::Working);
        assert!(c.reads_only());
        assert!(c.session_id.is_empty());
        let mut own = [c];
        let (_, learned) = follow(&mut own, vec![listed("claude-uuid", 42)]);
        assert!(learned.is_empty());
    }

    #[test]
    fn a_claude_session_is_read_back_from_its_tags_and_status() {
        let mut s = kept(
            7,
            claude_tags(
                "fix",
                std::path::Path::new("/api"),
                "uuid",
                &adroit_model::AgentPermissions::ReadOnly,
                42,
                None,
            ),
        );
        s.status = Some(Status {
            state: "blocked".into(),
            kind: Some("permission".into()),
            msg: None,
        });
        let Some(Kept::Agent(c)) = read(&s) else {
            panic!("not a session");
        };
        assert_eq!(c.id, hex(7));
        assert_eq!(id_of(&c.id), Some(7));
        assert_eq!(c.name, "fix");
        assert_eq!(c.cwd, PathBuf::from("/api"));
        assert_eq!(c.session_id, "uuid");
        assert_eq!(c.started_ms, Some(42));
        assert!(c.reads_only());
        assert_eq!(c.kind, Kind::Own);
        assert_eq!(c.state, State::Waiting(Wait::Permission));
    }

    #[test]
    fn a_session_goes_by_its_conversation_before_it_reports_anything() {
        let s = kept(
            7,
            claude_tags(
                "fix",
                std::path::Path::new("/api"),
                "uuid",
                &adroit_model::AgentPermissions::Default,
                0,
                None,
            ),
        );
        let Some(Kept::Agent(c)) = read(&s) else {
            panic!("not a session");
        };
        assert_eq!(c.session_id, "uuid");
        assert!(!c.reads_only());
        assert_eq!(c.started_from, "");
    }

    #[test]
    fn a_session_started_from_another_keeps_the_conversation_it_came_from() {
        let s = kept(
            7,
            claude_tags(
                "fix",
                std::path::Path::new("/api"),
                "uuid",
                &adroit_model::AgentPermissions::Default,
                0,
                Some("parent-uuid"),
            ),
        );
        let Some(Kept::Agent(c)) = read(&s) else {
            panic!("not a session");
        };
        assert_eq!(c.started_from, "parent-uuid");
    }

    #[test]
    fn a_session_that_reported_nothing_is_starting() {
        let s = kept(
            7,
            claude_tags(
                "fix",
                std::path::Path::new("/api"),
                "uuid",
                &adroit_model::AgentPermissions::Default,
                0,
                None,
            ),
        );
        let Some(Kept::Agent(c)) = read(&s) else {
            panic!("not a session");
        };
        assert_eq!(c.state, State::Starting);
        assert!(!c.reads_only());
    }

    #[test]
    fn a_terminal_keeps_its_resource_id_program_and_optional_owner() {
        for program in [TerminalProgram::Shell, TerminalProgram::Neovim] {
            let resource_id = adroit_model::TerminalResourceId::new();
            let binding = adroit_model::TerminalBinding {
                run_id: adroit_model::RunId::new(),
                worker_id: None,
            };
            let s = kept(
                9,
                terminal_tags(
                    "tool",
                    std::path::Path::new("/api"),
                    program,
                    resource_id,
                    Some(&binding),
                ),
            );
            assert_eq!(
                read(&s),
                Some(Kept::Terminal(TerminalSession {
                    id: 9,
                    resource_id,
                    name: "tool".into(),
                    cwd: "/api".into(),
                    program,
                    binding: Some(binding),
                }))
            );
        }
    }

    #[test]
    fn a_session_adroit_did_not_start_or_one_that_exited_is_not_adroit_s() {
        assert_eq!(read(&kept(1, vec!["other=1".into()])), None);
        let mut s = kept(
            9,
            terminal_tags(
                "shell 1",
                std::path::Path::new("/api"),
                TerminalProgram::Shell,
                adroit_model::TerminalResourceId::new(),
                None,
            ),
        );
        s.exited = true;
        assert_eq!(read(&s), None);
    }

    fn own(id: u128, conversation: &str, pid: Option<u32>) -> Session {
        let mut tags = claude_tags(
            "fix",
            std::path::Path::new("/api"),
            conversation,
            &adroit_model::AgentPermissions::Default,
            0,
            None,
        );
        if let Some(pid) = pid {
            tags.extend(tags::tagged(PID, &pid.to_string()));
        }
        let Some(Kept::Agent(c)) = read(&kept(id, tags)) else {
            panic!("not a session");
        };
        c
    }

    fn listed(conversation: &str, pid: u32) -> Session {
        Session {
            id: conversation.into(),
            session_id: conversation.into(),
            pid: Some(pid),
            kind: Kind::Interactive,
            ..Default::default()
        }
    }

    #[test]
    fn a_session_learns_the_process_holding_its_conversation() {
        let mut ours = [own(7, "uuid", None)];

        let (elsewhere, learned) = follow(&mut ours, vec![listed("uuid", 42), listed("other", 9)]);

        assert_eq!(ours[0].pid, Some(42));
        assert_eq!(learned, vec![(7, PID, "42".to_string())]);
        assert_eq!(elsewhere, vec![listed("other", 9)]);
    }

    #[test]
    fn a_session_follows_its_process_to_the_conversation_it_moves_to() {
        let mut ours = [own(7, "uuid", Some(42))];

        let (elsewhere, learned) = follow(&mut ours, vec![listed("cleared", 42)]);

        assert_eq!(ours[0].session_id, "cleared");
        assert_eq!(learned, vec![(7, CONVERSATION, "cleared".to_string())]);
        assert!(elsewhere.is_empty());
    }

    #[test]
    fn a_session_whose_conversation_and_process_are_known_learns_nothing() {
        let mut ours = [own(7, "uuid", Some(42))];

        let (elsewhere, learned) = follow(&mut ours, vec![listed("uuid", 42)]);

        assert_eq!(ours[0].session_id, "uuid");
        assert!(learned.is_empty());
        assert!(elsewhere.is_empty());
    }

    #[test]
    fn the_process_a_session_runs_in_is_read_back_from_its_tags() {
        assert_eq!(own(7, "uuid", Some(42)).pid, Some(42));
        assert_eq!(own(7, "uuid", None).pid, None);
    }

    #[test]
    fn only_a_daemon_id_reads_as_one() {
        assert_eq!(id_of("not an id"), None);
        assert_eq!(id_of(&hex(u128::MAX)), Some(u128::MAX));
    }
}
