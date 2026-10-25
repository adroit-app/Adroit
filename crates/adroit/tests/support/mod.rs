//! What the tests that run a real agent in a console share: a terminal host
//! of their own, a model that answers every prompt alike, and a wait on how
//! a session stands.

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use adroit_model::{Agent, State};
use adroit_term::{Daemon, Spawn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const REPLY: &str = "Hello from the fake model.";

/// A terminal host of the test's own, its socket in a new folder named
/// from `prefix`, and the folder.
pub async fn daemon(prefix: &str) -> (Daemon, tempfile::TempDir) {
    let dir = tempfile::Builder::new()
        .prefix(prefix)
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

/// An OpenAI-compatible endpoint that answers a request with no tool's
/// result in it by calling `tool`, a tool's name and its arguments, when one
/// is given, and streams [`REPLY`] to every other a word each `pace`: its
/// port.
pub async fn model(pace: Duration, tool: Option<(&'static str, serde_json::Value)>) -> u16 {
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
                    tokio::time::sleep(pace).await;
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

/// How the session `id` stands as its program reports it.
pub async fn reported(daemon: &Daemon, id: u128) -> Option<State> {
    daemon
        .sessions()
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.id == id)
        .and_then(|s| s.status)
        .map(|s| State::reported(&s.state, s.kind.as_deref()))
}

/// Until `check` holds of how the session `id` of `agent` stands and its
/// screen, looked at up to `tries` times, or what they were when it gave up.
/// It stands as its program reports, else as its screen shows, as adroit
/// reads it.
pub async fn until(
    daemon: &Daemon,
    id: u128,
    agent: Agent,
    tries: usize,
    check: impl Fn(Option<State>, &[String]) -> bool,
) -> (Option<State>, Vec<String>) {
    let mut last = (None, Vec::new());
    for _ in 0..tries {
        let screen = daemon.visible(id).await.unwrap_or_default();
        let state = match reported(daemon, id).await {
            Some(state) => Some(state),
            None => adroit_agents::adapter(agent).screen_state(&screen),
        };
        if check(state.clone(), &screen) {
            return (state, screen);
        }
        last = (state, screen);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("gave up at {:?}:\n{}", last.0, last.1.join("\n"));
}

/// `launch` in a console of the size the tests read screens at.
pub fn spawn(launch: Spawn) -> Spawn {
    Spawn {
        cols: 120,
        rows: 40,
        ..launch
    }
}
