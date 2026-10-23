//! Window-owned editor handoff and foreground requests.
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Request {
    /// Take the text Claude Code handed its external editor in `path`.
    Editor { path: PathBuf },
    /// Come to the front, for a window of the same instance opened again.
    Show,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Response {
    /// How many lines were taken from Claude Code's input.
    Taken(usize),
    /// The window did not ask for the text: the user opened the editor.
    Declined,
    /// The window came to the front.
    Shown,
    Error(String),
}

pub use adroit_model::paths::window_socket as socket_path;

/// Ask the window of this instance, if one is open, to come to the front;
/// whether it did, leaving this process nothing to open.
pub fn hand_to_open_window() -> bool {
    let socket = socket_path(&adroit_model::paths::instance_state());
    matches!(ask(&socket, &Request::Show), Ok(Response::Shown))
}

/// Send one request to the window and read its answer.
pub fn ask(socket: &Path, request: &Request) -> std::io::Result<Response> {
    let mut stream = UnixStream::connect(socket)?;
    let mut line = serde_json::to_string(request).map_err(std::io::Error::other)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply)?;
    serde_json::from_str(&reply).map_err(std::io::Error::other)
}

/// Claim the socket for this window. A socket left by a window that has gone
/// is replaced; one another window still answers on is left alone. Windows
/// starting together take turns, so only one of them claims it.
pub fn bind(socket: &Path) -> std::io::Result<tokio::net::UnixListener> {
    let dir = match socket.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    std::fs::create_dir_all(dir)?;
    // The folder is locked, not a file beside the socket: a lock file is what
    // a cleaner of old files takes away, and two windows then hold different
    // locks.
    let turn = std::fs::File::open(dir)?;
    turn.lock()?;
    if UnixStream::connect(socket).is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            format!("another adroit answers on {}", socket.display()),
        ));
    }
    let _ = std::fs::remove_file(socket);
    tokio::net::UnixListener::bind(socket)
}

/// A request the window is asked, with where to send its answer.
pub type Asked = (Request, oneshot::Sender<Response>);

/// Read requests from every connection to `listener`, each on its own task,
/// so a slow caller holds up no other. The window answers them in order.
pub fn serve(listener: tokio::net::UnixListener) -> mpsc::UnboundedReceiver<Asked> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(reply(stream, tx.clone()));
        }
    });
    rx
}

async fn reply(
    stream: tokio::net::UnixStream,
    window: mpsc::UnboundedSender<Asked>,
) -> std::io::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let (read, mut write) = stream.into_split();
    let mut line = String::new();
    tokio::io::BufReader::new(read).read_line(&mut line).await?;
    let response = match serde_json::from_str(&line) {
        Ok(request) => {
            let (tx, rx) = oneshot::channel();
            if window.send((request, tx)).is_err() {
                return Ok(());
            }
            rx.await
                .unwrap_or_else(|_| Response::Error("the window is closing".into()))
        }
        Err(e) => Response::Error(format!("cannot read the request: {e}")),
    };
    let mut out = serde_json::to_string(&response).map_err(std::io::Error::other)?;
    out.push('\n');
    write.write_all(out.as_bytes()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_answers_keep_the_lines_they_are_written_as() {
        let written = [
            (
                Request::Editor {
                    path: "/tmp/edit.md".into(),
                },
                r#"{"op":"editor","path":"/tmp/edit.md"}"#,
            ),
            (Request::Show, r#"{"op":"show"}"#),
        ];
        for (request, line) in written {
            assert_eq!(serde_json::to_string(&request).unwrap(), line);
            assert_eq!(serde_json::from_str::<Request>(line).unwrap(), request);
        }
        let answered = [
            (Response::Taken(3), r#"{"taken":3}"#),
            (Response::Declined, r#""declined""#),
            (Response::Shown, r#""shown""#),
            (Response::Error("no".into()), r#"{"error":"no"}"#),
        ];
        for (response, line) in answered {
            assert_eq!(serde_json::to_string(&response).unwrap(), line);
            assert_eq!(serde_json::from_str::<Response>(line).unwrap(), response);
        }
    }
    use std::io::Read;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::process::CommandExt;

    fn temp_socket(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("adroit-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("s.sock")
    }

    fn stale_socket(path: &Path) -> tokio::net::UnixSocket {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // A concurrent fork may inherit the descriptor before exec. Never
        // listening keeps this fixture stale even while that child holds it.
        let socket = tokio::net::UnixSocket::new_stream().unwrap();
        socket.bind(path).unwrap();
        socket
    }

    fn while_a_child_is_before_exec<T>(check: impl FnOnce() -> T) -> T {
        let (mut parent, child) = UnixStream::pair().unwrap();
        let mut command = std::process::Command::new("/usr/bin/true");
        // SAFETY: only async-signal-safe read/write calls run after fork. The
        // handshake keeps inherited descriptors open until the parent checks.
        unsafe {
            command.pre_exec(move || {
                let mut byte = 1_u8;
                if libc::write(child.as_raw_fd(), (&byte as *const u8).cast(), 1) != 1
                    || libc::read(child.as_raw_fd(), (&mut byte as *mut u8).cast(), 1) != 1
                {
                    return Err(std::io::Error::from_raw_os_error(libc::EIO));
                }
                Ok(())
            });
        }
        let starting = std::thread::spawn(move || command.status().unwrap());
        parent.read_exact(&mut [0]).unwrap();
        let result = check();
        parent.write_all(&[1]).unwrap();
        assert!(starting.join().unwrap().success());
        result
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_request_crosses_the_socket_and_its_answer_comes_back() {
        let path = temp_socket("ask");
        let mut asked = serve(bind(&path).unwrap());
        let window = tokio::spawn(async move {
            let (req, reply) = asked.recv().await.unwrap();
            assert_eq!(req, Request::Show);
            reply.send(Response::Shown).unwrap();
        });
        let p = path.clone();
        let got = tokio::task::spawn_blocking(move || ask(&p, &Request::Show))
            .await
            .unwrap()
            .unwrap();
        window.await.unwrap();
        assert_eq!(got, Response::Shown);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_caller_that_says_nothing_holds_up_no_other() {
        let path = temp_socket("idle");
        let mut asked = serve(bind(&path).unwrap());
        let _idle = UnixStream::connect(&path).unwrap();
        tokio::spawn(async move {
            while let Some((_, reply)) = asked.recv().await {
                let _ = reply.send(Response::Shown);
            }
        });
        let p = path.clone();
        let got = tokio::task::spawn_blocking(move || ask(&p, &Request::Show))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got, Response::Shown);
    }

    #[tokio::test]
    async fn a_socket_left_behind_is_replaced_but_a_live_one_is_not() {
        let stale = temp_socket("stale");
        drop(stale_socket(&stale));
        bind(&stale).unwrap();

        let path = temp_socket("bind");
        let _first = bind(&path).unwrap();
        assert_eq!(
            bind(&path).unwrap_err().kind(),
            std::io::ErrorKind::AddrInUse
        );
    }

    #[tokio::test]
    async fn the_stale_fixture_can_be_replaced_while_a_child_inherits_its_descriptor() {
        let path = temp_socket("inherited-stale");
        let first = stale_socket(&path);
        let replacement = while_a_child_is_before_exec(|| {
            drop(first);
            bind(&path)
        });
        replacement.unwrap();
    }

    #[tokio::test]
    async fn a_live_listener_inherited_by_a_child_is_not_unlinked() {
        let path = temp_socket("inherited-live");
        let first = bind(&path).unwrap();
        let inode = std::fs::metadata(&path).unwrap().ino();
        let (replacement, connection, metadata) = while_a_child_is_before_exec(|| {
            drop(first);
            (
                bind(&path),
                UnixStream::connect(&path),
                std::fs::metadata(&path),
            )
        });
        assert_eq!(
            replacement.unwrap_err().kind(),
            std::io::ErrorKind::AddrInUse
        );
        connection.unwrap();
        assert_eq!(metadata.unwrap().ino(), inode);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn of_windows_starting_together_only_one_answers() {
        const WINDOWS: usize = 8;
        for round in 0..20 {
            let path = temp_socket(&format!("race-{round}"));
            let start = std::sync::Arc::new(std::sync::Barrier::new(WINDOWS));
            let windows: Vec<_> = (0..WINDOWS)
                .map(|_| {
                    let (path, start) = (path.clone(), start.clone());
                    tokio::task::spawn_blocking(move || {
                        start.wait();
                        bind(&path)
                    })
                })
                .collect();
            let mut bound = Vec::new();
            for w in windows {
                if let Ok(listener) = w.await.unwrap() {
                    bound.push(listener);
                }
            }
            assert_eq!(bound.len(), 1, "round {round}");
        }
    }
}
