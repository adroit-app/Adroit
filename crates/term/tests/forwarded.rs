//! A byte-only socket forward, without local daemon lifecycle authority.

use std::path::{Path, PathBuf};
use std::time::Duration;

use adroit_term::{Daemon, Error, Palette, Pane, Size};
use tokio::net::{UnixListener, UnixStream};
use tokio::task::{JoinHandle, JoinSet};

mod support;
use support::{sh, socket_dir};

#[tokio::test]
async fn a_mirror_keeps_reading_while_keys_pastes_and_direct_input_are_refused() {
    let dir = socket_dir();
    let socket = dir.path().join("daemon.sock");
    let serving = socket.clone();
    let server = tokio::spawn(async move {
        Daemon::serve(&serving, Duration::from_secs(60), |_| {})
            .await
            .unwrap();
    });
    let local = Daemon::new(&socket);
    tokio::time::timeout(Duration::from_secs(5), async {
        while local.sessions().await.is_err() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let id = local
        .spawn(sh(
            "while IFS= read -r line; do printf 'reply:%s\\n' \"$line\"; done",
        ))
        .await
        .unwrap();
    let permission = adroit_term::InputPermission::closed();
    let mirror = Daemon::forwarded(&socket).with_input(permission.clone());
    let pane = Pane::attach(&mirror, id, Size::new(80, 8), &Palette::default(), || {})
        .await
        .unwrap();
    assert!(!pane.key(&adroit_term::Key::new("Enter")));
    pane.type_text("forbidden");
    pane.paste("also-forbidden");
    pane.fenced().await.unwrap();
    assert_eq!(pane.last_input_ms(), None);
    assert!(matches!(
        mirror.press(id, b"forbidden\r").await,
        Err(Error::InputDisabled)
    ));
    local.press(id, b"observed\r").await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !pane.screen().text().join("\n").contains("reply:observed") {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(!pane.screen().text().join("\n").contains("forbidden"));
    permission.set(true);
    pane.type_text("accepted");
    assert!(pane.key(&adroit_term::Key::new("Enter")));
    pane.fenced().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !pane.screen().text().join("\n").contains("reply:accepted") {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // This test runs on one Tokio thread: revocation happens before the
    // pane writer can poll the input just queued.
    pane.type_text("must-not-replay");
    permission.set(false);
    permission.set(true);
    assert!(pane.fenced().await.is_err());
    local.press(id, b"fresh\r").await.unwrap();
    let screen = local.everything(id, 80, &Palette::default()).await.unwrap();
    assert!(!screen.text().join("\n").contains("must-not-replay"));
    local.end(id).await.unwrap();
    drop(pane);
    server.abort();
    let _ = server.await;
}

struct Forward(JoinHandle<()>);

impl Forward {
    fn open(socket: &Path, target: PathBuf) -> Self {
        let listener = UnixListener::bind(socket).unwrap();
        Self(tokio::spawn(async move {
            let mut streams = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((mut from, _)) = accepted else { return };
                        let target = target.clone();
                        streams.spawn(async move {
                            if let Ok(mut to) = UnixStream::connect(target).await {
                                let _ = tokio::io::copy_bidirectional(&mut from, &mut to).await;
                            }
                        });
                    }
                    _ = streams.join_next(), if !streams.is_empty() => {}
                }
            }
        }))
    }
}

impl Drop for Forward {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
async fn forwarded_endpoints_refuse_local_lifecycle_before_connecting_or_starting_a_process() {
    let dir = socket_dir();
    let socket = dir.path().join("not-running.sock");
    let endpoint = Daemon::forwarded(&socket);
    let marker = dir.path().join("started");
    let args = vec![
        "-c".into(),
        "touch \"$0\"".into(),
        marker.display().to_string(),
    ];
    let refused = tokio::time::timeout(Duration::from_secs(1), async {
        assert!(matches!(
            endpoint.start(Path::new("/bin/sh"), &args).await,
            Err(Error::Forwarded(_))
        ));
        assert!(matches!(endpoint.stop().await, Err(Error::Forwarded(_))));
        assert!(matches!(
            endpoint.restart(Path::new("/bin/sh"), &args).await,
            Err(Error::Forwarded(_))
        ));
    })
    .await;
    assert!(refused.is_ok());
    assert!(!marker.exists());
    assert!(!socket.exists());
}

#[tokio::test]
async fn the_local_process_serving_a_forward_is_never_reported_as_the_daemon() {
    let dir = socket_dir();
    let socket = dir.path().join("forward.sock");
    let _listener = UnixListener::bind(&socket).unwrap();
    assert!(Daemon::new(&socket).pid().is_some());
    assert_eq!(Daemon::forwarded(&socket).pid(), None);
    assert_ne!(Daemon::new(&socket), Daemon::forwarded(&socket));
}

#[tokio::test]
async fn a_byte_only_forward_carries_screens_input_and_reattachment_without_ending_the_session() {
    let dir = socket_dir();
    let socket = dir.path().join("daemon.sock");
    let serving = socket.clone();
    let server = tokio::spawn(async move {
        Daemon::serve(&serving, Duration::from_secs(60), |_| {})
            .await
            .unwrap();
    });
    let local = Daemon::new(&socket);
    tokio::time::timeout(Duration::from_secs(5), async {
        while local.sessions().await.is_err() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let id = local
        .spawn(sh(
            "printf '日本語\\n'; while IFS= read -r line; do printf 'reply:%s\\n' \"$line\"; done",
        ))
        .await
        .unwrap();
    let forward_socket = dir.path().join("forward.sock");
    let relay = Forward::open(&forward_socket, socket.clone());
    let forwarded = Daemon::forwarded(&forward_socket);
    let pane = Pane::attach(&forwarded, id, Size::new(40, 6), &Palette::default(), || {})
        .await
        .unwrap();
    forwarded.press(id, b"first\r").await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !pane.screen().text().join("\n").contains("reply:first") {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(pane.screen().text().join("\n").contains("日本語"));
    drop(relay);
    tokio::time::timeout(Duration::from_secs(5), async {
        while !pane.exited() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(!pane.program_exited());
    assert!(pane.screen().text().join("\n").contains("reply:first"));
    drop(pane);
    assert!(local.sessions().await.unwrap().iter().any(|s| s.id == id));
    let second_socket = dir.path().join("again.sock");
    let _again = Forward::open(&second_socket, socket);
    let again = Daemon::forwarded(second_socket);
    let screen = again.everything(id, 40, &Palette::default()).await.unwrap();
    assert!(screen.text().join("\n").contains("reply:first"));
    assert!(screen.text().join("\n").contains("日本語"));
    let burst = local
        .spawn(sh(
            "stty raw -echo; printf '\\033[?1049hready'; head -c 600 | wc -c; sleep 5",
        ))
        .await
        .unwrap();
    let scrolling = Pane::attach(&again, burst, Size::new(40, 6), &Palette::default(), || {})
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !scrolling.screen().text().join("\n").contains("ready") {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    scrolling.wheel(200, 1, 1);
    scrolling.fenced().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !scrolling.screen().text().join("\n").contains("600") {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(!scrolling.exited());
    local.end(burst).await.unwrap();
    let mirror = Pane::attach(&again, id, Size::new(40, 6), &Palette::default(), || {})
        .await
        .unwrap();
    for width in 40..240 {
        mirror.resize(Size::new(width, 6));
    }
    assert!(
        mirror.exited(),
        "an overloaded forwarded writer must close instead of retaining unbounded updates"
    );
    assert!(!mirror.program_exited());
    assert!(
        local
            .sessions()
            .await
            .unwrap()
            .iter()
            .any(|session| session.id == id)
    );
    local.end(id).await.unwrap();
    server.abort();
}
