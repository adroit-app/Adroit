use adroit::control::{client::Client, managed, observer, protocol::Command, service::Service};
use adroit::execution::{self, StartRequest};
use adroit::settings::Settings;
use adroit_model::*;
use adroit_term::{Daemon, Palette};

#[tokio::test]
async fn a_daemon_observes_the_native_conversation_and_keeps_the_same_assignment_without_a_window()
{
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
    let serving = socket.clone();
    let task = tokio::spawn(async move {
        Daemon::serve(&serving, std::time::Duration::from_secs(60), |_| {}).await
    });
    let daemon = Daemon::new(socket);
    for _ in 0..100 {
        if daemon.sessions().await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let conversation = MessageId::new().to_string();
    let mut settings = Settings::default();
    settings.codex.command = vec![
        "/bin/sh".into(),
        "-c".into(),
        r#"printf '\033]0;Codex · %s\007' "$ADROIT_TEST_THREAD"; sleep 10"#.into(),
        "native-provider".into(),
    ];
    settings
        .codex
        .env
        .insert("ADROIT_TEST_THREAD".into(), conversation.clone());
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
                .is_some_and(|title| title.contains(&conversation))
        }) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    observer::tick(&service.actor(), &daemon).await.unwrap();
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
        adroit_term::tags::value(&native.tags, "adroit.claude"),
        Some(conversation.clone())
    );
    assert_eq!(
        adroit_term::tags::value(&native.tags, "adroit.binding-conversation"),
        Some(conversation)
    );
    daemon.end(terminal).await.unwrap();
    task.abort();
}
