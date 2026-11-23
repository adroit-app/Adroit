//! `adroit session changes` and `profiles`: what a session's worktree changed,
//! and what sessions can be started from.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use adroit_model::Agent;
use adroit_model::usage::{Limit, Severity, Window};
use serde_json::{Value, json};

use super::{cut, find};

/// The most commits and files told, and how much of a subject or a path, in
/// bytes: together they fit what a caller reads of a command.
const COMMITS: usize = 30;
const FILES: usize = 200;
const TEXT: usize = 200;

/// How long an account's usage stands before it is asked again, and how long
/// asking may take.
const FRESH: Duration = Duration::from_secs(60);
const ASKING: Duration = Duration::from_secs(20);

fn short(text: &str) -> &str {
    cut(text, TEXT).0
}

fn changes_json(
    dir: &std::path::Path,
    worktree: Option<&adroit_vcs::Worktree>,
    changed: Option<&adroit_vcs::Changed>,
) -> Value {
    let mut out = json!({
        "dir": dir,
        "worktree": worktree.map(|w| &w.path),
        "repository": worktree.map(|w| &w.repo),
        "branch": worktree.and_then(|w| w.branch.as_ref()),
        "vcs": null, "base": null, "commits": [], "files": [],
        "added": 0, "removed": 0, "truncated": false,
    });
    let Some(changed) = changed else {
        return out;
    };
    let jj = changed.backend == adroit_vcs::Backend::Jj;
    out["vcs"] = json!(if jj { "jj" } else { "git" });
    out["base"] = json!(changed.base);
    out["commits"] = changed
        .commits
        .iter()
        .take(COMMITS)
        .map(|c| {
            json!({"id": c.id, "subject": short(&c.subject), "current": c.current,
                "empty": c.empty, "bookmarks": c.bookmarks})
        })
        .collect();
    out["files"] = changed
        .files
        .iter()
        .take(FILES)
        .map(|(change, counts)| {
            json!({"status": change.status.trim(), "path": short(&change.path),
                "from": change.from, "added": counts.map(|c| c.0),
                "removed": counts.map(|c| c.1)})
        })
        .collect();
    let counted = || changed.files.iter().filter_map(|(_, counts)| *counts);
    out["added"] = json!(counted().map(|c| c.0).sum::<u64>());
    out["removed"] = json!(counted().map(|c| c.1).sum::<u64>());
    out["truncated"] = json!(
        changed.files.len() > FILES
            || changed.commits.len() > COMMITS
            || changed.files.iter().any(|(c, _)| c.path.len() > TEXT)
            || changed.commits.iter().any(|c| c.subject.len() > TEXT)
    );
    out
}

fn changes_text(told: &Value) -> String {
    let files = told["files"].as_array().into_iter().flatten();
    let count = |n: &Value| n.as_u64().map_or("-".into(), |n| n.to_string());
    let mut out: Vec<String> = files
        .map(|f| {
            format!(
                "{}\t+{}\t-{}\t{}",
                f["status"].as_str().unwrap_or_default(),
                count(&f["added"]),
                count(&f["removed"]),
                f["path"].as_str().unwrap_or_default()
            )
        })
        .collect();
    out.push(format!("+{} -{}", told["added"], told["removed"]));
    out.join("\n")
}

/// What the worktree the session `asked` names works in changed.
pub(super) async fn changes(asked: &str, json: bool) -> Result<String, String> {
    let dir = find(asked).await?.session.cwd;
    if !dir.is_dir() {
        return Err(format!("{} is not there any more", dir.display()));
    }
    // One more than told, so that more is known to be left out.
    let changed = adroit_vcs::changed(&dir, COMMITS + 1)
        .await
        .map_err(|e| e.to_string())?;
    let worktree = adroit_vcs::inspect(&dir).await;
    let told = changes_json(&dir, worktree.as_ref(), changed.as_ref());
    Ok(match json {
        true => told.to_string(),
        false => changes_text(&told),
    })
}

type Account = (Agent, Option<PathBuf>);
type Asked = (Instant, Option<Vec<Limit>>);

static USAGE: LazyLock<Mutex<HashMap<Account, Asked>>> = LazyLock::new(Default::default);

/// How much of the account `context` is logged in to is used; none where the
/// agent tells none, or did not answer.
async fn usage(context: adroit_agents::AgentContext) -> Option<Vec<Limit>> {
    let adapter = adroit_agents::adapter(context.agent());
    if !adapter.capabilities().usage_limits {
        return None;
    }
    let account = (context.agent(), context.home.clone());
    let held = {
        let known = USAGE.lock().unwrap_or_else(|e| e.into_inner());
        known.get(&account).cloned()
    };
    if let Some((at, limits)) = held
        && at.elapsed() < FRESH
    {
        return limits;
    }
    let asked = adapter.usage(context, crate::paths::home());
    let limits = tokio::time::timeout(ASKING, asked).await.ok().flatten();
    let mut known = USAGE.lock().unwrap_or_else(|e| e.into_inner());
    known.insert(account, (Instant::now(), limits.clone()));
    limits
}

fn limit_json(limit: &Limit) -> Value {
    let (window, of) = match &limit.window {
        Window::Session => ("session", None),
        Window::Week => ("week", None),
        Window::WeekOf(of) => ("week", Some(of)),
        Window::Other(of) => ("other", Some(of)),
    };
    let severity = match limit.severity {
        Severity::Normal => "normal",
        Severity::Warning => "warning",
        Severity::Critical => "critical",
    };
    json!({"window": window, "of": of, "used_percent": limit.percent,
        "left_percent": (100.0 - limit.percent).max(0.0),
        "resets_ms": limit.resets_ms, "severity": severity})
}

/// The profiles sessions can start from, with the models each lists and how
/// much of its account is used.
pub(super) async fn profiles(json: bool) -> Result<String, String> {
    let settings = crate::settings::try_load()?;
    let contexts = crate::programs::profiles(&settings);
    let default = settings.profile_of(settings.default_agent()).id.clone();
    let listed = crate::delegation::service::models_of(&crate::delegation::app::root());
    let logins = crate::programs::logins(&settings);
    let mut asking = tokio::task::JoinSet::new();
    for login in logins {
        let account = (login.agent(), login.home.clone());
        asking.spawn(async move { (account, usage(login).await) });
    }
    let mut used: HashMap<Account, Option<Vec<Limit>>> = HashMap::new();
    while let Some(Ok((account, limits))) = asking.join_next().await {
        used.insert(account, limits);
    }
    let told: Vec<Value> = contexts
        .iter()
        .map(|context| {
            let models = listed
                .iter()
                .find(|m| m.profile == context.profile)
                .and_then(|m| m.models.as_ref())
                .map(|models| {
                    models
                        .iter()
                        .map(|m| json!({"id": m.id, "name": m.name}))
                        .collect::<Vec<_>>()
                });
            let usage = used
                .get(&(context.agent(), context.home.clone()))
                .cloned()
                .flatten()
                .map(|limits| limits.iter().map(limit_json).collect::<Vec<_>>());
            json!({"profile": context.profile, "name": context.profile_name,
                "agent": context.agent().key(), "home": context.home,
                "default": context.profile == default,
                "models": models, "usage": usage})
        })
        .collect();
    if json {
        return Ok(json!(told).to_string());
    }
    Ok(told.iter().map(profile_line).collect::<Vec<_>>().join("\n"))
}

fn profile_line(told: &Value) -> String {
    let limits = told["usage"].as_array().into_iter().flatten();
    let used: Vec<String> = limits
        .map(|l| {
            format!(
                "{} {}%",
                l["of"]
                    .as_str()
                    .or(l["window"].as_str())
                    .unwrap_or_default(),
                l["used_percent"]
            )
        })
        .collect();
    format!(
        "{}\t{}\t{}\t{}",
        told["profile"].as_str().unwrap_or_default(),
        told["agent"].as_str().unwrap_or_default(),
        told["name"].as_str().unwrap_or_default(),
        used.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_vcs::{Backend, Change, Changed};

    fn changed(files: usize) -> Changed {
        Changed {
            backend: Backend::Jj,
            base: "abc".into(),
            commits: Vec::new(),
            files: (0..files)
                .map(|n| {
                    let change = Change {
                        status: " M".into(),
                        path: format!("f{n}"),
                        from: None,
                    };
                    (change, (n != 1).then_some((2, 1)))
                })
                .collect(),
        }
    }

    #[test]
    fn what_changed_is_told_file_by_file_and_summed() {
        let told = changes_json(std::path::Path::new("/r"), None, Some(&changed(3)));
        assert_eq!(told["vcs"], "jj");
        assert_eq!(told["files"][0]["status"], "M");
        assert_eq!(told["files"][1]["added"], Value::Null);
        assert_eq!((&told["added"], &told["removed"]), (&json!(4), &json!(2)));
        assert_eq!(told["truncated"], false);
        assert!(changes_text(&told).ends_with("M\t+2\t-1\tf2\n+4 -2"));
    }

    #[test]
    fn many_files_are_cut_and_still_summed() {
        let told = changes_json(std::path::Path::new("/r"), None, Some(&changed(FILES + 2)));
        assert_eq!(told["files"].as_array().unwrap().len(), FILES);
        assert_eq!(told["truncated"], true);
        assert_eq!(told["added"], json!((FILES + 1) * 2));
    }

    #[test]
    fn a_directory_outside_version_control_changed_nothing_known() {
        let told = changes_json(std::path::Path::new("/r"), None, None);
        assert_eq!(told["vcs"], Value::Null);
        assert_eq!(told["files"], json!([]));
    }

    #[test]
    fn a_limit_tells_its_window_and_what_is_left() {
        let limit = |window, percent| Limit {
            window,
            percent,
            resets_ms: Some(5),
            severity: Severity::Warning,
        };
        assert_eq!(
            limit_json(&limit(Window::WeekOf("Opus".into()), 30.0)),
            json!({"window": "week", "of": "Opus", "used_percent": 30.0,
                "left_percent": 70.0, "resets_ms": 5, "severity": "warning"})
        );
        assert_eq!(
            limit_json(&limit(Window::Session, 104.0))["left_percent"],
            0.0
        );
    }
}
