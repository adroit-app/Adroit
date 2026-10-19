//! The sessions adroit runs in its daemon, as the window knows them: what
//! each one's tags say it is.

use std::path::PathBuf;

use adroit_model::{Kind, Session, State};
use adroit_term::tags;

use crate::shells::ShellInfo;

const KIND: &str = "adroit.kind";
pub const NAME: &str = "adroit.name";
/// The directory a session was started in, which the daemon does not list
/// (felis-terminal/felis#7).
const CWD: &str = "adroit.cwd";
const STARTED: &str = "adroit.started";
const READER: &str = "adroit.reader";
/// The conversation a Claude Code session holds, told by its hooks.
pub const CONVERSATION: &str = "adroit.claude";

/// A session the daemon holds, as what it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kept {
    Claude(Session),
    Shell(ShellInfo),
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
        "claude" => Some(Kept::Claude(Session {
            id: hex(session.id),
            session_id: tags::value(t, CONVERSATION).unwrap_or_default(),
            pid: None,
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
            reader: tags::value(t, READER).is_some(),
        })),
        "shell" => Some(Kept::Shell(ShellInfo {
            id: session.id,
            name,
            worktree: cwd,
        })),
        _ => None,
    }
}

/// The tags a Claude Code session adroit starts carries.
pub fn claude_tags(
    name: &str,
    cwd: &std::path::Path,
    reader: bool,
    started_ms: u64,
) -> Vec<String> {
    let mut out = tags::tagged(KIND, "claude");
    out.extend(tags::tagged(NAME, name));
    out.extend(tags::tagged(CWD, &cwd.to_string_lossy()));
    out.extend(tags::tagged(STARTED, &started_ms.to_string()));
    if reader {
        out.extend(tags::tagged(READER, "1"));
    }
    out
}

/// The tags of a session resuming the conversation `conversation`: it goes
/// by it from the start, so the tab of the session it takes over follows it
/// before it reports the conversation itself.
pub fn resumed_tags(
    name: &str,
    cwd: &std::path::Path,
    conversation: &str,
    started_ms: u64,
) -> Vec<String> {
    let mut out = claude_tags(name, cwd, false, started_ms);
    out.extend(tags::tagged(CONVERSATION, conversation));
    out
}

/// The tags a shell adroit starts carries.
pub fn shell_tags(name: &str, worktree: &std::path::Path) -> Vec<String> {
    let mut out = tags::tagged(KIND, "shell");
    out.extend(tags::tagged(NAME, name));
    out.extend(tags::tagged(CWD, &worktree.to_string_lossy()));
    out
}

#[cfg(test)]
mod tests {
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
    fn a_claude_session_is_read_back_from_its_tags_and_status() {
        let mut s = kept(
            7,
            claude_tags("fix", std::path::Path::new("/api"), true, 42),
        );
        s.tags.extend(tags::tagged(CONVERSATION, "uuid"));
        s.status = Some(Status {
            state: "blocked".into(),
            kind: Some("permission".into()),
            msg: None,
        });
        let Some(Kept::Claude(c)) = read(&s) else {
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
    fn a_resumed_session_goes_by_its_conversation_before_it_reports_one() {
        let s = kept(
            7,
            resumed_tags("fix", std::path::Path::new("/api"), "uuid", 0),
        );
        let Some(Kept::Claude(c)) = read(&s) else {
            panic!("not a session");
        };
        assert_eq!(c.session_id, "uuid");
        assert!(!c.reads_only());
    }

    #[test]
    fn a_session_that_reported_nothing_is_starting() {
        let s = kept(
            7,
            claude_tags("fix", std::path::Path::new("/api"), false, 0),
        );
        let Some(Kept::Claude(c)) = read(&s) else {
            panic!("not a session");
        };
        assert_eq!(c.state, State::Starting);
        assert!(!c.reads_only());
    }

    #[test]
    fn a_shell_is_read_back_with_its_worktree() {
        let s = kept(9, shell_tags("shell 1", std::path::Path::new("/api")));
        assert_eq!(
            read(&s),
            Some(Kept::Shell(ShellInfo {
                id: 9,
                name: "shell 1".into(),
                worktree: "/api".into(),
            }))
        );
    }

    #[test]
    fn a_session_adroit_did_not_start_or_one_that_exited_is_not_adroit_s() {
        assert_eq!(read(&kept(1, vec!["other=1".into()])), None);
        let mut s = kept(9, shell_tags("shell 1", std::path::Path::new("/api")));
        s.exited = true;
        assert_eq!(read(&s), None);
    }

    #[test]
    fn only_a_daemon_id_reads_as_one() {
        assert_eq!(id_of("not an id"), None);
        assert_eq!(id_of(&hex(u128::MAX)), Some(u128::MAX));
    }
}
