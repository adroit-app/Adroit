//! The sessions adroit runs in its daemon, as the window knows them: what
//! each one's tags say it is.

use std::path::PathBuf;

use adroit_model::{Session, State, TerminalProgram, TerminalSession};
use adroit_term::tags;

use crate::delegation::launch::{PARENT_TAG, SESSION_TAG};

const KIND: &str = "adroit.kind";
pub const NAME: &str = "adroit.name";
/// The directory a session was started in, which the daemon does not list
/// (felis-terminal/felis#7).
const CWD: &str = "adroit.cwd";
const STARTED: &str = "adroit.started";
/// The file a Neovim was started on.
const FILE: &str = "adroit.file";
const COMMAND: &str = "adroit.command";
const LAST_INPUT: &str = "adroit.last-input";
const PERMISSIONS: &str = "adroit.permissions";
/// The conversation a session holds. The tag keeps its original spelling
/// so sessions started by earlier versions still carry their conversation.
const CONVERSATION: &str = "adroit.claude";
/// The process Claude Code runs in, learned from its listing while the
/// session still holds the conversation it was started with.
const PID: &str = "adroit.pid";
/// Present on a session started with adroit's editor as its external editor.
const EDITOR: &str = "adroit.editor";

/// A session the daemon holds, as what it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kept {
    Agent(Session),
    Terminal(TerminalSession),
}

pub use adroit_model::daemon::{hex, id_of};

/// Keep the conversation reported by the agent's terminal title on its
/// daemon session, including when the agent switches conversations.
pub fn follow_title(session: &mut Session, title: &str) -> Option<(u128, &'static str, String)> {
    let conversation = adroit_agents::adapter(session.agent).conversation_title(title)?;
    let id = id_of(&session.id)?;
    if conversation == session.session_id {
        return None;
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
            delegation: tags::value(t, SESSION_TAG).and_then(|id| id.parse().ok()),
            parent: tags::value(t, PARENT_TAG).and_then(|id| id.parse().ok()),
            agent: adroit_model::Agent::parse(kind)?,
            id: hex(session.id),
            session_id: tags::value(t, CONVERSATION).unwrap_or_default(),
            pid: tags::value(t, PID).and_then(|p| p.parse().ok()),
            name,
            cwd,
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
            adroit_editor: tags::value(t, EDITOR).is_some(),
        })),
        "terminal" => Some(Kept::Terminal(TerminalSession {
            id: session.id,
            resource_id: tags::value(t, "adroit.resource")?.parse().ok()?,
            name,
            cwd,
            program: match tags::value(t, "adroit.program")?.as_str() {
                "shell" => TerminalProgram::Shell,
                "neovim" => TerminalProgram::Neovim,
                "command" => TerminalProgram::Command,
                _ => return None,
            },
            file: tags::value(t, FILE).map(PathBuf::from),
            command: match tags::value(t, COMMAND) {
                Some(raw) => serde_json::from_str(&raw).ok()?,
                None => Vec::new(),
            },
        })),
        _ => None,
    }
}

/// The tag that keeps on its daemon session what a session `learned` from
/// its agent's listing: the session's daemon id, the tag's key and its value.
pub fn learned_tag((id, learned): (u128, adroit_agents::Learned)) -> (u128, &'static str, String) {
    match learned {
        adroit_agents::Learned::Conversation(conversation) => (id, CONVERSATION, conversation),
        adroit_agents::Learned::Pid(pid) => (id, PID, pid.to_string()),
    }
}

/// The tags a Claude Code session adroit starts or resumes carries, holding
/// the conversation `conversation` from the moment it is spawned.
pub fn claude_tags(
    name: &str,
    cwd: &std::path::Path,
    conversation: &str,
    permissions: &adroit_model::AgentPermissions,
    started_ms: u64,
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
    out
}

/// Tags for any supported agent; Claude retains its original wire format.
pub fn agent_tags(
    agent: adroit_model::Agent,
    name: &str,
    cwd: &std::path::Path,
    conversation: &str,
    permissions: &adroit_model::AgentPermissions,
    started_ms: u64,
) -> Vec<String> {
    let mut out = claude_tags(name, cwd, conversation, permissions, started_ms);
    out.retain(|t| {
        !t.starts_with("adroit.kind=")
            && (!conversation.is_empty() || !t.starts_with("adroit.claude="))
    });
    out.extend(tags::tagged(KIND, agent.key()));
    out
}

/// The tag saying a session started with `env` runs with adroit's editor,
/// if it does.
pub fn editor_tags(env: &[(String, String)]) -> Vec<String> {
    if crate::editor::runs_script(env, &crate::editor::current_script()) {
        tags::tagged(EDITOR, "1")
    } else {
        Vec::new()
    }
}

/// The tags a terminal adroit starts carries, saying what it runs.
pub fn terminal_tags(terminal: &TerminalSession) -> Vec<String> {
    let mut out = tags::tagged(KIND, "terminal");
    out.extend(tags::tagged(NAME, &terminal.name));
    out.extend(tags::tagged(CWD, &terminal.cwd.to_string_lossy()));
    out.extend(tags::tagged(
        "adroit.resource",
        &terminal.resource_id.to_string(),
    ));
    out.extend(tags::tagged(
        "adroit.program",
        match terminal.program {
            TerminalProgram::Shell => "shell",
            TerminalProgram::Neovim => "neovim",
            TerminalProgram::Command => "command",
        },
    ));
    if let Some(file) = &terminal.file {
        out.extend(tags::tagged(FILE, &file.to_string_lossy()));
    }
    if !terminal.command.is_empty() {
        out.extend(tags::tagged(
            COMMAND,
            &serde_json::to_string(&terminal.command).expect("serializable command"),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_session_launched_with_adroit_s_editor_is_known_to_hand_its_input_back() {
        use crate::execution::{StartRequest, resolve};
        let launched = |env: Vec<(String, String)>| {
            let launch = resolve(
                &crate::settings::Settings::default(),
                StartRequest {
                    cwd: "/repo".into(),
                    env,
                    ..Default::default()
                },
                std::path::Path::new("/adroit"),
            )
            .unwrap();
            let Some(Kept::Agent(session)) = read(&kept(42, launch.spawn.tags)) else {
                panic!("an agent session")
            };
            session
        };
        let script = crate::editor::current_script();
        let with_editor = crate::editor::session_env(Some(&script), |_| None);
        assert!(launched(with_editor).adroit_editor);
        assert!(!launched(Vec::new()).adroit_editor);
    }

    #[test]
    fn a_conversation_resumed_runs_its_agent_s_resume_and_keeps_its_id() {
        use crate::execution::{StartRequest, resolve};
        let id = "019f7a4b-fee9-74d3-9977-5bc9bb134d11";
        for (agent, resume) in [
            (adroit_model::Agent::Claude, ["--resume", id]),
            (adroit_model::Agent::Codex, ["resume", id]),
        ] {
            let launch = resolve(
                &crate::settings::Settings::default(),
                StartRequest {
                    agent,
                    cwd: "/repo".into(),
                    resume: Some(id.into()),
                    ..Default::default()
                },
                std::path::Path::new("/adroit"),
            )
            .unwrap();
            let args = &launch.spawn.args;
            assert!(args.windows(2).any(|w| w == resume), "{agent:?}: {args:?}");
            assert!(
                !args.iter().any(|a| a == "--session-id"),
                "{agent:?}: {args:?}"
            );
            assert_eq!(launch.conversation, id);
            let Some(Kept::Agent(session)) = read(&kept(42, launch.spawn.tags)) else {
                panic!("an agent session")
            };
            assert_eq!(session.session_id, id);
        }
    }

    #[test]
    fn a_session_whose_tags_do_not_say_how_it_was_launched_is_not_asked_for_its_input() {
        let tags = [tags::tagged(KIND, "claude"), tags::tagged(CWD, "/tmp")].concat();
        let Some(Kept::Agent(session)) = read(&kept(42, tags)) else {
            panic!("an agent session")
        };
        assert!(!session.adroit_editor);
    }

    #[test]
    fn user_input_times_are_kept_on_the_session_and_older_windows_cannot_replace_them() {
        let mut session = Session {
            id: hex(42),
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
    use adroit_model::Running;

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
    fn the_tags_a_session_is_started_with_keep_their_spelling() {
        let permissions = serde_json::to_string(&adroit_model::AgentPermissions::ReadOnly).unwrap();
        assert_eq!(
            claude_tags(
                "fix",
                std::path::Path::new("/api"),
                "uuid",
                &adroit_model::AgentPermissions::ReadOnly,
                42,
            ),
            [
                "adroit.kind=claude".to_string(),
                "adroit.name=fix".into(),
                "adroit.cwd=/api".into(),
                "adroit.started=42".into(),
                "adroit.claude=uuid".into(),
                format!("adroit.permissions={permissions}"),
            ]
        );
        assert_eq!(
            agent_tags(
                adroit_model::Agent::Codex,
                "fix",
                std::path::Path::new("/api"),
                "",
                &adroit_model::AgentPermissions::ReadOnly,
                42,
            ),
            [
                "adroit.name=fix".to_string(),
                "adroit.cwd=/api".into(),
                "adroit.started=42".into(),
                format!("adroit.permissions={permissions}"),
                "adroit.kind=codex".into(),
            ]
        );
        let resource = adroit_model::TerminalResourceId::default();
        assert_eq!(
            terminal_tags(&TerminalSession {
                resource_id: resource,
                name: "shell".into(),
                cwd: "/api".into(),
                program: TerminalProgram::Neovim,
                file: Some("/api/a.rs".into()),
                ..TerminalSession::default()
            }),
            [
                "adroit.kind=terminal".to_string(),
                "adroit.name=shell".into(),
                "adroit.cwd=/api".into(),
                format!("adroit.resource={resource}"),
                "adroit.program=neovim".into(),
                "adroit.file=/api/a.rs".into(),
            ]
        );
        assert_eq!(
            learned_tag((7, adroit_agents::Learned::Pid(42))),
            (7, "adroit.pid", "42".to_string())
        );
        assert_eq!(
            learned_tag((7, adroit_agents::Learned::Conversation("c".into()))),
            (7, "adroit.claude", "c".to_string())
        );
        assert_eq!(
            note_input(
                &mut Session {
                    id: hex(7),
                    ..Session::default()
                },
                5
            ),
            Some((7, "adroit.last-input", "5".to_string()))
        );
    }

    /// Claude Code's listing read against `own`, with what was learned as
    /// the tags that keep it.
    fn follow(own: &mut [Session], listed: Vec<Running>) -> Vec<(u128, &'static str, String)> {
        let learned = adroit_agents::adapter(adroit_model::Agent::Claude).follow(own, listed);
        learned.into_iter().map(learned_tag).collect()
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
        let learned = follow(&mut own, vec![listed("claude-uuid", 42)]);
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
            ),
        );
        let Some(Kept::Agent(c)) = read(&s) else {
            panic!("not a session");
        };
        assert_eq!(c.session_id, "uuid");
        assert!(!c.reads_only());
        assert_eq!(c.parent, None);
    }

    #[test]
    fn a_started_session_is_read_back_with_the_session_that_started_it() {
        let parent = adroit_model::SessionId::new();
        let delegation = crate::delegation::launch::Delegation::new(
            adroit_model::SessionId::new(),
            Some(parent),
            "/state".into(),
            "token".into(),
        );
        let mut tags = claude_tags(
            "fix",
            std::path::Path::new("/api"),
            "uuid",
            &adroit_model::AgentPermissions::Default,
            0,
        );
        tags.extend(delegation.tags());
        let Some(Kept::Agent(c)) = read(&kept(7, tags)) else {
            panic!("not a session");
        };
        assert_eq!(c.delegation, Some(delegation.id));
        assert_eq!(c.parent, Some(parent));
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
            ),
        );
        let Some(Kept::Agent(c)) = read(&s) else {
            panic!("not a session");
        };
        assert_eq!(c.state, State::Starting);
        assert!(!c.reads_only());
    }

    #[test]
    fn a_terminal_keeps_its_resource_id_and_program() {
        for (program, command) in [
            (TerminalProgram::Shell, vec![]),
            (TerminalProgram::Neovim, vec![]),
            (
                TerminalProgram::Command,
                vec!["claude".into(), "--model".into(), "it's opus".into()],
            ),
        ] {
            let terminal = TerminalSession {
                id: 9,
                resource_id: adroit_model::TerminalResourceId::new(),
                name: "tool".into(),
                cwd: "/api".into(),
                program,
                file: None,
                command,
            };
            let s = kept(9, terminal_tags(&terminal));
            assert_eq!(read(&s), Some(Kept::Terminal(terminal)));
        }
    }

    #[test]
    fn a_command_runs_through_the_shell_with_what_a_shell_terminal_gets() {
        use crate::execution::{TerminalStartRequest, resolve_terminal};
        let mut settings = crate::settings::Settings::default();
        settings.shell.env.insert("FROM_SHELL".into(), "1".into());
        let launch = resolve_terminal(
            &settings,
            TerminalStartRequest {
                name: "claude".into(),
                cwd: "/api".into(),
                program: TerminalProgram::Command,
                command: vec!["claude".into(), "--debug".into()],
                ..Default::default()
            },
            "/bin/zsh",
        );
        assert_eq!(launch.spawn.program, "/bin/zsh");
        assert_eq!(
            launch.spawn.args[launch.spawn.args.len() - 2..],
            ["claude".to_string(), "--debug".to_string()]
        );
        for var in [("SHELL", "/bin/zsh"), ("FROM_SHELL", "1")] {
            assert!(
                launch
                    .spawn
                    .env
                    .contains(&(var.0.to_string(), var.1.to_string())),
                "{var:?}"
            );
        }
        assert_eq!(launch.terminal.command, ["claude", "--debug"]);
    }

    #[test]
    fn neovim_started_on_a_file_is_handed_the_file_and_keeps_it() {
        use crate::execution::{TerminalStartRequest, resolve_terminal};
        let launch = resolve_terminal(
            &crate::settings::Settings::default(),
            TerminalStartRequest {
                name: "main.rs".into(),
                cwd: "/api".into(),
                program: TerminalProgram::Neovim,
                file: Some("/api/src/main.rs".into()),
                ..Default::default()
            },
            "/bin/sh",
        );
        assert_eq!(
            launch.spawn.args.last().map(String::as_str),
            Some("/api/src/main.rs")
        );
        let Some(Kept::Terminal(read)) = read(&kept(9, launch.spawn.tags)) else {
            panic!("a terminal")
        };
        assert_eq!(read.file, Some("/api/src/main.rs".into()));
        assert_eq!(
            read,
            TerminalSession {
                id: 9,
                ..launch.terminal
            }
        );
    }

    #[test]
    fn a_session_adroit_did_not_start_or_one_that_exited_is_not_adroit_s() {
        assert_eq!(read(&kept(1, vec!["other=1".into()])), None);
        let mut s = kept(
            9,
            terminal_tags(&TerminalSession {
                name: "shell 1".into(),
                cwd: "/api".into(),
                ..TerminalSession::default()
            }),
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
        );
        if let Some(pid) = pid {
            tags.extend(tags::tagged(PID, &pid.to_string()));
        }
        let Some(Kept::Agent(c)) = read(&kept(id, tags)) else {
            panic!("not a session");
        };
        c
    }

    fn listed(conversation: &str, pid: u32) -> Running {
        Running {
            session_id: conversation.into(),
            pid,
        }
    }

    #[test]
    fn a_session_learns_the_process_holding_its_conversation() {
        let mut ours = [own(7, "uuid", None)];

        let learned = follow(&mut ours, vec![listed("uuid", 42), listed("other", 9)]);

        assert_eq!(ours[0].pid, Some(42));
        assert_eq!(learned, vec![(7, PID, "42".to_string())]);
    }

    #[test]
    fn a_session_follows_its_process_to_the_conversation_it_moves_to() {
        let mut ours = [own(7, "uuid", Some(42))];

        let learned = follow(&mut ours, vec![listed("cleared", 42)]);

        assert_eq!(ours[0].session_id, "cleared");
        assert_eq!(learned, vec![(7, CONVERSATION, "cleared".to_string())]);
    }

    #[test]
    fn a_session_whose_conversation_and_process_are_known_learns_nothing() {
        let mut ours = [own(7, "uuid", Some(42))];

        let learned = follow(&mut ours, vec![listed("uuid", 42)]);

        assert_eq!(ours[0].session_id, "uuid");
        assert!(learned.is_empty());
    }

    #[test]
    fn the_process_a_session_runs_in_is_read_back_from_its_tags() {
        assert_eq!(own(7, "uuid", Some(42)).pid, Some(42));
        assert_eq!(own(7, "uuid", None).pid, None);
    }
}
