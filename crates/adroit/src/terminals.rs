//! Terminal resources addressed independently of working directory and window.

use std::path::PathBuf;

use adroit_model::{RunId, TerminalProgram, TerminalResourceId, TerminalSession};
use adroit_term::Daemon;

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

pub async fn stop(daemon: &Daemon, resource: TerminalResourceId) -> Result<(), String> {
    let terminal = find(daemon, resource).await?;
    daemon.end(terminal.id).await.map_err(|e| e.to_string())
}

pub async fn start(
    daemon: &Daemon,
    settings: &Settings,
    request: TerminalStartRequest,
) -> Result<TerminalSession, String> {
    let palette = crate::themes::palette(&settings.theme, &Settings::themes_dir())?;
    let mut resolved = resolve_terminal(settings, request, &crate::programs::login_shell());
    resolved.spawn.cols = 120;
    resolved.spawn.rows = 40;
    resolved.terminal.id = spawn_headless(daemon, resolved.spawn, &palette).await?;
    Ok(resolved.terminal)
}

#[derive(Debug, PartialEq, Eq)]
enum Operation {
    List(Option<RunId>),
    Screen(TerminalResourceId),
    Stop(TerminalResourceId),
    Start {
        program: TerminalProgram,
        dir: Option<PathBuf>,
        name: Option<String>,
    },
}

fn parse(args: &[String]) -> Option<(Operation, bool)> {
    let (command, rest) = args.split_first()?;
    let mut json = false;
    let mut resource = None;
    let mut run = None;
    let mut program = TerminalProgram::Shell;
    let mut dir = None;
    let mut name = None;
    let mut rest = rest.iter();
    while let Some(flag) = rest.next() {
        match (command.as_str(), flag.as_str()) {
            (_, "--json") if !json => json = true,
            ("screen" | "stop", "--terminal") if resource.is_none() => {
                resource = Some(rest.next()?.parse().ok()?)
            }
            ("list", "--run") if run.is_none() => run = Some(rest.next()?.parse().ok()?),
            ("start", "--program") => {
                program = match rest.next()?.as_str() {
                    "shell" => TerminalProgram::Shell,
                    "neovim" => TerminalProgram::Neovim,
                    _ => return None,
                }
            }
            ("start", "--dir") if dir.is_none() => dir = Some(rest.next()?.into()),
            ("start", "--name") if name.is_none() => name = Some(rest.next()?.clone()),
            _ => return None,
        }
    }
    let operation = match command.as_str() {
        "list" => Operation::List(run),
        "screen" => Operation::Screen(resource?),
        "stop" => Operation::Stop(resource?),
        "start" => Operation::Start { program, dir, name },
        _ => return None,
    };
    Some((operation, json))
}

async fn execute(daemon: &Daemon, operation: Operation, json: bool) -> Result<String, String> {
    match operation {
        Operation::List(run) => {
            let terminals: Vec<_> = list(daemon)
                .await?
                .into_iter()
                .filter(|t| {
                    run.is_none() || t.binding.as_ref().is_some_and(|b| Some(b.run_id) == run)
                })
                .collect();
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
                Ok(serde_json::json!({"resource_id": resource, "lines": lines}).to_string())
            } else {
                Ok(lines.join("\n"))
            }
        }
        Operation::Stop(resource) => {
            stop(daemon, resource).await?;
            Ok(if json {
                serde_json::json!({"resource_id": resource, "stopped": true}).to_string()
            } else {
                resource.to_string()
            })
        }
        Operation::Start { program, dir, name } => {
            let cwd = dir
                .map_or_else(std::env::current_dir, Ok)
                .and_then(std::fs::canonicalize)
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
            let base = match program {
                TerminalProgram::Shell => "shell",
                TerminalProgram::Neovim => "nvim",
            };
            let name = crate::tabs::fresh_name(name.as_deref().unwrap_or(base), &names);
            let terminal = start(
                daemon,
                &crate::settings::current(),
                TerminalStartRequest {
                    name,
                    cwd,
                    program,
                    env: crate::daemon::terminal_env(),
                    binding: None,
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

pub fn cli(args: &[String]) -> i32 {
    let Some((operation, json)) = parse(args) else {
        eprintln!(
            "usage: adroit terminal start [--program shell|neovim] [--dir PATH] [--name NAME] [--json]\n       adroit terminal list [--run ID] [--json]\n       adroit terminal screen|stop --terminal ID [--json]"
        );
        return 2;
    };
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())
        .and_then(|rt| {
            rt.block_on(async {
                tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    execute(&crate::daemon::daemon(), operation, json),
                )
                .await
                .map_err(|_| {
                    "the terminal command timed out; a requested start may have completed"
                        .to_string()
                })?
            })
        });
    match result {
        Ok(output) => {
            println!("{output}");
            0
        }
        Err(error) => {
            eprintln!("adroit: {error}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
