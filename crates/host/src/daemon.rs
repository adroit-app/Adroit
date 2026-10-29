//! The terminal host, the felis daemon adroit's sessions run in, which
//! `adroit daemon` serves. It holds their terminals and nothing else.

use std::path::{Path, PathBuf};

use adroit_term::Daemon;

use crate::awake::{self, Assertion, Keeper};
use crate::cli::DaemonCommand;
use crate::paths::state_dir;
use crate::standing::{SAY_AGAIN, build, keep_saying, say_when_listening};
use crate::work_host::{self, adroit_program};

/// Where the terminal host of the instance listens.
pub fn socket() -> PathBuf {
    crate::paths::terminal_host_socket()
}

/// The daemon the window talks to.
pub fn daemon() -> Daemon {
    Daemon::new(socket()).with_startup_lease(crate::work_host::adroit_program())
}

/// The arguments that have adroit serve the daemon at its socket.
pub fn serve_args() -> Vec<String> {
    vec!["daemon".into()]
}

/// felis's terminfo entry for the `xterm-felis` its sessions run as, kept
/// here until felis hands it to the programs that embed its daemon
/// (felis-terminal/felis#8).
const TERMINFO: &str = include_str!("../terminfo/felis.terminfo");

/// Where the entry is compiled for sessions to find.
pub fn terminfo_dir() -> PathBuf {
    state_dir().join("adroit").join("terminfo")
}

/// Compile the entry into `dir`, so programs in a session can read the
/// terminal they run in.
pub fn install_terminfo(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let source = dir.join("felis.terminfo");
    std::fs::write(&source, TERMINFO)?;
    let out = std::process::Command::new("tic")
        .arg("-x")
        .arg("-o")
        .arg(dir)
        .arg(&source)
        .output()?;
    if !out.status.success() {
        return Err(std::io::Error::other(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

/// The `TERMINFO_DIRS` a session starts with: `dir` first, then what
/// adroit was given, or else the system's own folders, which an empty
/// element names.
pub fn terminfo_dirs(dir: &Path, given: Option<&str>) -> String {
    let given = given.filter(|g| !g.is_empty()).unwrap_or("");
    format!("{}:{given}", dir.display())
}

/// What every session's environment holds for its terminal. A shell or
/// Neovim terminal gets only this, so the editor it opens stays the user's.
pub fn terminal_env() -> Vec<(String, String)> {
    let given = std::env::var("TERMINFO_DIRS").ok();
    let mut env = vec![(
        "TERMINFO_DIRS".into(),
        terminfo_dirs(&terminfo_dir(), given.as_deref()),
    )];
    if let Some(context) = crate::environment::current() {
        env.push((crate::environment::ENV.into(), context.id.clone()));
    }
    env
}

/// What every agent session's environment holds, whoever starts it: its
/// terminal's, and adroit's editor, through which the window takes back
/// what was typed into the session's input.
pub fn agent_env() -> Vec<(String, String)> {
    let mut env = terminal_env();
    env.extend(crate::editor::environment());
    env
}

fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
}

/// How often the daemon counts what runs and reads whether to keep the Mac
/// awake: well within the minutes idle sleep waits.
const AWAKE_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// `adroit daemon`: serve sessions at the socket until stopped, or stop or
/// restart the daemon serving there. Returns the process exit code.
pub fn cli(asked: DaemonCommand) -> i32 {
    let rt = match runtime() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("adroit: {e}");
            return 1;
        }
    };
    let done = match asked {
        DaemonCommand::Serve => serve(&rt),
        DaemonCommand::Stop => rt.block_on(async {
            let stopped = daemon().stop().await?;
            println!(
                "{}",
                if stopped {
                    "stopped the daemon"
                } else {
                    "no daemon was running"
                }
            );
            Ok(())
        }),
        DaemonCommand::Restart => rt.block_on(async {
            let daemon = daemon();
            daemon.restart(&adroit_program(), &serve_args()).await?;
            println!("restarted the daemon");
            // Sessions started from here on need a work host to be judged.
            let host = work_host::socket();
            if let Err(e) =
                work_host::ensure(&host, &adroit_program(), &work_host::serve_args()).await
            {
                eprintln!("adroit: {e}");
            }
            Ok(())
        }),
    };
    // Tasks still running once the daemon has stopped serving are not
    // waited for: the process is to end with it.
    rt.shutdown_background();
    match done {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("adroit: {e}");
            1
        }
    }
}

fn serve(rt: &tokio::runtime::Runtime) -> adroit_term::Result<()> {
    let mut keeper = Keeper::new(|| Assertion::take(awake::NAME));
    rt.block_on(async {
        crate::environment::spawn(async {
            let daemon = daemon();
            let build = build();
            say_when_listening(&daemon, &build, adroit_term::SPEAKS).await;
            keep_saying(&daemon, &build, adroit_term::SPEAKS, SAY_AGAIN).await
        });
        let monitor = async {
            loop {
                if let Ok(sessions) = daemon().sessions().await {
                    let mut counts = std::collections::BTreeMap::new();
                    for session in sessions.into_iter().filter(|s| !s.exited) {
                        let scope =
                            adroit_term::tags::value(&session.tags, crate::environment::TAG);
                        *counts.entry(scope).or_insert(0) += 1;
                    }
                    keeper.keep(awake::environments_wanted(&counts, |scope| {
                        let Some(scope) = scope else {
                            return crate::settings::load().keep_awake;
                        };
                        if !adroit_model::paths::is_instance(scope) {
                            return true;
                        }
                        let file = adroit_model::paths::instance_state()
                            .join("environments")
                            .join(scope)
                            .join("config/settings.toml");
                        crate::settings::Settings::read(&crate::config::file(&file))
                            .map_or(true, |settings| settings.keep_awake)
                    }));
                }
                tokio::time::sleep(AWAKE_EVERY).await;
            }
        };
        let service_socket = socket();
        tokio::select! {
            result = Daemon::serve(&service_socket, AWAKE_EVERY, |_| {}) => result,
            () = monitor => unreachable!(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_terminal_entry_is_compiled_where_sessions_look_for_it() {
        let dir = tempfile::tempdir().unwrap();
        install_terminfo(dir.path()).unwrap();
        let found = ["78", "x"]
            .iter()
            .any(|leaf| dir.path().join(leaf).join("xterm-felis").is_file());
        assert!(found);
    }

    #[test]
    fn sessions_look_in_adroit_s_folder_first_then_where_they_would_anyway() {
        let dir = Path::new("/s/terminfo");
        assert_eq!(terminfo_dirs(dir, None), "/s/terminfo:");
        assert_eq!(terminfo_dirs(dir, Some("")), "/s/terminfo:");
        assert_eq!(terminfo_dirs(dir, Some("/opt/t")), "/s/terminfo:/opt/t");
    }
}
