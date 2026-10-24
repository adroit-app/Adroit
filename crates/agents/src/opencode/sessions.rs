//! The conversations OpenCode keeps, as its server lists them, and the ids
//! adroit chooses for new ones.

use std::path::Path;

use adroit_model::Past;
use serde_json::Value;

use crate::AgentContext;

/// A new conversation's id in the form OpenCode makes its own: `ses_`, the
/// time it was made counting down, so that newer ones sort first, and
/// random letters. OpenCode makes the conversation with this id once its
/// first prompt is sent.
pub(super) fn new_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64);
    id_at(now, uuid::Uuid::new_v4().as_bytes())
}

/// The id [`new_id`] makes at `now`, in milliseconds since the epoch, from
/// the random bytes `random`.
fn id_at(now: u64, random: &[u8; 16]) -> String {
    const LETTERS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let counting_down = !(now.wrapping_mul(0x1000) + 1) & 0xffff_ffff_ffff;
    let letters: String = random[..14]
        .iter()
        .map(|byte| char::from(LETTERS[usize::from(*byte) % LETTERS.len()]))
        .collect();
    format!("ses_{counting_down:012x}{letters}")
}

/// The conversations `context`'s login keeps that ran in `dir`, or in any
/// directory when none is given, asked of OpenCode in `home`. A
/// conversation a subagent ran under another is left to that one.
pub(super) fn past(context: &AgentContext, home: &Path, dir: Option<&Path>) -> Vec<Past> {
    let args = [
        "api",
        "session.list",
        super::STANDALONE,
        "--param",
        "limit=200",
    ]
    .map(String::from)
    .to_vec();
    let start = crate::launch_in(context, args, Vec::new());
    match super::cli::ask_blocking(start, home, super::cli::ASKED_FOR) {
        Some((Some(0), out)) => listed(context, &out, dir),
        _ => Vec::new(),
    }
}

/// The conversations OpenCode listed in `out` that ran in `dir`, when one is
/// given. OpenCode names a directory by its real path, through every link.
fn listed(context: &AgentContext, out: &str, dir: Option<&Path>) -> Vec<Past> {
    let Ok(listed) = serde_json::from_str::<Value>(out) else {
        return Vec::new();
    };
    let dir = dir.map(|dir| dir.canonicalize().unwrap_or_else(|_| dir.into()));
    listed["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|session| session.get("parentID").is_none_or(Value::is_null))
        .filter_map(|session| {
            let cwd = Path::new(session["location"]["directory"].as_str()?);
            if dir.as_ref().is_some_and(|dir| dir != cwd) {
                return None;
            }
            Some(Past {
                agent: context.agent(),
                home: context.home.clone(),
                profile: context.profile.clone(),
                profile_name: context.profile_name.clone(),
                id: session["id"].as_str()?.into(),
                cwd: cwd.into(),
                title: adroit_model::title_of(session["title"].as_str().unwrap_or_default()),
                at_ms: session["time"]["updated"].as_u64().unwrap_or_default(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_model::Agent;

    #[test]
    fn a_new_id_is_one_opencode_makes_newer_ones_first() {
        // The time of one OpenCode made, ses_e914684c5ffedderBCC47g0J2L.
        let earlier = id_at(1_792_859_011_898, &[0; 16]);
        let later = id_at(1_792_859_011_899, &[61; 16]);
        assert_eq!(earlier.len(), "ses_".len() + 26);
        assert!(earlier.starts_with("ses_e914684c5ffe"), "{earlier}");
        assert!(earlier.ends_with("00000000000000"), "{earlier}");
        assert!(later.ends_with("zzzzzzzzzzzzzz"), "{later}");
        assert!(later < earlier);
        assert_ne!(new_id(), new_id());
    }

    #[test]
    fn past_conversations_are_those_of_the_directory_asked_about_by_their_title() {
        let context = AgentContext::builtin(Agent::OpenCode);
        let out = r#"{"data":[
            {"id":"ses_b","title":"Fix the\nlogin","time":{"created":1,"updated":20},"location":{"directory":"/w"}},
            {"id":"ses_c","parentID":"ses_b","title":"explore","time":{"updated":30},"location":{"directory":"/w"}},
            {"id":"ses_d","time":{"updated":10},"location":{"directory":"/w"}},
            {"id":"ses_e","title":"elsewhere","time":{"updated":40},"location":{"directory":"/x"}},
            {"title":"no id","location":{"directory":"/w"}}
        ]}"#;
        let seen = |dir: Option<&Path>| -> Vec<(String, String, u64)> {
            listed(&context, out, dir)
                .into_iter()
                .map(|p| (p.id, p.title, p.at_ms))
                .collect()
        };
        assert_eq!(
            seen(Some(Path::new("/w"))),
            [
                ("ses_b".into(), "Fix the login".into(), 20),
                ("ses_d".into(), String::new(), 10)
            ]
        );
        assert_eq!(seen(None).len(), 3);
        assert!(listed(&context, "not json", None).is_empty());
        // Asked through a link, the directory is the one it leads to.
        let real = tempfile::tempdir().unwrap();
        let linked = tempfile::tempdir().unwrap();
        let link = linked.path().join("link");
        std::os::unix::fs::symlink(real.path(), &link).unwrap();
        let there = format!(
            r#"{{"data":[{{"id":"ses_r","time":{{"updated":1}},"location":{{"directory":{}}}}}]}}"#,
            serde_json::to_string(&real.path().canonicalize().unwrap()).unwrap()
        );
        assert_eq!(listed(&context, &there, Some(&link)).len(), 1);
        let past = listed(&context, out, Some(Path::new("/x")));
        assert_eq!(
            (past[0].agent, past[0].profile.as_str()),
            (Agent::OpenCode, "opencode")
        );
    }

    #[test]
    fn the_conversations_are_asked_of_opencode_s_own_server_in_the_login() {
        let (_dir, opencode) = crate::testing::program(
            "opencode",
            r#"printf '%s ' "$@" > "$0.args"
echo '{"data":[{"id":"ses_a","title":"t","time":{"updated":5},"location":{"directory":"/w"}}]}'"#,
        );
        let mut context = AgentContext::builtin(Agent::OpenCode);
        context.launch.command = vec![opencode.display().to_string()];
        let past = past(&context, Path::new("/"), Some(Path::new("/w")));
        assert_eq!(past.len(), 1);
        let args = std::fs::read_to_string(opencode.with_extension("args")).unwrap();
        assert_eq!(args, "api session.list --standalone --param limit=200 ");
    }
}
