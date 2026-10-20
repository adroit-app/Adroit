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

/// `$ADROIT_SOCKET`, or `adroit.sock` in the instance's folder `dir`.
pub fn socket_path(dir: &Path) -> PathBuf {
    std::env::var_os("ADROIT_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.join("adroit.sock"))
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

    fn temp_socket(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("adroit-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("s.sock")
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
        let path = temp_socket("bind");
        let first = bind(&path).unwrap();
        assert_eq!(
            bind(&path).unwrap_err().kind(),
            std::io::ErrorKind::AddrInUse
        );
        drop(first);
        assert!(bind(&path).is_ok());
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
