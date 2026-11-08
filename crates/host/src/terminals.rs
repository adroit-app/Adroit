//! Terminal resources addressed independently of working directory and window.

use std::path::PathBuf;

use adroit_model::{SessionId, TerminalProgram, TerminalResourceId, TerminalSession};
use adroit_term::{Daemon, Input, Key};

use crate::execution::{TerminalStartRequest, resolve_terminal, spawn_headless};
use crate::sessions::{self, Kept};
use crate::settings::Settings;

pub async fn list(daemon: &Daemon) -> Result<Vec<TerminalSession>, String> {
    Ok(daemon
        .sessions()
        .await
        .map_err(|e| e.to_string())?
        .iter()
        .filter_map(|session| match sessions::read(session) {
            Some(Kept::Terminal(terminal)) => Some(terminal),
            _ => None,
        })
        .collect())
}

async fn find(daemon: &Daemon, resource: TerminalResourceId) -> Result<TerminalSession, String> {
    list(daemon)
        .await?
        .into_iter()
        .find(|t| t.resource_id == resource)
        .ok_or_else(|| format!("no terminal resource {resource}"))
}

pub async fn screen(daemon: &Daemon, resource: TerminalResourceId) -> Result<Vec<String>, String> {
    let terminal = find(daemon, resource).await?;
    let mut lines = daemon
        .visible(terminal.id)
        .await
        .map_err(|e| e.to_string())?;
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    Ok(lines)
}

pub async fn input(
    daemon: &Daemon,
    resource: TerminalResourceId,
    input: &[Input],
) -> Result<(), String> {
    let terminal = find(daemon, resource).await?;
    daemon
        .input(terminal.id, input)
        .await
        .map_err(|e| e.to_string())
}

/// Ask the Neovim the terminal `id` runs to quit as `:qa` does, which it
/// refuses while a buffer holds an unsaved change. It is typed in, as the
/// user would: Neovims started before adroit could listen to them have no
/// socket of adroit's, and a configured command may start one that never
/// does.
pub async fn quit_neovim(daemon: &Daemon, id: u128) -> Result<(), String> {
    daemon
        .input(id, &quit_keys())
        .await
        .map_err(|e| e.to_string())
}

/// `Ctrl+\ Ctrl+n` leaves whatever mode Neovim is in for Normal mode, and
/// drops a command line half typed, before `:qa` is typed.
fn quit_keys() -> Vec<Input> {
    vec![
        Input::Key(Key::new("\\").ctrl()),
        Input::Key(Key::new("n").ctrl()),
        Input::Text(":qa".into()),
        Input::Key(Key::new("Enter")),
    ]
}

pub async fn stop(daemon: &Daemon, resource: TerminalResourceId) -> Result<(), String> {
    let terminal = find(daemon, resource).await?;
    daemon.end(terminal.id).await.map_err(|e| e.to_string())
}

pub async fn start(
    daemon: &Daemon,
    settings: &Settings,
    request: TerminalStartRequest,
) -> Result<TerminalSession, String> {
    let palette = crate::themes::palette_in_force(settings)?;
    let mut resolved = resolve_terminal(settings, request, &crate::programs::login_shell());
    resolved.spawn.cols = 120;
    resolved.spawn.rows = 40;
    resolved.terminal.id = spawn_headless(daemon, resolved.spawn, &palette).await?;
    Ok(resolved.terminal)
}

#[derive(Debug, PartialEq, Eq)]
enum Operation {
    List,
    Screen(TerminalResourceId),
    Input(TerminalResourceId, Vec<Input>),
    Stop(TerminalResourceId),
    Start {
        program: TerminalProgram,
        dir: Option<PathBuf>,
        name: Option<String>,
        command: Vec<String>,
        /// Shown to the user while they look at the session `opener`.
        show: bool,
        opener: Option<SessionId>,
    },
}

/// A key written as the keys file writes it, as the window sends it; `None`
/// for one that never reaches a program.
fn key(text: &str) -> Option<Input> {
    let key = adroit_core::keymap::Chord::parse(text).ok()?.key();
    (!key.meta).then_some(Input::Key(key))
}

fn parse(args: &[String]) -> Option<(Operation, bool)> {
    let (command, rest) = args.split_first()?;
    let mut json = false;
    let mut resource = None;
    let mut program = TerminalProgram::Shell;
    let mut dir = None;
    let mut name = None;
    let mut input = Vec::new();
    let mut argv = None;
    let mut show = false;
    let mut opener = None;
    let mut rest = rest.iter().peekable();
    while let Some(flag) = rest.next() {
        match (command.as_str(), flag.as_str()) {
            (_, "--json") if !json => json = true,
            ("screen" | "stop" | "input", "--terminal") if resource.is_none() => {
                resource = Some(rest.next()?.parse().ok()?)
            }
            ("input", "--text") => input.push(Input::Text(rest.next()?.clone())),
            ("input", "--paste") => input.push(Input::Paste(rest.next()?.clone())),
            ("input", "--keys") => {
                let mut any = false;
                while let Some(k) = rest.next_if(|k| !k.starts_with("--")) {
                    input.push(key(k)?);
                    any = true;
                }
                if !any {
                    return None;
                }
            }
            ("start", "--program") => {
                program = match rest.next()?.as_str() {
                    "shell" => TerminalProgram::Shell,
                    "neovim" => TerminalProgram::Neovim,
                    "command" => TerminalProgram::Command,
                    _ => return None,
                }
            }
            ("start", "--dir") if dir.is_none() => dir = Some(rest.next()?.into()),
            ("start", "--name") if name.is_none() => name = Some(rest.next()?.clone()),
            ("start", "--show") if !show => show = true,
            ("start", "--opener") if opener.is_none() => opener = Some(rest.next()?.parse().ok()?),
            ("start", "--") => {
                argv = Some(rest.by_ref().cloned().collect::<Vec<_>>());
            }
            _ => return None,
        }
    }
    let operation = match command.as_str() {
        "list" => Operation::List,
        "screen" => Operation::Screen(resource?),
        "input" if !input.is_empty() => Operation::Input(resource?, input),
        "stop" => Operation::Stop(resource?),
        "start" => {
            let command = match (program, argv) {
                (TerminalProgram::Command, Some(argv)) if !argv.is_empty() => argv,
                (TerminalProgram::Command, _) | (_, Some(_)) => return None,
                (_, None) => Vec::new(),
            };
            Operation::Start {
                program,
                dir,
                name,
                command,
                show,
                opener,
            }
        }
        _ => return None,
    };
    Some((operation, json))
}

pub use adroit_model::base_name;

async fn execute(
    daemon: &Daemon,
    operation: Operation,
    json: bool,
    cwd: &std::path::Path,
) -> Result<String, String> {
    match operation {
        Operation::List => {
            let terminals = list(daemon).await?;
            if json {
                serde_json::to_string(&terminals).map_err(|e| e.to_string())
            } else {
                Ok(terminals
                    .iter()
                    .map(|t| format!("{}\t{}", t.resource_id, t.name))
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
        }
        Operation::Screen(resource) => {
            let lines = screen(daemon, resource).await?;
            if json {
                let exit = find(daemon, resource).await?.exit;
                Ok(
                    serde_json::json!({"resource_id": resource, "lines": lines, "exit": exit})
                        .to_string(),
                )
            } else {
                Ok(lines.join("\n"))
            }
        }
        Operation::Input(resource, sent) => {
            input(daemon, resource, &sent).await?;
            Ok(if json {
                serde_json::json!({"resource_id": resource, "sent": true}).to_string()
            } else {
                resource.to_string()
            })
        }
        Operation::Stop(resource) => {
            stop(daemon, resource).await?;
            Ok(if json {
                serde_json::json!({"resource_id": resource, "stopped": true}).to_string()
            } else {
                resource.to_string()
            })
        }
        Operation::Start {
            program,
            dir,
            name,
            command,
            show,
            opener,
        } => {
            let cwd = adroit_sys::fs::canonicalize(
                dir.map_or_else(|| cwd.to_path_buf(), |d| cwd.join(d)),
            )
            .map_err(|e| e.to_string())?;
            if !cwd.is_dir() {
                return Err("the terminal directory is not a folder".into());
            }
            let taken: Vec<_> = list(daemon)
                .await?
                .into_iter()
                .filter(|t| t.cwd == cwd)
                .map(|t| t.name)
                .collect();
            let names: Vec<_> = taken.iter().map(String::as_str).collect();
            let base = name.unwrap_or_else(|| base_name(program, &command));
            let name = crate::content::fresh_name(&base, &names);
            let terminal = start(
                daemon,
                &crate::settings::current(),
                TerminalStartRequest {
                    name,
                    cwd,
                    program,
                    // Not `agent_env`: a terminal keeps the user's own editor.
                    env: crate::daemon::terminal_env(),
                    file: None,
                    command,
                    opener,
                    show,
                },
            )
            .await?;
            if json {
                serde_json::to_string(&terminal).map_err(|e| e.to_string())
            } else {
                Ok(terminal.resource_id.to_string())
            }
        }
    }
}

const USAGE: &str = "usage: adroit terminal start [--program shell|neovim] [--dir PATH] [--name NAME] [--show] [--json]\n       adroit terminal start --program command [--dir PATH] [--name NAME] [--show] [--json] -- PROGRAM [ARGS...]\n       adroit terminal list [--json]\n       adroit terminal input --terminal ID [--text TEXT | --paste TEXT | --keys KEY...]... [--json]\n       adroit terminal screen|stop --terminal ID [--json]";

/// `adroit terminal <args>`, run by the work host. Returns the process exit
/// code.
pub fn cli(args: &[String]) -> i32 {
    match asked_as(args, opener) {
        Ok(ask) => crate::work_host::run_command(ask, None),
        Err(code) => code,
    }
}

/// What `adroit terminal <args>` asks the work host, naming the session it
/// runs in, by `opener`, for a terminal to be shown beside it; or the exit
/// code it ends with, having said why. Only the command names that session.
fn asked_as(
    args: &[String],
    opener: impl FnOnce() -> Result<String, String>,
) -> Result<Vec<String>, i32> {
    let Some((operation, _)) = parse(args) else {
        eprintln!("{USAGE}");
        return Err(2);
    };
    let named = match operation {
        Operation::Start {
            opener: Some(_), ..
        } => {
            eprintln!("{USAGE}");
            return Err(2);
        }
        Operation::Start { show: true, .. } => match opener() {
            Ok(id) => vec!["--opener".to_string(), id],
            Err(error) => {
                eprintln!("adroit: {error}");
                return Err(2);
            }
        },
        _ => Vec::new(),
    };
    let (command, rest) = args.split_first().expect("parsed");
    Ok(["terminal".to_string(), command.clone()]
        .into_iter()
        .chain(named)
        .chain(rest.iter().cloned())
        .collect())
}

/// The session this runs in, by Adroit's record of it, as the session's own
/// credential tells the work host.
fn opener() -> Result<String, String> {
    use crate::delegation::launch::{ROOT_ENV, TOKEN_ENV};
    use crate::delegation::protocol::{Command, ask};
    let (Ok(credential), Ok(root)) = (std::env::var(TOKEN_ENV), std::env::var(ROOT_ENV)) else {
        return Err(
            "--show shows a terminal beside the session Adroit runs that starts it; run it in one"
                .into(),
        );
    };
    let context = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?
        .block_on(ask(
            std::path::Path::new(&root),
            &credential,
            Command::Context,
            None,
        ))
        .map_err(|e| e.to_string())?;
    context["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "the work host did not say which session this is".into())
}

/// `adroit terminal <args>` as the work host runs it for a command run in
/// `cwd`: what it prints, or why it failed, with its exit code.
pub(crate) async fn run(args: &[String], cwd: &std::path::Path) -> crate::command::Output {
    use crate::command::Output;
    let Some((operation, json)) = parse(args) else {
        return Output {
            wrote: Err(USAGE.into()),
            code: 2,
            effects_possible: false,
        };
    };
    let effects_possible = matches!(
        operation,
        Operation::Input(..) | Operation::Stop(..) | Operation::Start { .. }
    );
    let done = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        execute(&crate::daemon::daemon(), operation, json, cwd),
    )
    .await
    .unwrap_or_else(|_| {
        Err("the terminal command timed out; a requested start may have completed".into())
    });
    let code = if done.is_ok() { 0 } else { 1 };
    Output {
        wrote: done,
        code,
        effects_possible,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_term::Key;

    #[test]
    fn screen_and_stop_address_a_resource_and_never_guess_from_a_name_or_cwd() {
        let id = TerminalResourceId::new();
        let args = [
            "screen".into(),
            "--terminal".into(),
            id.to_string(),
            "--json".into(),
        ];
        assert_eq!(parse(&args), Some((Operation::Screen(id), true)));
        assert_eq!(parse(&["screen".into(), "server".into()]), None);
        assert_eq!(parse(&["stop".into()]), None);
    }

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn input_sends_text_pastes_and_keys_in_the_order_given() {
        let id = TerminalResourceId::new();
        let id_text = id.to_string();
        let parsed = parse(&args(&[
            "input",
            "--terminal",
            &id_text,
            "--text",
            "hi",
            "--keys",
            "Tab",
            "Shift+Tab",
            "Ctrl+g",
            "--paste",
            "a\nb",
            "--keys",
            "Enter",
            "--json",
        ]));
        assert_eq!(
            parsed,
            Some((
                Operation::Input(
                    id,
                    vec![
                        Input::Text("hi".into()),
                        Input::Key(Key::new("Tab")),
                        Input::Key(Key::new("Tab").shift()),
                        Input::Key(Key::new("g").ctrl()),
                        Input::Paste("a\nb".into()),
                        Input::Key(Key::new("Enter")),
                    ]
                ),
                true
            ))
        );
    }

    #[test]
    fn input_names_keys_as_the_keys_file_does_and_refuses_what_never_reaches_a_program() {
        let id = TerminalResourceId::new().to_string();
        let input = |rest: &[&str]| {
            let mut words = vec!["input", "--terminal", &id];
            words.extend(rest);
            parse(&args(&words))
        };
        assert!(input(&["--keys", "Escape", "Space", "G", "F5", "ArrowUp"]).is_some());
        assert_eq!(input(&["--keys", "Return"]), None);
        assert_eq!(input(&["--keys", "Cmd+c"]), None);
        assert_eq!(input(&["--keys"]), None);
        assert_eq!(input(&["--text"]), None);
        assert_eq!(input(&[]), None);
        assert_eq!(parse(&args(&["input", "--text", "hi"])), None);
    }

    #[test]
    fn a_command_terminal_takes_its_program_and_arguments_after_a_double_dash() {
        let parsed = parse(&args(&[
            "start",
            "--program",
            "command",
            "--dir",
            "/src",
            "--",
            "claude",
            "--debug",
        ]));
        assert_eq!(
            parsed,
            Some((
                Operation::Start {
                    program: TerminalProgram::Command,
                    dir: Some("/src".into()),
                    name: None,
                    command: args(&["claude", "--debug"]),
                    show: false,
                    opener: None,
                },
                false
            ))
        );
        assert_eq!(parse(&args(&["start", "--program", "command"])), None);
        assert_eq!(parse(&args(&["start", "--program", "command", "--"])), None);
        assert_eq!(parse(&args(&["start", "--", "claude"])), None);
    }

    #[test]
    fn a_terminal_can_be_asked_to_be_shown_to_the_user_beside_the_session_that_started_it() {
        let opener = adroit_model::SessionId::new();
        let opener_text = opener.to_string();
        let shown = |words: &[&str]| match parse(&args(words)) {
            Some((Operation::Start { show, opener, .. }, _)) => Some((show, opener)),
            _ => None,
        };
        assert_eq!(
            shown(&[
                "start",
                "--program",
                "command",
                "--show",
                "--",
                "sudo",
                "true"
            ]),
            Some((true, None))
        );
        assert_eq!(
            shown(&["start", "--show", "--opener", &opener_text]),
            Some((true, Some(opener)))
        );
        assert_eq!(shown(&["start"]), Some((false, None)));
        assert_eq!(shown(&["start", "--show", "--show"]), None);
        assert_eq!(parse(&args(&["screen", "--show"])), None);
    }

    #[test]
    fn only_the_command_names_the_session_a_terminal_is_shown_beside() {
        let opener = adroit_model::SessionId::new().to_string();
        let given = args(&["start", "--show", "--opener", &opener]);
        assert!(matches!(asked_as(&given, || unreachable!()), Err(2)));
        let asked = asked_as(
            &args(&["start", "--program", "command", "--show", "--", "x"]),
            || Ok(opener.clone()),
        );
        assert_eq!(
            asked.ok(),
            Some(args(&[
                "terminal",
                "start",
                "--opener",
                &opener,
                "--program",
                "command",
                "--show",
                "--",
                "x"
            ]))
        );
        let refused = asked_as(&args(&["start", "--show"]), || Err("outside".into()));
        assert!(matches!(refused, Err(2)));
        let plain = asked_as(&args(&["list"]), || unreachable!());
        assert_eq!(plain.ok(), Some(args(&["terminal", "list"])));
    }

    #[test]
    fn a_terminal_is_named_after_what_it_runs() {
        assert_eq!(base_name(TerminalProgram::Shell, &[]), "shell");
        assert_eq!(base_name(TerminalProgram::Neovim, &[]), "nvim");
        assert_eq!(
            base_name(
                TerminalProgram::Command,
                &args(&["/usr/local/bin/claude", "--debug"])
            ),
            "claude"
        );
    }
}
