//! `adroit mcp`: the `adroit` MCP server every session Adroit starts carries,
//! with Claude Code's channel notifications when asked for.
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    sync::Mutex,
};

use super::{
    Code, Error, Result,
    launch::{ROOT_ENV, TOKEN_ENV},
    protocol::{self, Command},
};

/// How the bridge reaches the work host; a test answers in its place.
pub trait Daemon: Clone + Send + Sync + 'static {
    fn ask(
        &self,
        command: Command,
        thread: Option<String>,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
}

#[derive(Clone)]
pub struct Socket {
    root: PathBuf,
    credential: String,
}

impl Socket {
    pub fn new(root: impl Into<PathBuf>, credential: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            credential: credential.into(),
        }
    }
}

impl Daemon for Socket {
    async fn ask(&self, command: Command, thread: Option<String>) -> Result<Value> {
        protocol::ask(&self.root, &self.credential, command, thread).await
    }
}

type Output<W> = Arc<Mutex<W>>;

struct Background(tokio::task::JoinHandle<Result<()>>);

impl Drop for Background {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The tools every session has, whoever started it.
fn tools() -> Vec<Value> {
    use super::protocol::{Rename, Send, Sessions, Start};
    let tool = |name: &str, description: &str, schema: schemars::Schema| json!({"name": name, "description": description, "inputSchema": schema});
    vec![
        tool(
            "start_session",
            "Open a session beside you for a line of work the user will want to steer themselves — one that needs their decisions or review, lands as a change of its own, or lives in another repository — even when it came out of your own work, or when the user asks you to. What comes back to you to fold into your answer belongs to your own subagents instead. It answers to the user, not to you, in a console of its own; give it a first prompt that carries what it needs.",
            schemars::schema_for!(Start),
        ),
        tool(
            "send",
            "Send a message to another session by its code: what it should know, or a question about what it knows; never work for it to do. It reaches that session between its turns and costs it a turn: send only what it has to act on or answer, and plan nothing on a reply coming.",
            schemars::schema_for!(Send),
        ),
        tool(
            "sessions",
            "List the sessions Adroit runs, each with the code it keeps, where it works and which session opened it.",
            schemars::schema_for!(Sessions),
        ),
        tool(
            "rename_session",
            "Name your own session anew, as the user sees it in Adroit: a few words saying what it works on, in the language the user writes in. Call it once you have answered the first prompt, after the conversation is cleared once you have answered the next one, and otherwise only when the work turns to something else entirely, never as it moves from step to step. When it is refused, keep the name.",
            schemars::schema_for!(Rename),
        ),
        json!({
            "name": "close_session",
            "description": "End your own session, as the user can from Adroit; its conversation stays and can be resumed. Call it only once the user has agreed to close it, after you showed them a summary of where things stand, and as the last thing you do: the session ends a moment after this answers.",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
        }),
        json!({
            "name": "timeline_tasks",
            "description": TIMELINE_TASKS,
            "inputSchema": timeline_schema(),
        }),
        json!({
            "name": "timeline",
            "description": "Read your timeline as it stands: every task tree with its tasks' statuses, weights and notes, and each tree's progress.",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
        }),
        json!({
            "name": "models",
            "description": "List the profiles a session can start from, each with its agent, the folder of its own its login is kept in, if any, and the models it accepts, and which profile a session starts from by default.",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
        }),
    ]
}

const TIMELINE_TASKS: &str = "Record the plan of a goal that takes several steps as a task tree on your timeline, which the user follows in Adroit, and keep it current as you work. Make one top-level task per goal, titled with the goal, and put its steps under it as subtasks, split further where a step is large. Give each task a weight: its size beside its siblings. Mark a task in_progress as you start it, and completed or blocked as soon as it is; put what it will do in description, and what happened, its result or why it is blocked, in note. Use it in place of a todo list. ops are applied in order: add {id, parent?, title, weight?, status?, description?, note?}, update {id, status?, title?, weight?, description?, note?}, remove {id}.";

/// What `timeline_tasks` takes, written out flat: some agents read only a
/// part of JSON Schema, which the schema of a tagged enum steps outside.
fn timeline_schema() -> Value {
    let text = json!({"type": "string"});
    json!({
        "type": "object",
        "properties": {
            "ops": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "op": {"type": "string", "enum": ["add", "update", "remove"]},
                        "id": {"type": "string", "description": "A short id you choose, unique on your timeline."},
                        "parent": {"type": "string", "description": "add: the task it is a step of; a new tree when omitted."},
                        "title": {"type": "string", "description": "Required to add."},
                        "status": {"type": "string", "enum": ["pending", "in_progress", "completed", "blocked"]},
                        "weight": {"type": "number", "description": "Its size beside its siblings, zero or more; 1 when omitted."},
                        "description": text,
                        "note": text,
                    },
                    "required": ["op", "id"],
                },
            },
        },
        "required": ["ops"],
    })
}

/// The protocol command a tool call asks for.
fn command(name: &str, arguments: Value) -> std::result::Result<Command, String> {
    let arguments = if arguments.is_null() {
        json!({})
    } else {
        arguments
    };
    let parsed = |error: serde_json::Error| format!("invalid arguments: {error}");
    Ok(match name {
        "start_session" => {
            Command::StartSession(serde_json::from_value(arguments).map_err(parsed)?)
        }
        "send" => Command::Send(serde_json::from_value(arguments).map_err(parsed)?),
        "sessions" => Command::Sessions(serde_json::from_value(arguments).map_err(parsed)?),
        "models" => Command::Models,
        "rename_session" => Command::Rename(serde_json::from_value(arguments).map_err(parsed)?),
        "close_session" => Command::Close,
        "timeline_tasks" => {
            Command::TimelineTasks(serde_json::from_value(arguments).map_err(parsed)?)
        }
        "timeline" => Command::Timeline,
        _ => return Err(format!("unknown tool {name}")),
    })
}

pub async fn run<R, W>(mut input: R, output: W, daemon: impl Daemon, channel: bool) -> Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let output = Arc::new(Mutex::new(output));
    // Asked first so a session the work host does not know is told at once.
    daemon.ask(Command::Context, None).await?;
    let mut initialized = false;
    let mut background: Option<Background> = None;
    loop {
        let bytes = tokio::select! {
            frame = async {
                if input.fill_buf().await.map_err(Error::storage)?.is_empty() {
                    Ok(None)
                } else {
                    protocol::read_frame(&mut input).await.map(Some)
                }
            } => frame?,
            stopped = async { (&mut background.as_mut().expect("a running channel").0).await }, if background.is_some() => {
                return match stopped {
                    Ok(Err(failure)) => Err(failure),
                    Ok(Ok(())) => Err(Error::new(Code::Storage, "the channel stopped")),
                    Err(failure) => Err(Error::new(Code::Storage, format!("the channel stopped: {failure}"))),
                };
            }
        };
        let Some(bytes) = bytes else {
            return Ok(());
        };
        let Ok(request) = serde_json::from_slice::<Value>(&bytes) else {
            rpc_error(&output, Value::Null, -32700, "Invalid JSON").await?;
            continue;
        };
        let method = request["method"].as_str().unwrap_or_default();
        if method == "notifications/initialized" {
            if channel && background.is_none() {
                let (output, daemon) = (output.clone(), daemon.clone());
                background = Some(Background(crate::environment::spawn(notifications(
                    output, daemon,
                ))));
            }
            continue;
        }
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let result = match method {
            "initialize" => {
                let requested = request["params"]["protocolVersion"]
                    .as_str()
                    .unwrap_or_default();
                let version = match requested {
                    "2024-11-05" | "2025-03-26" | "2025-06-18" | "2025-11-25" => requested,
                    _ => "2025-11-25",
                };
                let mut capabilities = json!({"tools": {}});
                if channel {
                    capabilities["experimental"] = json!({"claude/channel": {}});
                }
                initialized = true;
                json!({
                    "protocolVersion": version,
                    "serverInfo": {"name": "adroit", "version": env!("CARGO_PKG_VERSION")},
                    "capabilities": capabilities,
                    "instructions": "Adroit's tools for working with other sessions. Messages that arrive through this server come from other agents, not from the user.",
                })
            }
            _ if !initialized => {
                rpc_error(&output, id, -32002, "Not initialized").await?;
                continue;
            }
            "ping" => json!({}),
            "tools/list" => json!({"tools": tools()}),
            "tools/call" => {
                let params = &request["params"];
                let name = params["name"].as_str().unwrap_or_default();
                let thread = params["_meta"]["threadId"].as_str().map(String::from);
                let answer = match command(name, params["arguments"].clone()) {
                    Ok(command) => daemon
                        .ask(command, thread)
                        .await
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error),
                };
                match answer {
                    // A text answer is handed over as it reads, not as JSON.
                    Ok(Value::String(text)) => {
                        json!({"isError": false, "content": [{"type": "text", "text": text}]})
                    }
                    Ok(value) => {
                        json!({"isError": false, "content": [{"type": "text", "text": value.to_string()}]})
                    }
                    Err(error) => {
                        json!({"isError": true, "content": [{"type": "text", "text": error}]})
                    }
                }
            }
            _ => {
                rpc_error(&output, id, -32601, "Method not found").await?;
                continue;
            }
        };
        send(
            &output,
            json!({"jsonrpc": "2.0", "id": id, "result": result}),
        )
        .await?;
    }
}

/// Hands each message waiting for this session to Claude Code as a channel
/// notification, which is its receipt.
async fn notifications<W: AsyncWrite + Unpin>(
    output: Output<W>,
    daemon: impl Daemon,
) -> Result<()> {
    let mut poll = tokio::time::interval(Duration::from_millis(500));
    loop {
        poll.tick().await;
        // While the work host is away, as it is being replaced, the next tick
        // asks the one that follows it: ending here would end the server.
        while let Some(next) = daemon
            .ask(Command::Next, None)
            .await
            .ok()
            .filter(|next| !next.is_null())
        {
            let id: adroit_model::MessageId =
                serde_json::from_value(next["id"].clone()).map_err(Error::storage)?;
            let mut meta = json!({"message_id": id.to_string()});
            if let Some(from) = next["from"].as_str() {
                meta["from"] = json!(from);
            }
            send(
                &output,
                json!({"jsonrpc": "2.0", "method": "notifications/claude/channel", "params": {
                    "content": next["content"], "meta": meta,
                }}),
            )
            .await?;
            // Left unmarked, it is handed over again once the work host is back.
            if daemon.ask(Command::Delivered { id }, None).await.is_err() {
                break;
            }
        }
    }
}

async fn send<W: AsyncWrite + Unpin>(output: &Output<W>, value: Value) -> Result<()> {
    let mut output = output.lock().await;
    super::protocol::write_frame(&mut *output, &value).await?;
    output.flush().await.map_err(Error::storage)
}

async fn rpc_error<W: AsyncWrite + Unpin>(
    output: &Output<W>,
    id: Value,
    code: i32,
    message: &str,
) -> Result<()> {
    send(
        output,
        json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
    )
    .await
}

/// What a Claude Code hook `event` is answered with: what the work host
/// says of the session's name, as a prompt arrives or once the conversation
/// was cleared; nothing for any other event, or while the work host cannot
/// say.
async fn hook(event: &Value, daemon: impl Daemon) -> Option<Value> {
    let name = event["hook_event_name"].as_str()?;
    let cleared = match name {
        "UserPromptSubmit" => false,
        "SessionStart" if event["source"] == "clear" => true,
        _ => return None,
    };
    let answer = daemon.ask(Command::Naming { cleared }, None).await.ok()?;
    let note = answer["note"].as_str()?;
    Some(json!({"hookSpecificOutput": {"hookEventName": name, "additionalContext": note}}))
}

/// `adroit mcp hook`: answers the hook event on standard input. It always
/// succeeds, adding nothing when it cannot answer, as a failing hook would
/// hold up the user's prompt.
fn hook_cli() -> i32 {
    use std::io::Read;
    let mut input = String::new();
    let event: Option<Value> = std::io::stdin()
        .read_to_string(&mut input)
        .ok()
        .and_then(|_| serde_json::from_str(&input).ok());
    let (Some(event), Ok(credential), Ok(root)) =
        (event, std::env::var(TOKEN_ENV), std::env::var(ROOT_ENV))
    else {
        return 0;
    };
    let answer = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()
        .and_then(|runtime| {
            runtime.block_on(hook(&event, Socket::new(Path::new(&root), credential)))
        });
    if let Some(answer) = answer {
        println!("{answer}");
    }
    0
}

/// `adroit mcp [--channel | hook]`. Returns the process exit code.
pub fn cli(args: &[String]) -> i32 {
    let channel = match args {
        [] => false,
        [flag] if flag == "--channel" => true,
        [verb] if verb == "hook" => return hook_cli(),
        _ => {
            eprintln!("usage: adroit mcp [--channel | hook]");
            return 2;
        }
    };
    let (Ok(credential), Ok(root)) = (std::env::var(TOKEN_ENV), std::env::var(ROOT_ENV)) else {
        eprintln!("adroit: the MCP server runs only in a session Adroit started");
        return 1;
    };
    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(Error::storage)
        .and_then(|runtime| {
            runtime.block_on(run(
                BufReader::new(tokio::io::stdin()),
                tokio::io::stdout(),
                Socket::new(Path::new(&root), credential),
                channel,
            ))
        });
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("adroit: {error}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as Plain;

    #[derive(Clone)]
    struct Fake {
        asked: Arc<Plain<Vec<String>>>,
        waiting: Arc<Plain<Vec<Value>>>,
        /// How many asks fail more, as while the work host is being replaced.
        away: Arc<Plain<usize>>,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                asked: Arc::default(),
                waiting: Arc::default(),
                away: Arc::default(),
            }
        }
    }

    impl Daemon for Fake {
        async fn ask(&self, command: Command, _thread: Option<String>) -> Result<Value> {
            let op = serde_json::to_value(&command).unwrap()["op"]
                .as_str()
                .unwrap()
                .to_owned();
            self.asked.lock().unwrap().push(op);
            {
                let mut away = self.away.lock().unwrap();
                if *away > 0 {
                    *away -= 1;
                    return Err(Error::new(Code::Storage, "cannot reach the work host"));
                }
            }
            Ok(match command {
                Command::Context => json!({"opened_by": null}),
                Command::Next => self.waiting.lock().unwrap().pop().unwrap_or(Value::Null),
                Command::Send(send) => json!({"sent_to": send.to}),
                Command::Naming { cleared } => json!({"note": format!("cleared={cleared}")}),
                Command::Timeline => json!("(no tasks)\nline two"),
                _ => Value::Null,
            })
        }
    }

    fn added(answer: Option<Value>) -> Option<(String, String)> {
        let output = &answer?["hookSpecificOutput"];
        Some((
            output["hookEventName"].as_str()?.to_owned(),
            output["additionalContext"].as_str()?.to_owned(),
        ))
    }

    #[tokio::test]
    async fn a_prompt_is_given_what_adroit_says_of_the_session_s_name() {
        let answer = hook(
            &json!({"hook_event_name": "UserPromptSubmit", "prompt": "hi"}),
            Fake::new(),
        )
        .await;
        assert_eq!(
            added(answer),
            Some(("UserPromptSubmit".into(), "cleared=false".into()))
        );
    }

    #[tokio::test]
    async fn a_cleared_conversation_is_told_its_session_s_name_is_of_the_work_before() {
        let answer = hook(
            &json!({"hook_event_name": "SessionStart", "source": "clear"}),
            Fake::new(),
        )
        .await;
        assert_eq!(
            added(answer),
            Some(("SessionStart".into(), "cleared=true".into()))
        );
    }

    #[tokio::test]
    async fn a_conversation_started_or_resumed_otherwise_is_told_nothing() {
        for source in ["startup", "resume", "compact"] {
            let daemon = Fake::new();
            let event = json!({"hook_event_name": "SessionStart", "source": source});
            assert_eq!(hook(&event, daemon.clone()).await, None, "{source}");
            assert!(daemon.asked.lock().unwrap().is_empty(), "{source}");
        }
    }

    #[tokio::test]
    async fn nothing_is_added_while_the_work_host_cannot_be_reached() {
        let daemon = Fake::new();
        *daemon.away.lock().unwrap() = 1;
        let event = json!({"hook_event_name": "UserPromptSubmit", "prompt": "hi"});
        assert_eq!(hook(&event, daemon).await, None);
    }

    async fn exchange(daemon: Fake, channel: bool, lines: &[Value], wait: Duration) -> Vec<Value> {
        let input: Vec<u8> = lines
            .iter()
            .flat_map(|line| format!("{line}\n").into_bytes())
            .collect();
        // Standard input stays open while the agent runs; ending it ends the server.
        let (mut stdin, served) = tokio::io::duplex(1 << 20);
        stdin.write_all(&input).await.unwrap();
        let (client, server) = tokio::io::duplex(1 << 20);
        let (_, write) = tokio::io::split(server);
        let running =
            crate::environment::spawn(run(BufReader::new(served), write, daemon, channel));
        tokio::time::sleep(wait).await;
        running.abort();
        drop(stdin);
        let mut read = BufReader::new(client);
        let mut out = Vec::new();
        let mut line = String::new();
        while tokio::time::timeout(Duration::from_millis(50), read.read_line(&mut line))
            .await
            .is_ok_and(|n| n.is_ok_and(|n| n > 0))
        {
            out.push(serde_json::from_str(&line).unwrap());
            line.clear();
        }
        out
    }

    fn rpc(id: u64, method: &str, params: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
    }

    fn names(listed: &Value) -> Vec<String> {
        listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect()
    }

    #[tokio::test]
    async fn every_session_has_the_same_tools_whoever_started_it() {
        let out = exchange(
            Fake::new(),
            false,
            &[
                rpc(1, "initialize", json!({})),
                rpc(2, "tools/list", json!({})),
            ],
            Duration::from_millis(100),
        )
        .await;
        assert_eq!(
            names(&out[1]),
            [
                "start_session",
                "send",
                "sessions",
                "rename_session",
                "close_session",
                "timeline_tasks",
                "timeline",
                "models"
            ]
        );
    }

    #[tokio::test]
    async fn closing_asks_the_daemon_to_close_the_caller_s_own_session() {
        let daemon = Fake::new();
        let out = exchange(
            daemon.clone(),
            false,
            &[
                rpc(1, "initialize", json!({})),
                rpc(2, "tools/call", json!({"name": "close_session"})),
            ],
            Duration::from_millis(100),
        )
        .await;
        assert_eq!(out[1]["result"]["isError"], false);
        assert_eq!(*daemon.asked.lock().unwrap(), ["context", "close"]);
    }

    #[tokio::test]
    async fn a_tool_adroit_does_not_offer_is_refused_without_asking_the_daemon() {
        let daemon = Fake::new();
        let out = exchange(
            daemon.clone(),
            false,
            &[
                rpc(1, "initialize", json!({})),
                rpc(
                    2,
                    "tools/call",
                    json!({"name": "report", "arguments": {"body": "done"}}),
                ),
            ],
            Duration::from_millis(100),
        )
        .await;
        assert_eq!(out[1]["result"]["isError"], true);
        assert_eq!(*daemon.asked.lock().unwrap(), ["context"]);
    }

    #[tokio::test]
    async fn a_tool_call_is_answered_with_what_the_daemon_says() {
        let to = adroit_model::SessionId::new();
        let out = exchange(
            Fake::new(),
            false,
            &[
                rpc(1, "initialize", json!({})),
                rpc(
                    2,
                    "tools/call",
                    json!({"name": "send", "arguments": {"to": to, "body": "hi"}}),
                ),
            ],
            Duration::from_millis(100),
        )
        .await;
        assert_eq!(out[1]["result"]["isError"], false);
        assert!(
            out[1]["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains(&to.to_string())
        );
    }

    #[tokio::test]
    async fn a_text_answer_reaches_the_agent_as_it_reads() {
        let out = exchange(
            Fake::new(),
            false,
            &[
                rpc(1, "initialize", json!({})),
                rpc(
                    2,
                    "tools/call",
                    json!({"name": "timeline", "arguments": {}}),
                ),
            ],
            Duration::from_millis(100),
        )
        .await;
        assert_eq!(
            out[1]["result"]["content"][0]["text"],
            "(no tasks)\nline two"
        );
    }

    #[test]
    fn timeline_operations_are_read_into_the_command_the_work_host_makes() {
        let asked = command(
            "timeline_tasks",
            json!({"ops": [{"op": "add", "id": "a", "title": "A", "weight": 2}, {"op": "remove", "id": "b"}]}),
        )
        .unwrap();
        let Command::TimelineTasks(asked) = asked else {
            panic!("{asked:?}");
        };
        assert_eq!(asked.ops.len(), 2);
        let refused = command(
            "timeline_tasks",
            json!({"ops": [{"op": "update", "id": "a", "parent": "b"}]}),
        )
        .unwrap_err();
        assert!(refused.contains("parent"), "{refused}");
    }

    #[tokio::test]
    async fn a_waiting_message_reaches_claude_code_through_its_channel_and_is_marked_delivered() {
        let daemon = Fake::new();
        let id = adroit_model::MessageId::new();
        daemon
            .waiting
            .lock()
            .unwrap()
            .push(json!({"id": id, "from": null, "content": "[adroit] ready"}));
        let out = exchange(
            daemon.clone(),
            true,
            &[
                rpc(1, "initialize", json!({})),
                json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            ],
            Duration::from_millis(300),
        )
        .await;
        assert_eq!(
            out[0]["result"]["capabilities"]["experimental"]["claude/channel"],
            json!({})
        );
        let notice = out
            .iter()
            .find(|line| line["method"] == "notifications/claude/channel")
            .unwrap();
        assert_eq!(notice["params"]["content"], "[adroit] ready");
        assert!(
            daemon
                .asked
                .lock()
                .unwrap()
                .contains(&"delivered".to_owned())
        );
    }

    #[tokio::test]
    async fn a_message_still_reaches_claude_code_after_the_work_host_was_away_for_a_while() {
        let daemon = Fake::new();
        let lines = [
            rpc(1, "initialize", json!({})),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        ];
        let going_away = {
            let (away, waiting) = (daemon.away.clone(), daemon.waiting.clone());
            async move {
                // Away only once the server knows who it is.
                tokio::time::sleep(Duration::from_millis(50)).await;
                *away.lock().unwrap() = 2;
                waiting.lock().unwrap().push(
                    json!({"id": adroit_model::MessageId::new(), "from": null, "content": "[adroit] back"}),
                );
            }
        };
        let (out, ()) = tokio::join!(
            exchange(daemon.clone(), true, &lines, Duration::from_millis(1700)),
            going_away,
        );
        let notice = out
            .iter()
            .find(|line| line["method"] == "notifications/claude/channel");
        assert_eq!(
            notice.map(|n| n["params"]["content"].clone()),
            Some(json!("[adroit] back"))
        );
    }

    #[tokio::test]
    async fn without_the_channel_messages_are_left_for_codex_s_queue() {
        let daemon = Fake::new();
        daemon
            .waiting
            .lock()
            .unwrap()
            .push(json!({"id": adroit_model::MessageId::new(), "content": "x"}));
        exchange(
            daemon.clone(),
            false,
            &[
                rpc(1, "initialize", json!({})),
                json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            ],
            Duration::from_millis(300),
        )
        .await;
        assert!(!daemon.asked.lock().unwrap().contains(&"next".to_owned()));
    }
}
