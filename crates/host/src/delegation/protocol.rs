//! What a session asks the work host, one JSON line each way.
use std::path::{Path, PathBuf};

use adroit_model::{Agent, AgentPermissions, MessageId, SessionId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::{Code, Error, Result};

pub const VERSION: u32 = 1;
pub const MAX_FRAME: usize = 1024 * 1024;

pub fn socket(root: &Path) -> PathBuf {
    root.join("delegation.sock")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub version: u32,
    pub credential: String,
    pub command: Command,
    /// The Codex thread a tool was called from, which tells the session's own
    /// conversation from its subagents'.
    #[serde(default)]
    pub thread: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub version: u32,
    pub result: std::result::Result<Value, Error>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Command {
    Context,
    StartSession(Start),
    Send(Send),
    Sessions(Sessions),
    Models,
    /// The oldest message waiting for the caller, as its agent is to read it.
    Next,
    Delivered {
        id: MessageId,
    },
}

impl Command {
    /// Whether the command acts for the session rather than reading.
    pub fn acts(&self) -> bool {
        matches!(self, Self::StartSession(_) | Self::Send(_))
    }
}

/// Open a session beside you with a first prompt.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Start {
    /// The complete first prompt: the line of work and all it needs to know of what you know.
    pub prompt: String,
    /// The directory to work in, or to make the worktree of; your own when omitted.
    #[serde(default)]
    pub dir: Option<PathBuf>,
    /// Work in a new worktree of that directory: a jj workspace or a Git worktree.
    #[serde(default)]
    pub worktree: bool,
    /// The revision the new worktree starts from; your current change or commit when omitted.
    #[serde(default)]
    pub base: Option<String>,
    /// `claude`, `codex`, `pi` or `opencode`; the agent of `profile`, else of the default profile, when omitted.
    #[serde(default)]
    pub agent: Option<Agent>,
    /// The profile it starts from, by id (see `models`); the agent's default profile when omitted, never your own.
    #[serde(default)]
    pub profile: Option<adroit_model::profile::ProfileId>,
    /// A model the agent lists (see `models`); its default when omitted.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub permissions: Option<AgentPermissions>,
    /// A short name; made from the prompt when omitted.
    #[serde(default)]
    pub name: Option<String>,
}

/// Send a message to another session.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Send {
    /// The recipient's id, as `sessions` lists it.
    pub to: SessionId,
    pub body: String,
}

/// List the sessions Adroit runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Sessions {
    /// Only sessions working in the repository holding this directory.
    #[serde(default)]
    pub repo: Option<PathBuf>,
    /// Only sessions working in this directory or below it.
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

pub(crate) async fn read_frame(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Vec<u8>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf().await.map_err(Error::storage)?;
        if available.is_empty() {
            return Err(Error::new(Code::Invalid, "the frame is incomplete"));
        }
        let end = available.iter().position(|byte| *byte == b'\n');
        let count = end.map_or(available.len(), |index| index + 1);
        if frame.len() + count > MAX_FRAME {
            return Err(Error::new(Code::Invalid, "the frame is too large"));
        }
        frame.extend_from_slice(&available[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(frame);
        }
    }
}

/// Asks the work host serving `root` as the session `credential` names.
pub async fn ask(
    root: &Path,
    credential: &str,
    command: Command,
    thread: Option<String>,
) -> Result<Value> {
    let stream = connect(&socket(root)).await.map_err(|error| {
        Error::new(
            Code::Storage,
            format!("cannot reach the work host: {error}"),
        )
    })?;
    let (read, mut write) = stream.into_split();
    let mut bytes = serde_json::to_vec(&Request {
        version: VERSION,
        credential: credential.to_owned(),
        command,
        thread,
    })
    .map_err(Error::storage)?;
    bytes.push(b'\n');
    write.write_all(&bytes).await.map_err(Error::storage)?;
    let frame = read_frame(&mut BufReader::new(read)).await?;
    let response: Response = serde_json::from_slice(&frame).map_err(Error::storage)?;
    response.result
}

/// How long a request waits for a work host to listen again, as one being
/// replaced is stopped and its successor started.
const REPLACING: std::time::Duration = std::time::Duration::from_secs(10);

/// Connect to `socket`, waiting while no work host listens there.
async fn connect(socket: &Path) -> std::io::Result<tokio::net::UnixStream> {
    let until = tokio::time::Instant::now() + REPLACING;
    loop {
        match tokio::net::UnixStream::connect(socket).await {
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) && tokio::time::Instant::now() < until =>
            {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            connected => return connected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_request_made_while_the_work_host_is_replaced_waits_for_the_new_one() {
        let dir = tempfile::Builder::new()
            .prefix("adroit-")
            .tempdir_in("/tmp")
            .unwrap();
        let root = dir.path().to_path_buf();
        let replacing = {
            let root = root.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                let listener = tokio::net::UnixListener::bind(socket(&root)).unwrap();
                let (stream, _) = listener.accept().await.unwrap();
                let (read, mut write) = stream.into_split();
                read_frame(&mut BufReader::new(read)).await.unwrap();
                let mut bytes = serde_json::to_vec(&Response {
                    version: VERSION,
                    result: Ok(Value::from("answered")),
                })
                .unwrap();
                bytes.push(b'\n');
                write.write_all(&bytes).await.unwrap();
            })
        };
        let answer = ask(&root, "credential", Command::Context, None).await;
        assert_eq!(answer.unwrap(), Value::from("answered"));
        replacing.await.unwrap();
    }
}
