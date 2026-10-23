use adroit_host::execution::TerminalStartRequest;
use adroit_host::settings::Settings;
use adroit_host::terminals;
use adroit_model::TerminalResourceId;
use adroit_term::Daemon;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

async fn daemon() -> (Daemon, tempfile::TempDir) {
    let dir = tempfile::Builder::new()
        .prefix("adroit-tools-")
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

#[tokio::test]
async fn terminal_resources_can_be_started_read_and_stopped_without_a_window() {
    let (daemon, dir) = daemon().await;
    let mut settings = Settings::default();
    settings.shell.command = vec![
        "/bin/sh".into(),
        "-c".into(),
        "printf terminal-log; read answer".into(),
    ];
    let terminal = terminals::start(
        &daemon,
        &settings,
        TerminalStartRequest {
            name: "server".into(),
            cwd: dir.path().into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let listed = terminals::list(&daemon).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].resource_id, terminal.resource_id);
    assert_eq!(listed[0].cwd, dir.path());
    let mut seen = false;
    for _ in 0..100 {
        if terminals::screen(&daemon, terminal.resource_id)
            .await
            .unwrap()
            .iter()
            .any(|line| line.contains("terminal-log"))
        {
            seen = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        terminals::screen(&daemon, TerminalResourceId::new())
            .await
            .is_err()
    );
    assert_eq!(terminals::list(&daemon).await.unwrap().len(), 1);
    terminals::stop(&daemon, terminal.resource_id)
        .await
        .unwrap();
    assert!(terminals::list(&daemon).await.unwrap().is_empty());
    assert!(seen, "the headless terminal did not start its program");
}
