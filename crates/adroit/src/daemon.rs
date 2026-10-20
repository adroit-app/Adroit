//! The felis daemon adroit's sessions run in, which `adroit daemon` serves.

use std::path::{Path, PathBuf};

use adroit_term::Daemon;

use crate::awake::{self, Assertion, Keeper};
use crate::paths::{instance_state, state_dir};
use crate::settings::Settings;

/// `$ADROIT_DAEMON`, or `adroit/felis/daemon.sock` under the state directory.
pub fn socket() -> PathBuf {
    std::env::var_os("ADROIT_DAEMON")
        .map(PathBuf::from)
        .unwrap_or_else(|| instance_state().join("felis").join("daemon.sock"))
}

/// The daemon the window talks to.
pub fn daemon() -> Daemon {
    Daemon::new(socket())
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

/// What every session's environment holds for its terminal.
pub fn terminal_env() -> Vec<(String, String)> {
    let given = std::env::var("TERMINFO_DIRS").ok();
    vec![(
        "TERMINFO_DIRS".into(),
        terminfo_dirs(&terminfo_dir(), given.as_deref()),
    )]
}

fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
}

/// How often the daemon counts what runs and reads whether to keep the Mac
/// awake: well within the minutes idle sleep waits.
const AWAKE_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// `adroit daemon`: serve sessions at the socket until killed. Returns the
/// process exit code.
pub fn cli() -> i32 {
    let mut keeper = Keeper::new(|| Assertion::take(awake::NAME));
    // The setting is read from its file each time, as the window that
    // changes it is another process, which may not be open.
    let running = move |n| keeper.keep(awake::wanted(n, Settings::load().keep_awake));
    let served = runtime().map_err(adroit_term::Error::from).and_then(|rt| {
        rt.block_on(async {
            let mut control =
                match crate::control::service::Service::bind(&crate::control::root()).await {
                    Ok(service) => Some(service),
                    Err(error) => {
                        eprintln!("adroit: orchestration is unavailable: {error}");
                        None
                    }
                };
            if let Some(service) = &mut control {
                let program = std::env::current_exe().unwrap_or_else(|_| "adroit".into());
                if let Err(error) = service.supervise(daemon(), program, Settings::load) {
                    eprintln!("adroit: orchestration supervision is unavailable: {error}");
                }
            }
            Daemon::serve(&socket(), AWAKE_EVERY, running).await
        })
    });
    match served {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("adroit: {e}");
            1
        }
    }
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
