//! Pi as it runs in a console of adroit's, against a model of the test's own
//! that answers every prompt alike, with a login of its own: no
//! provider's credentials are needed or reached.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use adroit_host::delegation::launch::Delegation;
use adroit_host::execution::{StartRequest, resolve};
use adroit_host::settings::Settings;
use adroit_model::{Agent, State};
use adroit_term::{Input, Key};

mod support;

use support::{REPLY, spawn};

/// How slowly the model answers, word by word, for a turn to be seen running.
const PACE: Duration = Duration::from_millis(150);

/// How many times a session is looked at before a test gives up on it.
const TRIES: usize = 600;

/// Held by each test for its whole run: every Pi session of an instance loads
/// the one extension adroit writes, naming the adroit that wrote it last.
static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Settings with a Pi profile with a login of its own in `home`, its
/// models naming the endpoint on `port`.
fn settings(home: &Path, port: u16) -> Settings {
    settings_loading(home, port, &[])
}

/// [`settings`], Pi loading the extensions `extensions` too.
fn settings_loading(home: &Path, port: u16, extensions: &[&Path]) -> Settings {
    std::fs::create_dir_all(home).unwrap();
    std::fs::write(
        home.join("models.json"),
        serde_json::json!({"providers":{"fake":{"baseUrl":format!("http://127.0.0.1:{port}/v1"),
            "api":"openai-completions","apiKey":"dummy",
            "models":[{"id":"slow","name":"Slow fake","contextWindow":32768,"maxTokens":4096}]}}})
        .to_string(),
    )
    .unwrap();
    let pi = std::env::var("ADROIT_TEST_PI").unwrap_or_else(|_| "pi".into());
    let command = std::iter::once(pi)
        .chain(
            extensions
                .iter()
                .flat_map(|e| ["-e".into(), e.display().to_string()]),
        )
        .map(|word| serde_json::to_string(&word).unwrap())
        .collect::<Vec<_>>()
        .join(", ");
    Settings::parse(&format!(
        "profile = \"pi-fake\"\n\n[[profiles]]\nid = \"pi-fake\"\nagent = \"pi\"\nhome = \"{}\"\n\n[profiles.launch]\ncommand = [{command}]\n\n[profiles.launch.env]\nPI_OFFLINE = \"1\"\nPI_SKIP_VERSION_CHECK = \"1\"\nPI_TELEMETRY = \"0\"\n\n[profiles.pi]\nmodel = \"fake/slow\"\n",
        home.display()
    ))
    .unwrap()
}

/// Whether the program says it waits for a prompt, a turn over or none run.
fn settled(state: &Option<State>) -> bool {
    matches!(state, Some(State::Idle | State::Done))
}

#[tokio::test]
#[ignore = "runs the Pi installed on the machine, or $ADROIT_TEST_PI"]
async fn pi_starts_replies_reports_itself_idle_and_resumes_its_conversation() {
    let _alone = ONE_AT_A_TIME.lock().await;
    let (daemon, dir) = support::daemon("adroit-pi-").await;
    let port = support::model(PACE, None).await;
    let home = dir.path().join("pi-home");
    let work = dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let settings = settings(&home, port);
    let adapter = adroit_agents::adapter(Agent::Pi);
    let request = |prompt: &str, resume: Option<String>| StartRequest {
        agent: Agent::Pi,
        name: "greet".into(),
        cwd: work.clone(),
        prompt: prompt.into(),
        home: resume.as_ref().map(|_| home.clone()),
        resume,
        ..Default::default()
    };
    let adroit = Path::new(env!("CARGO_BIN_EXE_adroit"));

    let launch = resolve(&settings, request("say hello", None), adroit).unwrap();
    let conversation = launch.conversation.clone();
    let id = daemon.spawn(spawn(launch.spawn)).await.unwrap();
    let (state, screen) = support::until(&daemon, id, Agent::Pi, TRIES, |state, screen| {
        settled(&state) && screen.iter().any(|l| l.contains(REPLY))
    })
    .await;
    assert!(settled(&state), "{state:?}");
    assert_eq!(
        adapter.startup_readiness(&screen),
        Some(adroit_model::StartupReadiness::Ready),
        "{}",
        screen.join("\n")
    );

    // A prompt typed in runs a turn the program reports working, then done.
    daemon
        .input(
            id,
            &[Input::Text("again".into()), Input::Key(Key::new("Enter"))],
        )
        .await
        .unwrap();
    support::until(&daemon, id, Agent::Pi, TRIES, |state, _| {
        state == Some(State::Working)
    })
    .await;
    support::until(&daemon, id, Agent::Pi, TRIES, |state, screen| {
        settled(&state) && screen.iter().filter(|l| l.contains(REPLY)).count() == 2
    })
    .await;

    let context = adroit_host::programs::starting(&settings, Some(Agent::Pi), None).unwrap();
    let session = adroit_model::Session {
        agent: Agent::Pi,
        session_id: conversation.clone(),
        cwd: work.clone(),
        ..Default::default()
    };
    let mut reader = adapter
        .transcript(&context, dir.path(), &session)
        .expect("a transcript");
    reader.read();
    assert_eq!(reader.stats().user_turns, 2);
    assert_eq!(reader.stats().model.as_deref(), Some("slow"));
    let past = adapter.past(&context, dir.path(), Some(&work));
    assert_eq!(
        past.iter()
            .map(|p| (p.id.as_str(), p.title.as_str()))
            .collect::<Vec<_>>(),
        [(conversation.as_str(), "greet")]
    );
    assert_eq!(
        adapter.history(&context, dir.path(), &work),
        ["say hello", "again"]
    );
    daemon.end(id).await.unwrap();

    let launch = resolve(&settings, request("", Some(conversation.clone())), adroit).unwrap();
    assert_eq!(launch.conversation, conversation);
    let id = daemon.spawn(spawn(launch.spawn)).await.unwrap();
    let (_, screen) = support::until(&daemon, id, Agent::Pi, TRIES, |state, screen| {
        settled(&state) && screen.iter().any(|l| l.contains(REPLY))
    })
    .await;
    assert!(
        screen.iter().any(|l| l.contains("again")),
        "{}",
        screen.join("\n")
    );
    let sessions: Vec<_> = std::fs::read_dir(home.join("sessions"))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(sessions.len(), 1, "the resume kept to its conversation");
    daemon.end(id).await.unwrap();
}

/// An adroit whose MCP server, the one sessions reach other sessions
/// through, answers with a `send` tool and says nothing more; any other
/// command is `@ADROIT@`'s.
const ADROIT: &str = r#"#!/usr/bin/env node
if (process.argv[2] !== "mcp") {
    const { status } = require("node:child_process").spawnSync("@ADROIT@", process.argv.slice(2), { stdio: "inherit" });
    process.exit(status ?? 1);
}
require("node:readline").createInterface({ input: process.stdin }).on("line", (line) => {
    let message;
    try {
        message = JSON.parse(line);
    } catch {
        return;
    }
    if (message.id === undefined) return;
    const result =
        message.method === "initialize"
            ? { protocolVersion: message.params.protocolVersion, capabilities: { tools: {} }, serverInfo: { name: "adroit", version: "0" } }
            : message.method === "tools/list"
              ? { tools: [{ name: "send", description: "Send a message", inputSchema: { type: "object", properties: {} } }] }
              : {};
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: message.id, result })}\n`);
});
"#;

/// An extension that writes the tools active in Pi to `@OUT@` once one of
/// an MCP server's is among them, or once it has waited long enough.
const LISTER: &str = r#"import { writeFileSync } from "node:fs";
export default function (pi) {
    pi.on("session_start", () => {
        (async () => {
            for (let i = 0; i < 100; i++) {
                if (pi.getAllTools().some((tool) => tool.name.startsWith("mcp__"))) break;
                await new Promise((r) => setTimeout(r, 100));
            }
            writeFileSync("@OUT@", JSON.stringify(pi.getActiveTools()));
        })();
    });
}
"#;

#[tokio::test]
#[ignore = "runs the Pi installed on the machine, or $ADROIT_TEST_PI"]
async fn a_read_only_pi_session_that_reaches_other_sessions_keeps_adroit_s_tools() {
    let _alone = ONE_AT_A_TIME.lock().await;
    let (daemon, dir) = support::daemon("adroit-pi-").await;
    let work = dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let adroit = dir.path().join("adroit");
    std::fs::write(
        &adroit,
        ADROIT.replace("@ADROIT@", env!("CARGO_BIN_EXE_adroit")),
    )
    .unwrap();
    std::fs::set_permissions(&adroit, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = dir.path().join("tools.json");
    let lister = dir.path().join("lister.ts");
    std::fs::write(&lister, LISTER.replace("@OUT@", &out.display().to_string())).unwrap();
    let settings = settings_loading(&dir.path().join("pi-home"), 9, &[&lister]);
    let request = StartRequest {
        agent: Agent::Pi,
        name: "look".into(),
        cwd: work,
        permissions: adroit_model::AgentPermissions::ReadOnly,
        delegation: Some(Delegation::new(
            adroit_model::SessionId::new(),
            None,
            dir.path().join("delegation"),
            "test-token".into(),
        )),
        ..Default::default()
    };

    let launch = resolve(&settings, request, &adroit).unwrap();
    let id = daemon.spawn(spawn(launch.spawn)).await.unwrap();
    support::until(&daemon, id, Agent::Pi, TRIES, |_, _| out.exists()).await;
    daemon.end(id).await.unwrap();
    let active: Vec<String> = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert!(
        active.iter().any(|tool| tool == "mcp__adroit__send"),
        "{active:?}"
    );
    assert!(
        ["read", "grep", "find", "ls"]
            .iter()
            .all(|tool| active.iter().any(|a| a == tool)),
        "{active:?}"
    );
    assert!(
        !active.iter().any(|tool| tool == "bash" || tool == "write"),
        "{active:?}"
    );
}
