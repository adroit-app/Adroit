//! The directories a new session may be started in that Claude Code has been
//! used in, and where it keeps their conversations.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::AgentContext;

/// The directory Claude Code keeps its state in as `context` starts it: the
/// connection's, else `CLAUDE_CONFIG_DIR` as the launch or adroit's own
/// environment sets it; none for the home directory's.
pub(crate) fn config_dir(context: &AgentContext) -> Option<PathBuf> {
    context
        .home
        .clone()
        .or_else(|| {
            context
                .launch
                .env
                .get("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
        })
        .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from))
}

/// Where Claude Code keeps its state: in `config`, else in the home
/// directory, which holds `.claude.json` and `.claude/` side by side.
fn claude_home(config: Option<&Path>, home: &Path) -> (PathBuf, PathBuf) {
    match config {
        Some(dir) => (dir.join(".claude.json"), dir.join("projects")),
        None => (
            home.join(".claude.json"),
            home.join(".claude").join("projects"),
        ),
    }
}

/// Where Claude Code, started as `context` says, keeps the conversations of
/// every directory.
pub(crate) fn projects(context: &AgentContext, home: &Path) -> PathBuf {
    claude_home(config_dir(context).as_deref(), home).1
}

/// The directories Claude Code's own state names as used.
pub fn parse_known(json: &str) -> Vec<PathBuf> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    value
        .get("projects")
        .and_then(serde_json::Value::as_object)
        .map(|o| o.keys().map(PathBuf::from).collect())
        .unwrap_or_default()
}

/// The folder Claude Code keeps a directory's conversations in: the path with
/// every character but ASCII letters, digits and `-` turned into `-`.
pub fn transcripts_name(dir: &Path) -> String {
    dir.to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Where Claude Code, started as `context` says, writes the conversation
/// `session_id` of a session started in `cwd`.
pub fn transcript(context: &AgentContext, home: &Path, cwd: &Path, session_id: &str) -> PathBuf {
    transcript_in(&projects(context, home), cwd, session_id)
}

fn transcript_in(projects: &Path, cwd: &Path, session_id: &str) -> PathBuf {
    projects
        .join(transcripts_name(cwd))
        .join(format!("{session_id}.jsonl"))
}

/// `dirs` with the most recently used first, by when each was last talked in;
/// those never talked in keep their order after the rest.
pub fn by_recency(
    dirs: Vec<PathBuf>,
    last_used: impl Fn(&Path) -> Option<SystemTime>,
) -> Vec<PathBuf> {
    let mut dated: Vec<(Option<SystemTime>, usize, PathBuf)> = dirs
        .into_iter()
        .enumerate()
        .map(|(i, d)| (last_used(&d), i, d))
        .collect();
    dated.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    dated.into_iter().map(|(_, _, d)| d).collect()
}

/// The directories Claude Code, started as `context` says, has been used in
/// that still exist, the most recently used first.
pub fn recent(context: &AgentContext, home: &Path) -> Vec<PathBuf> {
    let (state, transcripts) = claude_home(config_dir(context).as_deref(), home);
    let known = std::fs::read_to_string(state)
        .map(|t| parse_known(&t))
        .unwrap_or_default();
    let alive = known.into_iter().filter(|d| d.is_dir()).collect();
    by_recency(alive, |d| {
        last_written(&transcripts.join(transcripts_name(d)))
    })
}

/// When anything in `dir` was last written. The folder's own time is not
/// enough: it moves only when a conversation is added, not when one goes on.
fn last_written(dir: &Path) -> Option<SystemTime> {
    let own = std::fs::metadata(dir).and_then(|m| m.modified()).ok()?;
    let entries = std::fs::read_dir(dir).into_iter().flatten().flatten();
    let newest = entries.filter_map(|e| e.metadata().and_then(|m| m.modified()).ok());
    newest.chain([own]).max()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use adroit_model::Agent;

    use super::*;

    #[test]
    fn a_connection_s_conversations_are_read_in_its_own_directory_whatever_the_launch_sets() {
        let home = Path::new("/h");
        let mut context = AgentContext::builtin(Agent::Claude);
        context
            .launch
            .env
            .insert("CLAUDE_CONFIG_DIR".into(), "/launch".into());
        assert_eq!(
            transcript(&context, home, Path::new("/src"), "id"),
            PathBuf::from("/launch/projects/-src/id.jsonl")
        );
        context.home = Some("/work".into());
        assert_eq!(
            transcript(&context, home, Path::new("/src"), "id"),
            PathBuf::from("/work/projects/-src/id.jsonl")
        );
        assert_eq!(
            crate::claude::history::history_path(&context, home),
            PathBuf::from("/work/history.jsonl")
        );
    }

    #[test]
    fn the_directories_used_are_the_keys_of_claude_code_s_projects() {
        let json = r#"{"numStartups":3,"projects":{"/src/api":{"allowedTools":[]},"/notes":{}}}"#;
        let mut got = parse_known(json);
        got.sort();
        assert_eq!(
            got,
            vec![PathBuf::from("/notes"), PathBuf::from("/src/api")]
        );
    }

    #[test]
    fn unreadable_state_names_no_directory() {
        assert!(parse_known("not json").is_empty());
        assert!(parse_known(r#"{"projects":[]}"#).is_empty());
        assert!(parse_known("{}").is_empty());
    }

    #[test]
    fn a_directory_s_conversations_are_under_its_path_with_punctuation_dashed() {
        assert_eq!(
            transcripts_name(Path::new(
                "/Users/me/ghq/github.com/a-b/C/.claude/worktrees/x_y"
            )),
            "-Users-me-ghq-github-com-a-b-C--claude-worktrees-x-y"
        );
    }

    #[test]
    fn a_conversation_is_written_under_its_directory_s_folder_named_by_its_id() {
        assert_eq!(
            transcript_in(
                Path::new("/h/.claude/projects"),
                Path::new("/src/a_b"),
                "uuid"
            ),
            PathBuf::from("/h/.claude/projects/-src-a-b/uuid.jsonl")
        );
    }

    #[test]
    fn the_most_recently_used_come_first_and_the_unused_keep_their_order() {
        let at = |s| Some(SystemTime::UNIX_EPOCH + Duration::from_secs(s));
        let got = by_recency(
            ["/a", "/b", "/c", "/d"].map(PathBuf::from).to_vec(),
            |d| match d.to_str() {
                Some("/b") => at(1),
                Some("/d") => at(2),
                _ => None,
            },
        );
        assert_eq!(got, ["/d", "/b", "/a", "/c"].map(PathBuf::from).to_vec());
    }

    #[test]
    fn a_conversation_going_on_counts_as_use_of_its_directory() {
        let dir = std::env::temp_dir().join(format!("adroit-places-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let talk = dir.join("a.jsonl");
        std::fs::write(&talk, "").unwrap();
        let later = SystemTime::now() + Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(&talk)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert_eq!(last_written(&dir), Some(later));
        assert_eq!(last_written(&dir.join("missing")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
