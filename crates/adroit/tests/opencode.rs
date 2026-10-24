//! OpenCode as adroit starts it: refused when it is of a line adroit cannot
//! drive, and, as it runs in a console of adroit's, against a model of the
//! test's own that answers every prompt alike, in an account of its own: no
//! provider's credentials are needed or reached.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use adroit_host::delegation::launch::Delegation;
use adroit_host::execution::{StartRequest, resolve};
use adroit_host::settings::Settings;
use adroit_model::{Agent, State};
use adroit_term::{Daemon, Input, Key, Spawn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const REPLY: &str = "Hello from the fake model.";

/// The adroit sessions are started through.
const ADROIT: &str = env!("CARGO_BIN_EXE_adroit");

/// Settings with an OpenCode profile with a login of its own in `home`,
/// running `opencode` with the user's home at `user`, its model `fake/slow`.
fn settings(home: &Path, user: &Path, opencode: &str) -> Settings {
    Settings::parse(&format!(
        "profile = \"oc-fake\"\n\n[[profiles]]\nid = \"oc-fake\"\nagent = \"opencode\"\nhome = {}\n\n[profiles.launch]\ncommand = [{}]\n\n[profiles.launch.env]\nHOME = {}\nOPENCODE_DISABLE_AUTOUPDATE = \"1\"\nOPENCODE_DISABLE_MODELS_FETCH = \"1\"\n\n[profiles.opencode]\nmodel = \"fake/slow\"\n",
        serde_json::to_string(&home.display().to_string()).unwrap(),
        serde_json::to_string(opencode).unwrap(),
        serde_json::to_string(&user.display().to_string()).unwrap()
    ))
    .unwrap()
}

fn request(cwd: &Path, prompt: &str, resume: Option<String>) -> StartRequest {
    StartRequest {
        agent: Agent::OpenCode,
        name: "greet".into(),
        cwd: cwd.into(),
        prompt: prompt.into(),
        resume,
        ..Default::default()
    }
}

#[tokio::test]
async fn an_opencode_of_the_first_line_is_refused_before_anything_is_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let opencode = dir.path().join("opencode");
    std::fs::write(
        &opencode,
        "#!/bin/sh\n[ \"$1\" = --version ] && { echo 1.19.4; exit 0; }\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&opencode, std::fs::Permissions::from_mode(0o755)).unwrap();
    let work = dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let settings = settings(
        &dir.path().join("account"),
        dir.path(),
        &opencode.display().to_string(),
    );
    let root = dir.path().join("delegation");
    let Err(refused) = adroit_host::delegation::app::prepare(
        &root,
        &settings,
        request(&work, "go", None),
        None,
        Path::new(ADROIT),
    )
    .await
    else {
        panic!("started");
    };
    assert!(refused.contains("OpenCode 1.19.4"), "{refused}");
    assert!(refused.contains("npm i -g @opencode/cli"), "{refused}");
    let recorded = adroit_host::delegation::app::sessions(&root)
        .await
        .unwrap_or_default();
    assert!(recorded.is_empty(), "{recorded:?}");
}

async fn daemon() -> (Daemon, tempfile::TempDir) {
    let dir = tempfile::Builder::new()
        .prefix("adroit-opencode-")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.path().join("daemon.sock");
    let serving = socket.clone();
    tokio::spawn(async move {
        Daemon::serve(&serving, Duration::from_millis(20), |_| {})
            .await
            .unwrap()
    });
    let daemon = Daemon::new(socket);
    for _ in 0..100 {
        if daemon.sessions().await.is_ok() {
            return (daemon, dir);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the test daemon did not start");
}

/// An OpenAI-compatible endpoint that streams [`REPLY`] to every request:
/// its port.
async fn model() -> u16 {
    model_calling(None).await
}

/// An OpenAI-compatible endpoint that answers a request with no tool's
/// result in it by calling `tool`, a tool's name and its arguments, when one
/// is given, and streams [`REPLY`] to every other: its port.
async fn model_calling(tool: Option<(&'static str, serde_json::Value)>) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let tool = tool.clone();
            tokio::spawn(async move {
                let mut read = Vec::new();
                let mut buffer = [0; 8192];
                // The request is read whole before the answer starts.
                loop {
                    let Ok(n) = stream.read(&mut buffer).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    read.extend_from_slice(&buffer[..n]);
                    let text = String::from_utf8_lossy(&read);
                    if let Some(end) = text.find("\r\n\r\n") {
                        let length = text[..end]
                            .lines()
                            .find_map(|l| {
                                l.to_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if read.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let chunk = |delta: serde_json::Value, finish: Option<&str>| {
                    format!(
                        "data: {}\n\n",
                        serde_json::json!({"id":"x","object":"chat.completion.chunk","created":0,"model":"slow",
                            "choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
                    )
                };
                let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(head.as_bytes()).await;
                let answered = String::from_utf8_lossy(&read).contains(r#""role":"tool""#);
                if let Some((name, arguments)) = tool.filter(|_| !answered) {
                    let call = serde_json::json!({"tool_calls":[{"index":0, "id":"call_1", "type":"function",
                        "function":{"name":name, "arguments":arguments.to_string()}}]});
                    let end = chunk(call, None)
                        + &chunk(serde_json::json!({}), Some("tool_calls"))
                        + "data: [DONE]\n\n";
                    let _ = stream.write_all(end.as_bytes()).await;
                    let _ = stream.shutdown().await;
                    return;
                }
                // Word by word, slowly enough for the turn to be seen running.
                for word in REPLY.split_inclusive(' ') {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    let _ = stream
                        .write_all(chunk(serde_json::json!({"content": word}), None).as_bytes())
                        .await;
                }
                let end = chunk(serde_json::json!({}), Some("stop")) + "data: [DONE]\n\n";
                let _ = stream.write_all(end.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    port
}

/// The account `home` with the provider of the endpoint on `port` in its
/// own configuration.
fn account(home: &Path, port: u16) {
    let config = home.join("config").join("opencode");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("opencode.json"),
        serde_json::json!({"providers":{"fake":{"name":"Fake",
            "package":"@opencode/ai/providers/openai-compatible",
            "settings":{"baseURL":format!("http://127.0.0.1:{port}/v1"),"apiKey":"dummy"},
            "models":{"slow":{"name":"Slow fake"}}}}})
        .to_string(),
    )
    .unwrap();
}

/// The OpenCode the real runs start: `$ADROIT_TEST_OPENCODE`, or the one on
/// the path.
fn opencode() -> String {
    std::env::var("ADROIT_TEST_OPENCODE").unwrap_or_else(|_| "opencode".into())
}

/// How the session `id` stands as OpenCode reports it.
async fn reported(daemon: &Daemon, id: u128) -> Option<State> {
    daemon
        .sessions()
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.id == id)
        .and_then(|s| s.status)
        .map(|s| State::reported(&s.state, s.kind.as_deref()))
}

/// Until `check` holds of how the session `id` stands and its screen, or what
/// they were when it gave up. It stands as OpenCode reports, else as its
/// screen shows, as adroit reads it.
async fn until(
    daemon: &Daemon,
    id: u128,
    check: impl Fn(Option<State>, &[String]) -> bool,
) -> (Option<State>, Vec<String>) {
    let mut last = (None, Vec::new());
    for _ in 0..1200 {
        let screen = daemon.visible(id).await.unwrap_or_default();
        let state = match reported(daemon, id).await {
            Some(state) => Some(state),
            None => adroit_agents::adapter(Agent::OpenCode).screen_state(&screen),
        };
        if check(state.clone(), &screen) {
            return (state, screen);
        }
        last = (state, screen);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("gave up at {:?}:\n{}", last.0, last.1.join("\n"));
}

/// The replies on `screen`, each on a line of its own; the conversation's
/// title, named after the first, is drawn beside more.
fn replies(screen: &[String]) -> usize {
    screen.iter().filter(|l| l.trim() == REPLY).count()
}

fn spawn(launch: Spawn) -> Spawn {
    Spawn {
        cols: 120,
        rows: 40,
        ..launch
    }
}

#[tokio::test]
#[ignore = "runs the OpenCode installed on the machine, or $ADROIT_TEST_OPENCODE"]
async fn opencode_starts_replies_settles_idle_and_resumes_its_conversation() {
    let (daemon, dir) = daemon().await;
    let port = model().await;
    let home = dir.path().join("oc-home");
    account(&home, port);
    let user = dir.path().join("user");
    let work = dir.path().join("work");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::create_dir_all(&work).unwrap();
    let settings = settings(&home, &user, &opencode());
    let adapter = adroit_agents::adapter(Agent::OpenCode);

    let launch = resolve(
        &settings,
        request(&work, "say hello", None),
        Path::new(ADROIT),
    )
    .unwrap();
    let conversation = launch.conversation.clone();
    assert!(conversation.starts_with("ses_"), "{conversation}");
    let id = daemon.spawn(spawn(launch.spawn)).await.unwrap();
    let (_, screen) = until(&daemon, id, |state, screen| {
        state == Some(State::Idle) && replies(screen) == 1
    })
    .await;
    assert_eq!(
        adapter.startup_readiness(&screen),
        Some(adroit_model::StartupReadiness::Ready),
        "{}",
        screen.join("\n")
    );

    // A prompt typed in runs a turn the screen shows working, then idle.
    daemon
        .input(
            id,
            &[Input::Text("again".into()), Input::Key(Key::new("Enter"))],
        )
        .await
        .unwrap();
    until(&daemon, id, |state, _| state == Some(State::Working)).await;
    assert_eq!(reported(&daemon, id).await, Some(State::Working));
    until(&daemon, id, |state, screen| {
        state == Some(State::Idle) && replies(screen) == 2
    })
    .await;

    let context = adroit_agents::AgentContext {
        adroit: ADROIT.into(),
        ..adroit_host::programs::starting(&settings, Some(Agent::OpenCode), None).unwrap()
    };
    let session = adroit_model::Session {
        agent: Agent::OpenCode,
        session_id: conversation.clone(),
        cwd: work.clone(),
        ..Default::default()
    };
    let mut reader = adapter
        .transcript(&context, dir.path(), &session)
        .expect("a transcript");
    // The reader goes on without waiting for the export it asks for.
    for _ in 0..600 {
        reader.read();
        if reader.stats().user_turns > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(reader.stats().user_turns, 2);
    assert_eq!(reader.stats().model.as_deref(), Some("slow"));
    let past = adapter.past(&context, &user, Some(&work));
    assert_eq!(
        past.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        [conversation.as_str()]
    );
    // Everything OpenCode keeps is the account's.
    assert!(home.join("data/opencode/opencode.db").is_file());
    assert!(home.join("tmp").is_dir());
    assert!(
        !user.join(".local").exists(),
        "{:?}",
        user.read_dir().unwrap().collect::<Vec<_>>()
    );
    daemon.end(id).await.unwrap();

    let launch = resolve(
        &settings,
        StartRequest {
            home: Some(home.clone()),
            ..request(&work, "", Some(conversation.clone()))
        },
        Path::new(ADROIT),
    )
    .unwrap();
    assert_eq!(launch.conversation, conversation);
    let id = daemon.spawn(spawn(launch.spawn)).await.unwrap();
    let (_, screen) = until(&daemon, id, |state, screen| {
        state == Some(State::Idle) && replies(screen) == 2
    })
    .await;
    assert!(
        screen.iter().any(|l| l.contains("again")),
        "{}",
        screen.join("\n")
    );
    daemon.end(id).await.unwrap();
    let past = adapter.past(&context, &user, Some(&work));
    assert_eq!(past.len(), 1, "the resume kept to its conversation");
}

/// An adroit whose MCP server answers with a `send` tool and says nothing
/// more, and whose channel hands the session one message once it is heard;
/// any other command is `@ADROIT@`'s.
const FAKE_ADROIT: &str = r#"#!/usr/bin/env node
if (process.argv[2] !== "mcp") {
    const { status } = require("node:child_process").spawnSync("@ADROIT@", process.argv.slice(2), { stdio: "inherit" });
    process.exit(status ?? 1);
}
const channel = process.argv[3] === "--channel";
const write = (message) => process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", ...message })}\n`);
require("node:readline").createInterface({ input: process.stdin }).on("line", (line) => {
    let message;
    try {
        message = JSON.parse(line);
    } catch {
        return;
    }
    if (channel) {
        if (message.id === 1) write({ id: 1, result: {} });
        if (message.method === "notifications/initialized") {
            setTimeout(() => write({ method: "notifications/claude/channel", params: { content: "@MESSAGE@" } }), 500);
        }
        return;
    }
    if (message.id === undefined) return;
    const result =
        message.method === "initialize"
            ? { protocolVersion: message.params.protocolVersion, capabilities: { tools: {} }, serverInfo: { name: "adroit", version: "0" } }
            : message.method === "tools/list"
              ? { tools: [{ name: "send", description: "Send a message", inputSchema: { type: "object", properties: {} } }] }
              : {};
    write({ id: message.id, result });
});
"#;

const MESSAGE: &str = "A message from another session.";

#[tokio::test]
#[ignore = "runs the OpenCode installed on the machine, or $ADROIT_TEST_OPENCODE"]
async fn a_message_from_another_session_reaches_opencode_after_its_turn() {
    let (daemon, dir) = daemon().await;
    let port = model().await;
    let home = dir.path().join("oc-home");
    account(&home, port);
    let user = dir.path().join("user");
    let work = dir.path().join("work");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::create_dir_all(&work).unwrap();
    let adroit: PathBuf = dir.path().join("adroit");
    std::fs::write(
        &adroit,
        FAKE_ADROIT
            .replace("@ADROIT@", ADROIT)
            .replace("@MESSAGE@", MESSAGE),
    )
    .unwrap();
    std::fs::set_permissions(&adroit, std::fs::Permissions::from_mode(0o755)).unwrap();
    let settings = settings(&home, &user, &opencode());
    let mut request = request(&work, "say hello", None);
    request.delegation = Some(Delegation::new(
        adroit_model::SessionId::new(),
        None,
        dir.path().join("delegation"),
        "test-token".into(),
    ));

    let launch = resolve(&settings, request, &adroit).unwrap();
    let id = daemon.spawn(spawn(launch.spawn)).await.unwrap();
    let (_, screen) = until(&daemon, id, |state, screen| {
        state == Some(State::Idle)
            && replies(screen) == 2
            && screen.iter().any(|l| l.contains(MESSAGE))
    })
    .await;
    daemon.end(id).await.unwrap();
    let said = screen.iter().position(|l| l.contains("say hello")).unwrap();
    let heard = screen.iter().position(|l| l.contains(MESSAGE)).unwrap();
    assert!(said < heard, "{}", screen.join("\n"));
}

#[tokio::test]
#[ignore = "runs the OpenCode installed on the machine, or $ADROIT_TEST_OPENCODE"]
async fn a_read_only_opencode_session_runs_no_command_whatever_the_repository_allows() {
    let (daemon, dir) = daemon().await;
    let work = dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let ran = work.join("ran");
    let port = model_calling(Some((
        "shell",
        serde_json::json!({"command": format!("touch {}", ran.display())}),
    )))
    .await;
    let home = dir.path().join("oc-home");
    account(&home, port);
    let user = dir.path().join("user");
    std::fs::create_dir_all(&user).unwrap();
    // The repository gives every agent, and the agents it may run as above
    // all, leave for everything.
    let everything = serde_json::json!([{"action":"*", "resource":"*", "effect":"allow"}]);
    std::fs::write(
        work.join("opencode.json"),
        serde_json::json!({"permissions": everything, "agents": {"plan": {"permissions": everything}, "build": {"permissions": everything}}})
            .to_string(),
    )
    .unwrap();
    let settings = settings(&home, &user, &opencode());
    let mut request = request(&work, "touch a file", None);
    request.permissions = adroit_model::AgentPermissions::ReadOnly;
    let launch = resolve(&settings, request, Path::new(ADROIT)).unwrap();
    let id = daemon.spawn(spawn(launch.spawn)).await.unwrap();
    // Denied rather than asked, the call fails and the model replies.
    let (_, screen) = until(&daemon, id, |state, screen| {
        state == Some(State::Idle) && replies(screen) == 1
    })
    .await;
    daemon.end(id).await.unwrap();
    assert!(!ran.exists(), "{}", screen.join("\n"));
}
