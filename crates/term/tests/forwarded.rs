//! A byte-only socket forward, without local daemon lifecycle authority.

use std::path::{Path, PathBuf};
use std::time::Duration;

use adroit_term::{Daemon, Error, Palette, Pane, Size};
use tokio::net::{UnixListener, UnixStream};
use tokio::task::{JoinHandle, JoinSet};

mod support;
use support::{sh, socket_dir};

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
    drop(pane);
    drop(relay);
    assert!(local.sessions().await.unwrap().iter().any(|s| s.id == id));
    let second_socket = dir.path().join("again.sock");
    let _again = Forward::open(&second_socket, socket);
    let again = Daemon::forwarded(second_socket);
    let screen = again.everything(id, 40, &Palette::default()).await.unwrap();
    assert!(screen.text().join("\n").contains("reply:first"));
    assert!(screen.text().join("\n").contains("日本語"));
    local.end(id).await.unwrap();
    server.abort();
}
