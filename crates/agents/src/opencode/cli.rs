//! Asking OpenCode something through one of its commands, which starts a
//! server of its own for the command and ends with it.

use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use adroit_model::ProviderModel;

/// How long a command has to answer: it starts a server before it asks it.
pub(super) const ASKED_FOR: Duration = Duration::from_secs(20);

/// The exit status and output of the command `start` runs in `dir`; none
/// when it cannot be started or has not ended within `timeout`.
pub(super) async fn ask(
    (program, args, env): crate::Started,
    dir: &Path,
    timeout: Duration,
) -> Option<(Option<i32>, String)> {
    let child = tokio::process::Command::new(program)
        .args(args)
        .envs(env)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let out = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .ok()?
        .ok()?;
    Some((
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    ))
}

/// [`ask`], for a caller that may block: what lists and reads OpenCode's
/// conversations runs off the work host's tasks.
pub(super) fn ask_blocking(
    (program, args, env): crate::Started,
    dir: &Path,
    timeout: Duration,
) -> Option<(Option<i32>, String)> {
    let mut child = std::process::Command::new(program)
        .args(args)
        .envs(env)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    // Read apart from the wait, so a long answer cannot fill the pipe and
    // hold the command up until it is given up on.
    let reading = std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        out
    });
    let until = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let out = reading.join().ok()?;
    Some((status.code(), String::from_utf8_lossy(&out).into_owned()))
}

/// The models `opencode models` listed in `out`, one `provider/model` a
/// line.
pub(super) fn models(out: &str) -> Vec<ProviderModel> {
    out.lines()
        .map(str::trim)
        .filter(|line| {
            line.split_once('/')
                .is_some_and(|(provider, model)| !provider.is_empty() && !model.is_empty())
        })
        .map(|line| ProviderModel {
            id: line.into(),
            resolved: None,
            name: line.into(),
            description: String::new(),
        })
        .collect()
}
