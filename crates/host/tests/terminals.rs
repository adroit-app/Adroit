use adroit_host::execution::TerminalStartRequest;
use adroit_host::settings::Settings;
use adroit_host::terminals;
use adroit_model::TerminalResourceId;
use adroit_term::{Daemon, Input};
use std::time::Duration;

async fn daemon() -> (Daemon, tempfile::TempDir) {
    let dir = tempfile::Builder::new()
        .prefix("adroit-tools-")
        .tempdir_in(adroit_sys::local::temp_dir())
        .unwrap();
    adroit_sys::fs::set_mode(dir.path(), 0o700).unwrap();
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
#[cfg_attr(
    windows,
    ignore = "holds the program back through the adroit executable, which the test binary is not"
)]
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

#[tokio::test]
#[cfg_attr(
    windows,
    ignore = "holds the program back through the adroit executable, which the test binary is not"
)]
async fn a_terminal_started_without_a_window_is_told_the_theme_s_background_however_late_it_asks() {
    let (daemon, dir) = daemon().await;
    let mut settings = Settings::default();
    settings.appearance = adroit_core::settings::Appearance::Dark;
    // Asked once the program has started up, as Codex asks, its reply shown
    // with the escape as `E`.
    settings.shell.command = vec![
        "/bin/sh".into(),
        "-c".into(),
        r"sleep 0.5; stty raw -echo; printf '\033]11;?\033\\'; head -c 23 | tr '\033' E; sleep 5"
            .into(),
    ];
    let terminal = terminals::start(
        &daemon,
        &settings,
        TerminalStartRequest {
            name: "asks".into(),
            cwd: dir.path().into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let mut screen = Vec::new();
    for _ in 0..200 {
        screen = terminals::screen(&daemon, terminal.resource_id)
            .await
            .unwrap();
        if screen.iter().any(|line| line.contains("rgb:")) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // Nightfox's background, the dark theme named when none is.
    assert!(
        screen
            .iter()
            .any(|line| line.contains("rgb:1919/2323/3030")),
        "{screen:?}"
    );
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
#[cfg_attr(
    windows,
    ignore = "holds the program back through the adroit executable, which the test binary is not"
)]
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
#[cfg_attr(
    windows,
    ignore = "holds the program back through the adroit executable, which the test binary is not"
)]
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

async fn shell_running(daemon: &Daemon, dir: &std::path::Path, script: &str) -> TerminalResourceId {
    let mut settings = Settings::default();
    settings.shell.command = vec!["/bin/sh".into(), "-c".into(), script.into()];
    terminals::start(
        daemon,
        &settings,
        TerminalStartRequest {
            name: "server".into(),
            cwd: dir.into(),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .resource_id
}

fn text(text: &str) -> terminals::Pattern {
    terminals::Pattern::Text(text.into())
}

#[tokio::test]
#[cfg_attr(
    windows,
    ignore = "holds the program back through the adroit executable, which the test binary is not"
)]
async fn a_wait_ends_with_the_line_that_matched_once_the_program_writes_it() {
    let (daemon, dir) = daemon().await;
    let terminal = shell_running(&daemon, dir.path(), "sleep 0.5; echo server ready; read x").await;
    let line = terminals::wait(
        &daemon,
        terminal,
        &text("ready"),
        Duration::from_secs(10),
        false,
    )
    .await;
    assert_eq!(line.as_deref(), Ok("server ready"));
    let regex = terminals::Pattern::Regex(regex::Regex::new("^server r.+y$").unwrap());
    let line = terminals::wait(&daemon, terminal, &regex, Duration::from_secs(10), false).await;
    assert_eq!(line.as_deref(), Ok("server ready"));
    terminals::stop(&daemon, terminal).await.unwrap();
}

#[tokio::test]
#[cfg_attr(
    windows,
    ignore = "holds the program back through the adroit executable, which the test binary is not"
)]
async fn a_wait_finds_a_line_scrolled_off_the_screen() {
    let (daemon, dir) = daemon().await;
    let terminal = shell_running(&daemon, dir.path(), "echo early ready; seq 1 100; read x").await;
    let line = terminals::wait(
        &daemon,
        terminal,
        &text("early"),
        Duration::from_secs(10),
        false,
    )
    .await;
    assert_eq!(line.as_deref(), Ok("early ready"));
    terminals::stop(&daemon, terminal).await.unwrap();
}

#[tokio::test]
#[cfg_attr(
    windows,
    ignore = "holds the program back through the adroit executable, which the test binary is not"
)]
async fn a_new_wait_passes_over_what_was_written_before_it_and_times_out_showing_the_screen() {
    let (daemon, dir) = daemon().await;
    let terminal = shell_running(
        &daemon,
        dir.path(),
        "echo first ready; read x; echo second ready; read y",
    )
    .await;
    let first = terminals::wait(
        &daemon,
        terminal,
        &text("ready"),
        Duration::from_secs(10),
        false,
    )
    .await;
    assert_eq!(first.as_deref(), Ok("first ready"));
    let timed_out = terminals::wait(
        &daemon,
        terminal,
        &text("ready"),
        Duration::from_secs(1),
        true,
    )
    .await
    .unwrap_err();
    assert!(timed_out.contains("first ready"), "{timed_out}");
    let ready = text("ready");
    let (second, _) = tokio::join!(
        terminals::wait(&daemon, terminal, &ready, Duration::from_secs(10), true),
        async {
            tokio::time::sleep(Duration::from_millis(500)).await;
            terminals::input(&daemon, terminal, &[Input::Text("\n".into())])
                .await
                .unwrap();
        }
    );
    assert_eq!(second.as_deref(), Ok("second ready"));
    terminals::stop(&daemon, terminal).await.unwrap();
}

#[tokio::test]
#[cfg_attr(
    windows,
    ignore = "holds the program back through the adroit executable, which the test binary is not"
)]
async fn a_wait_on_a_terminal_that_ends_ends_too() {
    let (daemon, dir) = daemon().await;
    let terminal = shell_running(&daemon, dir.path(), "read x").await;
    let never = text("never");
    let (waited, _) = tokio::join!(
        terminals::wait(&daemon, terminal, &never, Duration::from_secs(30), false),
        async {
            tokio::time::sleep(Duration::from_millis(500)).await;
            terminals::stop(&daemon, terminal).await.unwrap();
        }
    );
    assert!(waited.unwrap_err().contains("ended"));
}
