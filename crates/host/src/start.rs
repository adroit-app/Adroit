//! `adroit session start`: start a session in the daemon from a command,
//! the user's own or that of a session handing work on.

use std::io::Read;
use std::path::PathBuf;

use adroit_model::time::now_ms;
use adroit_model::{AgentPermissions, CustomPermissions};
use adroit_term::Daemon;

use crate::scratch;
use crate::starting;

const USAGE: &str = "usage: adroit session start [--dir <path>] [--worktree [--base <rev>]] [--name <name>] [--background] [--permissions <default|read_only> | --permissions-file <path>] [--agent <claude|codex|pi|opencode>] [--profile <id>] [--model <model>] [-- <prompt>]";

/// What `adroit session start` was asked for.
#[derive(Debug, Default, PartialEq, Eq)]
struct Asked {
    dir: Option<PathBuf>,
    worktree: bool,
    base: Option<String>,
    name: Option<String>,
    permissions: AgentPermissions,
    permissions_file: Option<PathBuf>,
    agent: Option<adroit_model::Agent>,
    profile: Option<adroit_model::profile::ProfileId>,
    model: Option<String>,
    /// Only listed, never opened in a window.
    background: bool,
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
            "--worktree" => asked.worktree = true,
            "--base" => asked.base = Some(rest.next()?.clone()),
            "--name" => asked.name = Some(rest.next()?.clone()),
            "--background" => asked.background = true,
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
            "--profile" => asked.profile = Some(rest.next()?.parse().ok()?),
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
    (asked.worktree || asked.base.is_none()).then_some(asked)
}

/// Starts the session `asked` for as the user's own, beside the sessions
/// `listed`.
async fn start(
    daemon: &Daemon,
    settings: &crate::settings::Settings,
    asked: &Asked,
    dir: PathBuf,
    prompt: &str,
    listed: &[adroit_term::Session],
) -> Result<String, String> {
    // Refused before a worktree is made for it.
    let chosen = crate::programs::starting(settings, asked.agent, asked.profile.as_ref())?;
    let request = crate::execution::StartRequest {
        agent: chosen.agent(),
        profile: Some(chosen.profile),
        model: asked.model.clone(),
        name: starting::name_for(asked.name.as_deref(), prompt, listed),
        name_given: asked.name.is_some(),
        permissions: asked.permissions.clone(),
        prompt: prompt.into(),
        env: crate::daemon::agent_env(),
        started_ms: now_ms(),
        show: !asked.background,
        ..Default::default()
    };
    let place = starting::Place {
        dir,
        worktree: asked.worktree,
        base: asked.base.clone(),
    };
    let host = starting::Host {
        daemon,
        root: &crate::delegation::app::root(),
        program: &crate::work_host::adroit_program(),
    };
    starting::start(host, settings, None, place, request)
        .await
        .map(|started| started.name)
        .map_err(|failure| failure.message)
}

/// Asks the work host to start the session `asked` for as the session that
/// `credential` names, which it is then started by.
async fn start_as(
    root: &std::path::Path,
    credential: &str,
    asked: &Asked,
    dir: PathBuf,
    prompt: &str,
) -> Result<String, String> {
    let command =
        crate::delegation::protocol::Command::StartSession(crate::delegation::protocol::Start {
            prompt: prompt.into(),
            dir: Some(dir),
            worktree: asked.worktree,
            base: asked.base.clone(),
            agent: asked.agent,
            profile: asked.profile.clone(),
            model: asked.model.clone(),
            permissions: Some(asked.permissions.clone()),
            name: asked.name.clone(),
            background: asked.background,
        });
    let started = crate::delegation::protocol::ask(root, credential, command, None)
        .await
        .map_err(|error| error.to_string())?;
    Ok(started["name"].as_str().unwrap_or_default().to_owned())
}

/// `adroit session explain <rest>`: a screen kept in a file read here, since
/// the file is where this command runs, and a session asked of its work host.
fn explain_cli(args: &[String], rest: &[String]) -> i32 {
    if crate::explain::parse(rest).is_none() {
        eprintln!("{}", crate::explain::USAGE);
        return 2;
    }
    match crate::explain::of_file(rest) {
        Some((Ok(text), code)) => {
            println!("{text}");
            code
        }
        Some((Err(text), code)) => {
            eprintln!("adroit: {text}");
            code
        }
        None => {
            let mut ask = vec!["session".to_string()];
            ask.extend(args.iter().cloned());
            crate::work_host::run_command(ask, None)
        }
    }
}

/// `adroit session <args>`. Returns the process exit code.
pub fn cli(args: &[String]) -> i32 {
    // Read before the environment is cleared of the session's marks below.
    let acting_as = std::env::var(crate::delegation::launch::TOKEN_ENV)
        .ok()
        .zip(std::env::var(crate::delegation::launch::ROOT_ENV).ok());
    if let [op, rest @ ..] = args
        && op == "explain"
    {
        return explain_cli(args, rest);
    }
    let Some(asked) = asked(args) else {
        eprintln!("{USAGE}\n{}", crate::explain::USAGE);
        return 2;
    };
    let input = match &asked.prompt {
        Some(_) => None,
        None => {
            let mut p = String::new();
            if let Err(e) = std::io::stdin().read_to_string(&mut p) {
                eprintln!("adroit: cannot read the prompt: {e}");
                return 1;
            }
            Some(p)
        }
    };
    let Some((credential, root)) = acting_as else {
        let mut ask = vec!["session".to_string()];
        ask.extend(args.iter().cloned());
        return crate::work_host::run_command(ask, input);
    };
    let started = (|| {
        let asked = prepared(asked, input, &std::env::current_dir()?)?;
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(start_as(
                std::path::Path::new(&root),
                &credential,
                &asked.asked,
                asked.dir,
                &asked.prompt,
            ))
            .map_err(Failure::Run)
    })();
    match started {
        Ok(name) => {
            println!("{name}");
            0
        }
        Err(failure) => failure.told(),
    }
}

/// What `adroit session start` was asked, read from `args`; none when they
/// do not say it.
fn asked(args: &[String]) -> Option<Asked> {
    match args {
        [op, rest @ ..] if op == "start" => parse(rest),
        _ => None,
    }
}

/// Why `adroit session start` did not start a session.
#[derive(Debug)]
enum Failure {
    /// What it was asked cannot be done as asked.
    Asked(String),
    /// Starting it failed.
    Run(String),
}

impl Failure {
    /// Said on standard error; the exit code.
    fn told(self) -> i32 {
        match self {
            Failure::Asked(e) => {
                eprintln!("adroit: {e}");
                2
            }
            Failure::Run(e) => {
                eprintln!("adroit: {e}");
                1
            }
        }
    }

    fn text(&self) -> &str {
        match self {
            Failure::Asked(e) | Failure::Run(e) => e,
        }
    }

    fn code(&self) -> i32 {
        match self {
            Failure::Asked(_) => 2,
            Failure::Run(_) => 1,
        }
    }
}

impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        Failure::Run(e.to_string())
    }
}

/// A start asked for, its prompt and the folder it starts in settled.
struct Prepared {
    asked: Asked,
    dir: PathBuf,
    prompt: String,
}

/// `asked`, as run in `cwd` with `input` on its standard input: its custom
/// permissions read, its prompt taken, and the folder it starts in made.
fn prepared(
    mut asked: Asked,
    input: Option<String>,
    cwd: &std::path::Path,
) -> Result<Prepared, Failure> {
    if let Some(path) = &asked.permissions_file {
        let permissions = std::fs::read_to_string(cwd.join(path))
            .map_err(|e| e.to_string())
            .and_then(|text| {
                serde_json::from_str::<CustomPermissions>(&text).map_err(|e| e.to_string())
            })
            .map_err(|e| Failure::Asked(format!("cannot read custom permissions: {e}")))?;
        asked.permissions = AgentPermissions::Custom(Box::new(permissions));
    }
    let prompt = asked
        .prompt
        .clone()
        .or(input)
        .unwrap_or_default()
        .trim()
        .to_string();
    if prompt.is_empty() {
        return Err(Failure::Asked("the prompt is empty".into()));
    }
    let dir = asked
        .dir
        .as_ref()
        .map_or_else(|| cwd.to_path_buf(), |d| cwd.join(d));
    let dir = adroit_sys::fs::canonicalize(dir)
        .and_then(|d| scratch::start_dir(&d, &crate::paths::scratch_dir()))?;
    Ok(Prepared { asked, dir, prompt })
}

/// `adroit session <args>` as the work host runs it for a command run in
/// `cwd` with `input` on its standard input: what it prints, or why it
/// failed, with its exit code.
pub(crate) async fn run(
    args: &[String],
    cwd: &std::path::Path,
    input: Option<String>,
) -> crate::command::Output {
    use crate::command::Output;
    if let [op, rest @ ..] = args
        && op == "explain"
    {
        return crate::explain::run(rest).await;
    }
    let Some(asked) = asked(args) else {
        return Output {
            wrote: Err(USAGE.into()),
            code: 2,
            effects_possible: false,
        };
    };
    let prepared = match prepared(asked, input, cwd) {
        Ok(prepared) => prepared,
        Err(failure) => {
            return Output {
                wrote: Err(failure.text().to_string()),
                code: failure.code(),
                effects_possible: false,
            };
        }
    };
    let started = async {
        let daemon = crate::daemon::daemon();
        let listed = daemon
            .sessions()
            .await
            .map_err(|e| format!("cannot reach the daemon: {e}"))?;
        start(
            &daemon,
            &crate::settings::current(),
            &prepared.asked,
            prepared.dir,
            &prepared.prompt,
            &listed,
        )
        .await
    }
    .await
    .map_err(Failure::Run);
    match started {
        Ok(name) => Output {
            wrote: Ok(name),
            code: 0,
            effects_possible: true,
        },
        Err(failure) => Output {
            wrote: Err(failure.text().to_string()),
            code: failure.code(),
            effects_possible: true,
        },
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
                worktree: false,
                base: None,
                name: Some("flaky".into()),
                permissions: AgentPermissions::ReadOnly,
                permissions_file: None,
                agent: None,
                profile: None,
                model: None,
                background: false,
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
        assert_eq!(
            parse(&args(&["--agent", "pi"])).unwrap().agent,
            Some(adroit_model::Agent::Pi)
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
    fn the_command_can_name_a_profile_by_its_id() {
        let asked = parse(&args(&["--profile", "claude-work", "--", "go"])).unwrap();
        assert_eq!(
            asked.profile.map(String::from).as_deref(),
            Some("claude-work")
        );
        assert_eq!(parse(&args(&["--profile", "Claude Work"])), None);
        assert_eq!(parse(&args(&["--profile"])), None);
    }

    #[test]
    fn a_base_revision_is_taken_only_for_a_new_worktree() {
        let asked = parse(&args(&["--worktree", "--base", "main", "--", "go"])).unwrap();
        assert!(asked.worktree);
        assert_eq!(asked.base.as_deref(), Some("main"));
        assert_eq!(parse(&args(&["--base", "main", "--", "go"])), None);
    }

    #[test]
    fn a_session_opens_in_the_window_unless_started_in_the_background() {
        assert!(!parse(&args(&["--", "go"])).unwrap().background);
        assert!(
            parse(&args(&["--background", "--", "go"]))
                .unwrap()
                .background
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

    fn worktrees(source: &std::path::Path) -> usize {
        let listed = std::process::Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(source)
            .output()
            .unwrap();
        String::from_utf8_lossy(&listed.stdout)
            .lines()
            .filter(|line| line.starts_with("worktree "))
            .count()
    }

    #[tokio::test]
    async fn a_start_that_fails_before_its_agent_ran_takes_its_new_worktree_back() {
        let dir = tempfile::tempdir().unwrap();
        let source = crate::testing::repository(dir.path());
        let settings = crate::settings::Settings::parse(
            "[[profiles]]\nid = \"gone\"\nagent = \"claude\"\n[profiles.launch]\ncommand = [\"/nonexistent/claude\"]\n",
        )
        .unwrap();
        let asked = Asked {
            worktree: true,
            profile: Some("gone".parse().unwrap()),
            ..Default::default()
        };

        let refused = start(
            &Daemon::new(dir.path().join("unused.sock")),
            &settings,
            &asked,
            source.clone(),
            "review the change",
            &[],
        )
        .await
        .unwrap_err();

        assert!(refused.contains("/nonexistent/claude"), "{refused}");
        assert_eq!(worktrees(&source), 1);
    }
}
