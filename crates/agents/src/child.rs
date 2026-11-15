//! A question put to an agent's program in JSON lines, in a process started
//! only to answer it.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

use crate::Started;

/// The input and output of a child being asked something.
pub(crate) struct Conversation {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Conversation {
    /// Write `text` to the child's input, flushed.
    pub(crate) async fn write(&mut self, text: &str) -> Option<()> {
        self.stdin.write_all(text.as_bytes()).await.ok()?;
        self.stdin.flush().await.ok()
    }

    /// Read the child's output a line at a time until `read` settles one,
    /// passing over a line that is not text; `None` once the output ends.
    pub(crate) async fn until<T>(&mut self, mut read: impl FnMut(&str) -> Option<T>) -> Option<T> {
        let mut line = Vec::new();
        loop {
            line.clear();
            // Split on LF alone: a JSON string may hold U+2028.
            if self.stdout.read_until(b'\n', &mut line).await.ok()? == 0 {
                return None;
            }
            let Ok(text) = std::str::from_utf8(&line) else {
                continue;
            };
            let text = text.strip_suffix('\n').unwrap_or(text);
            if let Some(settled) = read(text.strip_suffix('\r').unwrap_or(text)) {
                return Some(settled);
            }
        }
    }
}

/// What `talk` makes of the program `started` runs in `dir`, its input and
/// output piped; `None` when it cannot be started or `talk` has not settled
/// within `timeout`. The program is killed either way.
pub(crate) async fn converse<T>(
    (program, args, env): Started,
    dir: &Path,
    timeout: Duration,
    talk: impl AsyncFnOnce(&mut Conversation) -> Option<T>,
) -> Option<T> {
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .envs(env)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let mut conversation = Conversation {
        stdin: child.stdin.take()?,
        stdout: BufReader::new(child.stdout.take()?),
    };
    let got = tokio::time::timeout(timeout, talk(&mut conversation))
        .await
        .ok()
        .flatten();
    let _ = child.start_kill();
    let _ = child.wait().await;
    got
}

#[cfg(test)]
mod tests;
