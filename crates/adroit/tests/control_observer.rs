mod support;
use adroit::control::{client::Client, managed, observer, protocol::Command, service::Service};
use adroit::execution::{self, StartRequest};
use adroit::settings::Settings;
use adroit_model::*;
use adroit_term::{Daemon, Palette};

#[tokio::test]
async fn a_daemon_observes_the_native_conversation_and_keeps_the_same_assignment_without_a_window()
{
    observe(false).await;
}

#[tokio::test]
async fn a_shortened_native_title_is_resolved_asynchronously_without_a_window() {
    observe(true).await;
}

async fn observe(shortened: bool) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::Builder::new()
        .prefix("adroit-observe-")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let socket = dir.path().join("d.sock");
    let task = support::serve_process(socket.clone());
    let daemon = Daemon::new(socket);
    for _ in 0..100 {
        if daemon.sessions().await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let conversation = MessageId::new().to_string();
    let next_conversation = dir.path().join("next-conversation");
    let mut settings = Settings::default();
    settings.codex.command = vec![
        "/bin/sh".into(),
        "-c".into(),
        support::codex_script(
            &format!(
                "{}\n{}",
                if shortened {
                    r#"printf '\033]0;codex | %.29s...\007' "$ADROIT_TEST_THREAD""#
                } else {
                    r#"printf '\033]0;Codex · %s\007' "$ADROIT_TEST_THREAD""#
                },
                r#"while [ ! -f "$ADROIT_TEST_NEXT_CONVERSATION" ]; do sleep 0.02; done
IFS= read -r next < "$ADROIT_TEST_NEXT_CONVERSATION"
printf '\033]0;Codex · %s\007' "$next"
sleep 10"#
            ),
            None,
        ),
        "native-provider".into(),
    ];
    settings
        .codex
        .env
        .insert("ADROIT_TEST_THREAD".into(), conversation.clone());
    settings.codex.env.insert(
        "ADROIT_TEST_NEXT_CONVERSATION".into(),
        next_conversation.to_string_lossy().into_owned(),
    );
    settings.codex.env.insert(
        "ADROIT_TEST_THREAD_PAGE".into(),
        serde_json::json!({
            "id":"adroit-query", "result":{"data":[{"id":conversation}],"nextCursor":null}
        })
        .to_string(),
    );
    let prepared = managed::prepare(
        &root,
        &settings,
        StartRequest {
            agent: Agent::Codex,
            name: "leader".into(),
            cwd: source,
            prompt: "Review the inputs".into(),
            ..Default::default()
        },
        std::path::Path::new("/adroit"),
    )
    .await
    .unwrap();
    let ownership = prepared.ownership.unwrap();
    let identity = ownership.identity();
    let original_binding =
        adroit_term::tags::holding(&prepared.resolved.spawn.tags, "adroit.binding");
    let original_generation =
        adroit_term::tags::holding(&prepared.resolved.spawn.tags, "adroit.generation");
    let terminal = execution::spawn_headless_prepared(
        &daemon,
        prepared.resolved.spawn,
        &Palette::default(),
        move |terminal| async move { ownership.attach(terminal).await },
    )
    .await
    .unwrap();
    for _ in 0..200 {
        if daemon.sessions().await.unwrap().iter().any(|session| {
            session
                .title
                .as_ref()
                .is_some_and(|title| title.contains(&conversation[..29]))
        }) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let mut observer = observer::Observer::default();
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            observer.tick(&service.actor(), &daemon).await.unwrap();
            let snapshot = service
                .actor()
                .call(|store| store.snapshot())
                .await
                .unwrap();
            if snapshot.sessions[0].conversation_id.as_deref() == Some(conversation.as_str()) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let snapshot = Client::human(&root)
        .unwrap()
        .request(RequestId::new(), Command::Snapshot)
        .await
        .unwrap();
    let snapshot: ControlSnapshot = serde_json::from_value(snapshot).unwrap();
    assert_eq!(snapshot.sessions[0].reference(), identity);
    assert_eq!(
        snapshot.sessions[0].conversation_id,
        Some(conversation.clone())
    );
    assert_eq!(snapshot.sessions[0].state, ManagedSessionState::Running);
    let sessions = daemon.sessions().await.unwrap();
    let native = sessions
        .iter()
        .find(|session| session.id == terminal)
        .unwrap();
    assert_eq!(
        adroit_term::tags::holding(&native.tags, "adroit.binding"),
        original_binding
    );
    assert_eq!(
        adroit_term::tags::holding(&native.tags, "adroit.generation"),
        original_generation
    );
    let binding: SessionBinding =
        serde_json::from_str(&adroit_term::tags::value(&native.tags, "adroit.binding").unwrap())
            .unwrap();
    assert_eq!(binding, snapshot.sessions[0].binding);
    assert_eq!(
        adroit_term::tags::value(&native.tags, "adroit.claude"),
        Some(conversation.clone())
    );
    assert_eq!(
        adroit_term::tags::value(&native.tags, "adroit.binding-conversation"),
        Some(conversation.clone())
    );
    observer.tick(&service.actor(), &daemon).await.unwrap();
    let replacement = MessageId::new().to_string();
    let ready = next_conversation.with_extension("ready");
    std::fs::write(&ready, format!("{replacement}\n")).unwrap();
    std::fs::rename(ready, &next_conversation).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            observer.tick(&service.actor(), &daemon).await.unwrap();
            let snapshot = service
                .actor()
                .call(|store| store.snapshot())
                .await
                .unwrap();
            if snapshot.sessions[0].state == ManagedSessionState::NeedsReconciliation {
                assert_eq!(snapshot.sessions[0].reference(), identity);
                assert_eq!(
                    snapshot.sessions[0].conversation_id.as_deref(),
                    Some(conversation.as_str())
                );
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("a later native conversation change must still reach the assigned observer");
    daemon.end(terminal).await.unwrap();
    Client::human(&root)
        .unwrap()
        .request(
            RequestId::new(),
            Command::ConfirmTerminalExit {
                identity,
                terminal_id: format!("{terminal:032x}"),
            },
        )
        .await
        .unwrap();
    observer.tick(&service.actor(), &daemon).await.unwrap();
    let snapshot = service
        .actor()
        .call(|store| store.snapshot())
        .await
        .unwrap();
    assert_eq!(snapshot.sessions[0].state, ManagedSessionState::Exited);
    assert_eq!(snapshot.runs[0].state, RunState::Active);
    assert!(snapshot.completions.is_empty());
    task.abort();
}
