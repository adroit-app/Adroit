//! What a session asks the work host, one JSON line each way.
use std::path::{Path, PathBuf};

use adroit_model::{Agent, AgentPermissions, MessageId, SessionCode, SessionId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use super::{Code, Error, Result};

pub const VERSION: u32 = 1;
pub const MAX_FRAME: usize = 1024 * 1024;

#[cfg(unix)]
pub fn socket(root: &Path) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    let path = root.join("delegation.sock");
    if path.as_os_str().as_bytes().len() < 104 {
        return path;
    }
    // Only the endpoint is shortened; credentials and records stay in their private root.
    let mut hash = 0x6c62272e07bb014262b821756295c58du128;
    for byte in root.as_os_str().as_bytes() {
        hash = (hash ^ u128::from(*byte)).wrapping_mul(0x1000000000000000000013b);
    }
    let uid = adroit_sys::euid();
    PathBuf::from(format!("/tmp/adroit-{uid}/d{hash:032x}.sock"))
}

/// The socket in `root`, whose path no length limit holds to here.
#[cfg(windows)]
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
    /// Ask another session for work.
    Ask(Send),
    /// Propose to pass the user's own words on to other sessions.
    ProposeRelay(ProposeRelay),
    Sessions(Sessions),
    Models,
    /// Name the caller's own session anew.
    Rename(Rename),
    /// End the caller's own session, once its user agreed.
    Close,
    /// What the caller's agent is to weigh as a prompt arrives, or once its
    /// conversation was cleared. Still read under its former name, which
    /// hooks of an Adroit installed before send to a work host started since.
    #[serde(alias = "naming")]
    Prompted {
        cleared: bool,
    },
    /// Record changes to the caller's timeline.
    TimelineTasks(TimelineTasks),
    /// The caller's timeline as it stands.
    Timeline,
    /// The oldest message waiting for the caller, as its agent is to read it.
    Next,
    Delivered {
        id: MessageId,
    },
    /// What the caller's agent asks the user, held for as long as the one
    /// who tells of it stays connected.
    Asking(super::questions::Asking),
}

impl Command {
    /// Whether the command acts for the session rather than reading.
    pub fn acts(&self) -> bool {
        matches!(
            self,
            Self::StartSession(_)
                | Self::Send(_)
                | Self::Ask(_)
                | Self::ProposeRelay(_)
                | Self::Rename(_)
                | Self::Close
        )
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
    /// Keep it out of the user's sight: it is only listed, and no tab opens for it. Otherwise it opens in a tab, with the keys while the user is looking at you.
    #[serde(default)]
    pub background: bool,
}

/// Send a message to another session.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Send {
    /// The recipient's code, or its id, as `sessions` lists them.
    pub to: String,
    pub body: String,
}

/// Propose to pass the user's own words on to other sessions.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProposeRelay {
    /// The recipients' codes, or their ids, as `sessions` lists them.
    pub to: Vec<String>,
    /// What the user wants said, written to stand on its own for a session that knows nothing of this conversation.
    pub body: String,
}

/// Name your own session anew.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rename {
    /// A few words saying what this session works on, in the language the user writes in.
    pub name: String,
}

/// Change the tasks of your timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineTasks {
    /// Applied in the order given.
    pub ops: Vec<adroit_model::timeline::Op>,
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

/// A session that opened another, as the one it opened is told of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Opener {
    pub id: SessionId,
    pub code: SessionCode,
    pub name: String,
}

impl Opener {
    pub(crate) fn of(session: &super::Session) -> Self {
        Self {
            id: session.id,
            code: session.code.clone(),
            name: session.name.clone(),
        }
    }
}

/// The session that opened the caller, and whether it has ended.

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parent {
    #[serde(flatten)]
    pub opener: Opener,
    pub ended: bool,
}

/// The answer to [`Command::Context`]: the caller's own session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Caller {
    pub id: SessionId,
    pub code: SessionCode,
    pub name: String,
    pub dir: PathBuf,
    pub opened_by: Option<Parent>,
}

/// The answer to [`Command::StartSession`]: the session started.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Started {
    pub id: SessionId,
    pub code: SessionCode,
    pub name: String,
    pub dir: PathBuf,
}

/// One session of the answer to [`Command::Sessions`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Listed {
    pub id: SessionId,
    pub code: SessionCode,
    pub name: String,
    pub agent: Agent,
    pub dir: PathBuf,
    /// The repository it works in, by its common directory.
    pub repository: Option<PathBuf>,
    pub readiness: super::Readiness,
    pub opened_by: Option<Opener>,
}

/// The answer to [`Command::Send`]: the message sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sent {
    pub sent: MessageId,
}

/// The answer to [`Command::Ask`]: the request sent, and whether its
/// recipient takes it up as work as things stand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asked {
    pub sent: MessageId,
    pub taken_up: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The answer to [`Command::ProposeRelay`]: what is proposed, and to whom,
/// which the user is yet to decide of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposed {
    pub proposed: MessageId,
    pub to: Vec<adroit_model::RelayTo>,
    pub note: String,
}

/// The answer to [`Command::Next`]: a message to hand the caller's agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Delivery {
    pub id: MessageId,
    pub from: SessionId,
    /// The program outside Adroit that sent it, by the name it gave itself,
    /// `from` then being no session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outside: Option<String>,
    /// The message as its recipient's agent reads it.
    pub content: String,
}

/// The answer to [`Command::Rename`]: the name taken.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Renamed {
    pub name: String,
}

/// The answer to [`Command::Close`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Closing {
    pub closing: bool,
}

/// The answer to [`Command::Prompted`]: what the caller's agent is told, if
/// anything.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub note: Option<String>,
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

pub(crate) async fn write_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    value: &impl Serialize,
) -> Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(Error::storage)?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await.map_err(Error::storage)
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
    let (read, mut write) = tokio::io::split(stream);
    write_frame(
        &mut write,
        &Request {
            version: VERSION,
            credential: credential.to_owned(),
            command,
            thread,
        },
    )
    .await?;
    let frame = read_frame(&mut BufReader::new(read))
        .await
        .map_err(|error| Error::new(Code::Storage, format!("the work host went away: {error}")))?;
    let response: Response = serde_json::from_slice(&frame).map_err(Error::storage)?;
    response.result
}

/// [`ask`], its answer read as `T`.
pub async fn ask_for<T: serde::de::DeserializeOwned>(
    root: &Path,
    credential: &str,
    command: Command,
    thread: Option<String>,
) -> Result<T> {
    let answer = ask(root, credential, command, thread).await?;
    serde_json::from_value(answer).map_err(|error| {
        Error::new(
            Code::Storage,
            format!("the work host's answer is not understood: {error}"),
        )
    })
}

/// How long a request waits for a work host to listen again, as one being
/// replaced is stopped and its successor started.
const REPLACING: std::time::Duration = std::time::Duration::from_secs(10);

/// Connect to `socket`, waiting while no work host listens there.
async fn connect(socket: &Path) -> std::io::Result<adroit_sys::local::Stream> {
    let until = tokio::time::Instant::now() + REPLACING;
    loop {
        match adroit_sys::local::connect(socket).await {
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
    use serde_json::json;

    fn wire(answer: impl Serialize) -> Value {
        serde_json::to_value(answer).unwrap()
    }

    fn opener() -> Opener {
        Opener {
            id: "6efede4a-8866-4d1a-9431-07c591be83ba".parse().unwrap(),
            code: "57u7".parse().unwrap(),
            name: "lead".into(),
        }
    }

    #[test]
    fn each_answer_reads_on_the_wire_as_agents_were_told_it_does() {
        let id: adroit_model::SessionId = "e0131a25-f7be-40d6-838e-946265fbada4".parse().unwrap();
        let code: adroit_model::SessionCode = "ku8c".parse().unwrap();
        assert_eq!(
            wire(Caller {
                id,
                code: code.clone(),
                name: "review".into(),
                dir: "/repo".into(),
                opened_by: Some(Parent {
                    opener: opener(),
                    ended: false
                }),
            }),
            json!({"id": id, "code": "ku8c", "name": "review", "dir": "/repo",
                "opened_by": {"id": opener().id, "code": "57u7", "name": "lead", "ended": false}})
        );
        assert_eq!(
            wire(Started {
                id,
                code: code.clone(),
                name: "review".into(),
                dir: "/repo".into(),
            }),
            json!({"id": id, "code": "ku8c", "name": "review", "dir": "/repo"})
        );
        assert_eq!(
            wire(Listed {
                id,
                code,
                name: "review".into(),
                agent: Agent::Codex,
                dir: "/repo".into(),
                repository: None,
                readiness: super::super::Readiness::Ready,
                opened_by: Some(opener()),
            }),
            json!({"id": id, "code": "ku8c", "name": "review", "agent": "codex",
                "dir": "/repo", "repository": null, "readiness": "ready",
                "opened_by": {"id": opener().id, "code": "57u7", "name": "lead"}})
        );
        let message = MessageId::new();
        assert_eq!(wire(Sent { sent: message }), json!({"sent": message}));
        assert_eq!(
            wire(Delivery {
                id: message,
                from: id,
                outside: None,
                content: "hi".into()
            }),
            json!({"id": message, "from": id, "content": "hi"})
        );
        assert_eq!(
            wire(Renamed {
                name: "review-2".into()
            }),
            json!({"name": "review-2"})
        );
        assert_eq!(wire(Closing { closing: true }), json!({"closing": true}));
        assert_eq!(wire(Note { note: None }), json!({"note": null}));
    }

    #[tokio::test]
    async fn a_frame_written_is_read_back_as_one_line() {
        let mut written = Vec::new();

        write_frame(&mut written, &Value::from("hello"))
            .await
            .unwrap();

        assert_eq!(written, b"\"hello\"\n");
        let read = read_frame(&mut written.as_slice()).await.unwrap();
        assert_eq!(read, written);
    }

    #[tokio::test]
    async fn a_request_made_while_the_work_host_is_replaced_waits_for_the_new_one() {
        let dir = tempfile::Builder::new()
            .prefix("adroit-")
            .tempdir_in(adroit_sys::local::temp_dir())
            .unwrap();
        let root = dir.path().to_path_buf();
        let replacing = {
            let root = root.clone();
            crate::environment::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                let listener = adroit_sys::local::Listener::bind(socket(&root)).unwrap();
                let stream = listener.accept().await.unwrap();
                let (read, mut write) = tokio::io::split(stream);
                read_frame(&mut BufReader::new(read)).await.unwrap();
                let answer = Response {
                    version: VERSION,
                    result: Ok(Value::from("answered")),
                };
                write_frame(&mut write, &answer).await.unwrap();
            })
        };
        let answer = ask(&root, "credential", Command::Context, None).await;
        assert_eq!(answer.unwrap(), Value::from("answered"));
        replacing.await.unwrap();
    }

    #[test]
    fn a_start_opens_in_the_window_unless_asked_to_stay_in_the_background() {
        let start = |value: Value| serde_json::from_value::<Start>(value);
        assert!(
            !start(serde_json::json!({"prompt": "go"}))
                .unwrap()
                .background
        );
        let background = start(serde_json::json!({"prompt": "go", "background": true}));
        assert!(background.unwrap().background);
        assert!(start(serde_json::json!({"prompt": "go", "show": true})).is_err());
    }
}

#[cfg(all(test, unix))]
mod environment_socket_tests {
    use super::*;
    #[test]
    fn long_profile_roots_have_short_distinct_sockets() {
        use std::os::unix::ffi::OsStrExt;
        let root = PathBuf::from(format!(
            "/Users/person/.local/state/adroit/environments/{}/state/delegation",
            "a".repeat(32)
        ));
        assert!(socket(&root).as_os_str().as_bytes().len() < 104);
        assert_ne!(socket(&root), socket(&root.with_file_name("other")));
        assert_eq!(
            socket(Path::new("/tmp/short")),
            Path::new("/tmp/short/delegation.sock")
        );
    }
}
