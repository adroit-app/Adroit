use adroit_host::execution::TerminalStartRequest;
use adroit_host::settings::Settings;
use adroit_host::terminals;
use adroit_model::TerminalResourceId;
use adroit_term::{Daemon, Input};
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

/// A Neovim started on a new file in `dir`, of the user's configuration
/// none, with its first screen drawn.
async fn neovim_on_a_file(daemon: &Daemon, dir: &std::path::Path) -> adroit_model::TerminalSession {
    let mut settings = Settings::default();
    settings.neovim.command = vec!["nvim".into(), "--clean".into(), "-n".into()];
    let file = dir.join("notes.txt");
    let terminal = terminals::start(
        daemon,
        &settings,
        TerminalStartRequest {
            name: "notes.txt".into(),
            cwd: dir.into(),
            program: adroit_model::TerminalProgram::Neovim,
            file: Some(file),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(
        screen_shows(daemon, &terminal, "notes.txt").await,
        "Neovim did not draw its first screen"
    );
    terminal
}

async fn screen_shows(
    daemon: &Daemon,
    terminal: &adroit_model::TerminalSession,
    text: &str,
) -> bool {
    for _ in 0..300 {
        let lines = terminals::screen(daemon, terminal.resource_id)
            .await
            .unwrap_or_default();
        if lines.iter().any(|line| line.contains(text)) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

async fn listed(daemon: &Daemon, terminal: &adroit_model::TerminalSession) -> bool {
    terminals::list(daemon)
        .await
        .unwrap()
        .iter()
        .any(|t| t.resource_id == terminal.resource_id)
}

#[tokio::test]
async fn a_neovim_asked_to_quit_with_nothing_unsaved_ends() {
    let (daemon, dir) = daemon().await;
    let terminal = neovim_on_a_file(&daemon, dir.path()).await;
    // Left in insert mode, as a user may leave it.
    daemon
        .input(terminal.id, &[Input::Text("i".into())])
        .await
        .unwrap();
    terminals::quit_neovim(&daemon, terminal.id).await.unwrap();
    let mut ended = false;
    for _ in 0..300 {
        if !listed(&daemon, &terminal).await {
            ended = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(ended, "Neovim did not quit");
}

#[tokio::test]
async fn a_neovim_asked_to_quit_with_an_unsaved_edit_keeps_running_and_the_edit() {
    let (daemon, dir) = daemon().await;
    let terminal = neovim_on_a_file(&daemon, dir.path()).await;
    daemon
        .input(terminal.id, &[Input::Text("iunsaved words".into())])
        .await
        .unwrap();
    assert!(screen_shows(&daemon, &terminal, "unsaved words").await);
    terminals::quit_neovim(&daemon, terminal.id).await.unwrap();
    assert!(
        screen_shows(&daemon, &terminal, "E37").await,
        "Neovim did not refuse to quit"
    );
    assert!(listed(&daemon, &terminal).await);
    assert!(screen_shows(&daemon, &terminal, "unsaved words").await);
    terminals::stop(&daemon, terminal.resource_id)
        .await
        .unwrap();
}
