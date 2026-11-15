//! Holding a session's program back until its pane has told it the colours:
//! by a shell gate, this executable on Windows, or a startup lease.

use crate::daemon::{Daemon, Error, Result, Spawn};

/// Holds a program back until a line comes in. felis runs a session's
/// program the moment it makes the session, before a window can tell the
/// session its colours, so a program asking for them at once would race the
/// window; held, it cannot ask before the line, which is sent after them.
/// The line's echo moves the cursor down, and is taken back.
#[cfg(unix)]
const GATE: &str = r#"IFS= read -r _; printf '\033[H'; exec "$0" "$@""#;

/// What lets a [`gated`] program go.
pub(crate) const RELEASE: &[u8] = b"\r";

/// `spawn`, its program held back until [`RELEASE`] comes in.
#[cfg(unix)]
pub(crate) fn gated(spawn: Spawn) -> Spawn {
    let args = ["-c".to_string(), GATE.to_string(), spawn.program]
        .into_iter()
        .chain(spawn.args)
        .collect();
    Spawn {
        program: "/bin/sh".into(),
        args,
        ..spawn
    }
}

/// `spawn`, its program held back until [`RELEASE`] comes in by this
/// executable, as [`hold`] does: Windows has no shell to hold it.
#[cfg(windows)]
pub(crate) fn gated(spawn: Spawn) -> Spawn {
    let program = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "adroit".into());
    let args = [HOLD.to_string(), spawn.program]
        .into_iter()
        .chain(spawn.args)
        .collect();
    Spawn {
        program,
        args,
        ..spawn
    }
}

/// The argument that starts this executable as [`hold`].
#[cfg(windows)]
pub const HOLD: &str = "__hold";

/// Run as a held program's gate, with `args` the program and its arguments:
/// wait for a line, take its echo back as the shell gate does, then run the
/// program on this console and end with its exit code.
#[cfg(windows)]
pub fn hold(args: &[String]) -> i32 {
    use std::io::Write;
    let Some((program, rest)) = args.split_first() else {
        eprintln!("adroit: nothing to hold");
        return 2;
    };
    let _ = std::io::stdin().read_line(&mut String::new());
    let _ = std::io::stdout()
        .write_all(b"\x1b[H")
        .and_then(|()| std::io::stdout().flush());
    let cwd = std::env::current_dir().unwrap_or_default();
    let resolved = adroit_sys::path::resolved(program, &cwd, &[]).unwrap_or_else(|| program.into());
    match std::process::Command::new(&resolved).args(rest).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("adroit: cannot start {program}: {error}");
            127
        }
    }
}

/// `spawn` [`gated`], refused as the daemon would refuse it when its program
/// is missing: behind the gate the daemon starts a shell, which is always
/// there.
fn held(spawn: Spawn) -> Result<Spawn> {
    if !found(&spawn) {
        return Err(Error::NotFound(spawn.program));
    }
    Ok(gated(spawn))
}

pub(crate) fn held_by(daemon: &Daemon, spawn: Spawn) -> Result<Spawn> {
    if let Some(program) = &daemon.startup_lease {
        if !found(&spawn) {
            return Err(Error::NotFound(spawn.program));
        }
        return crate::gate::leased(spawn, program, std::time::Duration::from_secs(10));
    }
    held(spawn)
}

impl Daemon {
    /// Use this executable's startup lease for every held program, including
    /// programs started with an unshown pane.
    pub fn with_startup_lease(mut self, program: std::path::PathBuf) -> Self {
        self.startup_lease = Some(program);
        self
    }
    /// Start a session as `spawn` says with its program held back, returning
    /// its id. A pane attached to it tells it the colours; [`Daemon::release`]
    /// then lets the program go, so the first colour it asks for is theirs.
    pub async fn spawn_held(&self, spawn: Spawn) -> Result<u128> {
        self.spawn(held_by(self, spawn)?).await
    }

    /// Hold a program with a terminal-owned deadline, independent of the work host.
    pub async fn spawn_held_for(
        &self,
        spawn: Spawn,
        program: &std::path::Path,
        timeout: std::time::Duration,
    ) -> Result<u128> {
        if !found(&spawn) {
            return Err(Error::NotFound(spawn.program));
        }
        self.spawn(crate::gate::leased(spawn, program, timeout)?)
            .await
    }

    /// Let the program of a session [`Daemon::spawn_held`] made go.
    pub async fn release(&self, id: u128) -> Result<()> {
        if let Some(socket) = self
            .sessions()
            .await?
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| crate::tags::value(&s.tags, "adroit:startup-lease"))
        {
            return crate::gate::release(std::path::Path::new(&socket)).await;
        }
        self.press(id, RELEASE).await
    }
}

/// Whether `spawn`'s program is one that can run, looked up on the `PATH`
/// it is started with.
fn found(spawn: &Spawn) -> bool {
    adroit_sys::path::runnable(&spawn.program, &spawn.cwd, &spawn.env)
}
