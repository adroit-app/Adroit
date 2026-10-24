//! `adroit session start`: start a session in the daemon from a command,
//! the user's own or that of a session handing work on.

use std::io::Read;
use std::path::PathBuf;

use adroit_model::time::now_ms;
use adroit_model::{AgentPermissions, CustomPermissions};
use adroit_term::Daemon;

use crate::content::{fresh_name, label_from, label_of};
use crate::scratch::{self, Moment};
use crate::sessions::{self, Kept};

const USAGE: &str = "usage: adroit session start [--dir <path>] [--worktree [--base <rev>]] [--name <name>] [--permissions <default|read_only> | --permissions-file <path>] [--agent <claude|codex>] [--model <model>] [-- <prompt>]";

/// The size a session starts at, until a window shows it at its own.
const COLS: u16 = 120;
const ROWS: u16 = 40;

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
            "--worktree" => asked.worktree = true,
            "--base" => asked.base = Some(rest.next()?.clone()),
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
    (asked.worktree || asked.base.is_none()).then_some(asked)
}

/// The name the session goes by: the one given, or one made from its prompt,
/// numbered when taken.
pub(crate) fn name_for(given: Option<&str>, prompt: &str, taken: &[&str]) -> String {
    let label = given
        .and_then(label_of)
        .unwrap_or_else(|| label_from(prompt));
    fresh_name(&label, taken)
}

/// Starts the session `asked` for as the user's own.
async fn start(
    daemon: &Daemon,
    asked: &Asked,
    dir: PathBuf,
    prompt: &str,
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
    let cwd = if asked.worktree {
        let summary = prompt.lines().next().unwrap_or_default();
        adroit_vcs::worktree::create(&dir, &name, asked.base.as_deref(), summary)
            .await?
            .path
    } else {
        dir
    };
    let settings = crate::settings::current();
    let request = crate::execution::StartRequest {
        agent: asked.agent.unwrap_or(settings.default_agent()),
        model: asked.model.clone(),
        name: name.clone(),
        cwd,
        permissions: asked.permissions.clone(),
        prompt: prompt.into(),
        env: crate::daemon::agent_env(),
        started_ms: now_ms(),
        ..Default::default()
    };
    let program = std::env::current_exe().unwrap_or_else(|_| "adroit".into());
    let (resolved, recorded) = crate::delegation::app::prepare(
        &crate::delegation::app::root(),
        &settings,
        request,
        None,
        &program,
    )
    .await?;
    let mut spawn = resolved.spawn;
    spawn.cols = COLS;
    spawn.rows = ROWS;
    let palette = crate::themes::palette_in_force(&settings)?;
    let attach = recorded.clone();
    let started =
        crate::execution::spawn_headless_prepared(daemon, spawn, &palette, move |id| async move {
            attach.attach(id).await
        })
        .await;
    if started.is_err() {
        recorded.failed().await;
    }
    started.map_err(|e| format!("cannot start {name}: {e}"))?;
    Ok(name)
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
            model: asked.model.clone(),
            permissions: Some(asked.permissions.clone()),
            name: asked.name.clone(),
        });
    let started = crate::delegation::protocol::ask(root, credential, command, None)
        .await
        .map_err(|error| error.to_string())?;
    Ok(started["name"].as_str().unwrap_or_default().to_owned())
}

/// `adroit session <args>`. Returns the process exit code.
pub fn cli(args: &[String]) -> i32 {
    // Read before the environment is cleared of the session's marks below.
    let acting_as = std::env::var(crate::delegation::launch::TOKEN_ENV)
        .ok()
        .zip(std::env::var(crate::delegation::launch::ROOT_ENV).ok());
    let Some(asked) = asked(args) else {
        eprintln!("{USAGE}");
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
    let dir = std::fs::canonicalize(dir)
        .and_then(|d| scratch::start_dir(&d, &crate::paths::scratch_dir(), Moment::now()))?;
    Ok(Prepared { asked, dir, prompt })
}

/// `adroit session <args>` as the work host runs it for a command run in
/// `cwd` with `input` on its standard input: what it prints, or why it
/// failed, with its exit code.
pub(crate) async fn run(
    args: &[String],
    cwd: &std::path::Path,
    input: Option<String>,
) -> (Result<String, String>, i32) {
    let Some(asked) = asked(args) else {
        return (Err(USAGE.into()), 2);
    };
    let started = match prepared(asked, input, cwd) {
        Ok(p) => start(&crate::daemon::daemon(), &p.asked, p.dir, &p.prompt)
            .await
            .map_err(Failure::Run),
        Err(failure) => Err(failure),
    };
    match started {
        Ok(name) => (Ok(name), 0),
        Err(failure) => (Err(failure.text().to_string()), failure.code()),
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
    fn a_base_revision_is_taken_only_for_a_new_worktree() {
        let asked = parse(&args(&["--worktree", "--base", "main", "--", "go"])).unwrap();
        assert!(asked.worktree);
        assert_eq!(asked.base.as_deref(), Some("main"));
        assert_eq!(parse(&args(&["--base", "main", "--", "go"])), None);
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
}
