//! `adroit session start`: start a session in the daemon from a command, as
//! a session hands work to another.

use std::io::Read;
use std::path::PathBuf;

use adroit_model::{AgentPermissions, CustomPermissions};
use adroit_term::Daemon;

use crate::scratch::{self, Moment};
use crate::sessions::{self, Kept};
use crate::settings::Settings;
use crate::tabs::{fresh_name, label_from, label_of};

const USAGE: &str = "usage: adroit session start [--dir <path>] [--name <name>] [--permissions <default|read_only> | --permissions-file <path>] [--agent <claude|codex>] [--model <model>] [-- <prompt>]";

/// Where Claude Code tells a command it runs which conversation it is.
const CONVERSATION_ENV: &str = "CLAUDE_CODE_SESSION_ID";

/// The size a session starts at, until a window shows it at its own.
const COLS: u16 = 120;
const ROWS: u16 = 40;

/// What `adroit session start` was asked for.
#[derive(Debug, Default, PartialEq, Eq)]
struct Asked {
    dir: Option<PathBuf>,
    name: Option<String>,
    permissions: AgentPermissions,
    permissions_file: Option<PathBuf>,
    agent: Option<adroit_model::Agent>,
    model: Option<String>,
    /// `None` when the prompt is to be read from standard input.
    prompt: Option<String>,
}

fn parse(args: &[String]) -> Option<Asked> {
    let mut asked = Asked::default();
    let mut rest = args.iter();
    let mut permissions_given = false;
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--dir" => asked.dir = Some(rest.next()?.into()),
            "--name" => asked.name = Some(rest.next()?.clone()),
            "--permissions" if !permissions_given => {
                asked.permissions = match rest.next()?.as_str() {
                    "default" => AgentPermissions::Default,
                    "read_only" => AgentPermissions::ReadOnly,
                    _ => return None,
                };
                permissions_given = true;
            }
            "--permissions-file" if !permissions_given => {
                asked.permissions_file = Some(rest.next()?.into());
                permissions_given = true;
            }
            "--agent" => asked.agent = Some(adroit_model::Agent::parse(rest.next()?)?),
            "--model" => {
                let model = rest.next()?.trim();
                if model.is_empty() {
                    return None;
                }
                asked.model = Some(model.into());
            }
            "--" => {
                let words: Vec<&str> = rest.by_ref().map(String::as_str).collect();
                asked.prompt = Some(words.join(" "));
            }
            _ => return None,
        }
    }
    Some(asked)
}

/// The name the session goes by: the one given, or one made from its prompt,
/// numbered when taken.
fn name_for(given: Option<&str>, prompt: &str, taken: &[&str]) -> String {
    let label = given
        .and_then(label_of)
        .unwrap_or_else(|| label_from(prompt));
    fresh_name(&label, taken)
}

/// The conversation the command was run from, as Claude Code tells it.
fn started_from(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

async fn start(
    daemon: &Daemon,
    asked: &Asked,
    dir: PathBuf,
    prompt: &str,
    from: Option<&str>,
) -> Result<String, String> {
    let listed = daemon
        .sessions()
        .await
        .map_err(|e| format!("cannot reach the daemon: {e}"))?;
    let names: Vec<String> = listed
        .iter()
        .filter_map(|s| match sessions::read(s)? {
            Kept::Agent(c) => Some(c.name),
            Kept::Terminal(_) => None,
        })
        .collect();
    let taken: Vec<&str> = names.iter().map(String::as_str).collect();
    let name = name_for(asked.name.as_deref(), prompt, &taken);
    let settings = crate::settings::current();
    let request = crate::execution::StartRequest {
        agent: asked.agent.unwrap_or(settings.agent),
        model: asked.model.clone(),
        name: name.clone(),
        cwd: dir,
        permissions: asked.permissions.clone(),
        prompt: prompt.into(),
        env: crate::daemon::terminal_env(),
        started_ms: now_ms(),
        started_from: from.map(String::from),
        ..Default::default()
    };
    let program = std::env::current_exe().unwrap_or_else(|_| "adroit".into());
    let prepared =
        crate::control::managed::prepare(&crate::control::root(), &settings, request, &program)
            .await?;
    let mut spawn = prepared.resolved.spawn;
    spawn.cols = COLS;
    spawn.rows = ROWS;
    let palette = crate::themes::palette(&settings.theme, &Settings::themes_dir())?;
    let failure_owner = prepared.ownership.clone();
    let started =
        crate::execution::spawn_headless_prepared(daemon, spawn, &palette, move |id| async move {
            match prepared.ownership {
                Some(ownership) => ownership.attach(id).await,
                None => Ok(()),
            }
        })
        .await;
    if let Err(error) = &started
        && let Some(owner) = failure_owner
    {
        owner.failed(error).await;
    }
    started.map_err(|e| format!("cannot start {name}: {e}"))?;
    Ok(name)
}

/// `adroit session <args>`. Returns the process exit code.
pub fn cli(args: &[String]) -> i32 {
    if std::env::var_os("ADROIT_CONTROL_TOKEN").is_some() {
        eprintln!(
            "adroit: managed agents delegate using put_task and queue_task; session start creates a human-owned session"
        );
        return 2;
    }

    let Some(mut asked) = (match args {
        [op, rest @ ..] if op == "start" => parse(rest),
        _ => None,
    }) else {
        eprintln!("{USAGE}");
        return 2;
    };
    if let Some(path) = &asked.permissions_file {
        let permissions = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|text| {
                serde_json::from_str::<CustomPermissions>(&text).map_err(|e| e.to_string())
            });
        match permissions {
            Ok(options) => asked.permissions = AgentPermissions::Custom(Box::new(options)),
            Err(e) => {
                eprintln!("adroit: cannot read custom permissions: {e}");
                return 2;
            }
        }
    }
    let prompt = match &asked.prompt {
        Some(p) => p.clone(),
        None => {
            let mut p = String::new();
            if let Err(e) = std::io::stdin().read_to_string(&mut p) {
                eprintln!("adroit: cannot read the prompt: {e}");
                return 1;
            }
            p
        }
    };
    let prompt = prompt.trim();
    if prompt.is_empty() {
        eprintln!("adroit: the prompt is empty");
        return 2;
    }
    let from = started_from(std::env::var(CONVERSATION_ENV).ok());
    // The daemon hands the new session this process's environment, which
    // holds the marks of the session it was run from.
    let names = std::env::vars_os().map(|(k, _)| k.to_string_lossy().into_owned());
    for k in crate::env::inherited(names) {
        // SAFETY: no other thread exists yet.
        unsafe { std::env::remove_var(k) };
    }
    let dir = match asked
        .dir
        .clone()
        .map_or_else(std::env::current_dir, Ok)
        .and_then(std::fs::canonicalize)
        .and_then(|d| scratch::start_dir(&d, &crate::paths::scratch_dir(), Moment::now()))
    {
        Ok(d) => d,
        Err(e) => {
            eprintln!("adroit: {e}");
            return 1;
        }
    };
    let started = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())
        .and_then(|rt| {
            rt.block_on(start(
                &crate::daemon::daemon(),
                &asked,
                dir,
                prompt,
                from.as_deref(),
            ))
        });
    match started {
        Ok(name) => {
            println!("{name}");
            0
        }
        Err(e) => {
            eprintln!("adroit: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn the_prompt_follows_the_options_after_a_double_dash() {
        assert_eq!(
            parse(&args(&[
                "--dir",
                "/src/api",
                "--name",
                "flaky",
                "--permissions",
                "read_only",
                "--",
                "find why",
                "--it fails"
            ])),
            Some(Asked {
                dir: Some("/src/api".into()),
                name: Some("flaky".into()),
                permissions: AgentPermissions::ReadOnly,
                permissions_file: None,
                agent: None,
                model: None,
                prompt: Some("find why --it fails".into()),
            })
        );
    }

    #[test]
    fn the_command_can_choose_an_agent_and_rejects_an_unknown_one() {
        assert_eq!(
            parse(&args(&["--agent", "codex", "--", "hello"]))
                .unwrap()
                .agent,
            Some(adroit_model::Agent::Codex)
        );
        assert_eq!(
            parse(&args(&["--agent", "claude"])).unwrap().agent,
            Some(adroit_model::Agent::Claude)
        );
        assert_eq!(parse(&args(&["--agent", "other"])), None);
        assert_eq!(parse(&args(&["--agent"])), None);
        assert!(
            parse(&args(&["--", "--agent codex"]))
                .unwrap()
                .agent
                .is_none()
        );
    }

    #[test]
    fn without_a_double_dash_the_prompt_is_left_to_standard_input() {
        assert_eq!(parse(&args(&[])), Some(Asked::default()));
    }

    #[test]
    fn permissions_are_explicit_and_the_removed_reader_flag_is_not_an_alias() {
        assert_eq!(parse(&args(&["--read-only"])), None);
        assert_eq!(parse(&args(&["--permissions", "unknown"])), None);
        assert_eq!(
            parse(&args(&[
                "--permissions",
                "read_only",
                "--permissions-file",
                "/profile.json"
            ])),
            None
        );
        let asked = parse(&args(&[
            "--permissions-file",
            "/profile.json",
            "--",
            "review",
        ]))
        .unwrap();
        assert_eq!(asked.permissions_file, Some("/profile.json".into()));
    }

    #[test]
    fn a_model_is_specific_to_the_requested_launch_and_not_part_of_the_prompt() {
        let asked = parse(&args(&[
            "--agent", "codex", "--model", "my model", "--", "review",
        ]))
        .unwrap();
        assert_eq!(asked.model.as_deref(), Some("my model"));
        assert_eq!(asked.prompt.as_deref(), Some("review"));
        assert_eq!(parse(&args(&["--model"])), None);
        assert_eq!(parse(&args(&["--model", " "])), None);
        assert!(
            parse(&args(&["--", "--model", "haiku"]))
                .unwrap()
                .model
                .is_none()
        );
    }

    #[test]
    fn an_unknown_option_or_one_missing_its_value_is_refused() {
        assert_eq!(parse(&args(&["--unknown", "x"])), None);
        assert_eq!(parse(&args(&["find why"])), None);
        assert_eq!(parse(&args(&["--dir"])), None);
    }

    #[test]
    fn a_session_is_named_from_its_prompt_unless_given_one() {
        assert_eq!(
            name_for(None, "Find why the login test is flaky", &[]),
            "find-login-test"
        );
        assert_eq!(name_for(Some("Review API"), "anything", &[]), "review-api");
    }

    #[test]
    fn a_name_already_taken_gets_a_number() {
        assert_eq!(
            name_for(Some("review"), "anything", &["review"]),
            "review-2"
        );
    }

    #[test]
    fn a_session_is_started_from_the_conversation_claude_code_names() {
        assert_eq!(started_from(Some("a08b".into())).as_deref(), Some("a08b"));
        assert_eq!(started_from(Some(String::new())), None);
        assert_eq!(started_from(None), None);
    }
}
