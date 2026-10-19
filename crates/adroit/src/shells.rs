//! The shells adroit runs, and how sessions ask for them over a unix socket.
//! Claude Code's external editor asks over the same socket.
//!
//! A request and its answer are each one line of JSON. The window answers;
//! `adroit shell` asks on behalf of a session, sending the directory it runs
//! in so the window picks the shells of that worktree.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use adroit_model::{Shell, innermost};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Request {
    /// The names of the shells in the worktree holding `cwd`.
    List { cwd: PathBuf },
    /// What the named shell in the worktree holding `cwd` shows.
    Screen { cwd: PathBuf, name: String },
    /// Take the text Claude Code handed its external editor in `path`.
    Editor { path: PathBuf },
    /// Come to the front, for a window of the same instance opened again.
    Show,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Response {
    Shells(Vec<String>),
    Lines(Vec<String>),
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

/// Answer a request from the shells adroit runs. `worktrees` are the other
/// worktrees adroit knows, so a directory in one without shells is not taken
/// for its parent's. `screen` reads what a shell shows.
pub async fn answer(
    request: &Request,
    shells: &[Shell],
    worktrees: &[PathBuf],
    screen: impl AsyncFn(&Shell) -> Option<Vec<String>>,
) -> Response {
    let cwd = match request {
        Request::List { cwd } | Request::Screen { cwd, .. } => cwd,
        Request::Editor { .. } | Request::Show => {
            return Response::Error("not a question about shells".into());
        }
    };
    let known = shells.iter().map(|s| &s.worktree).chain(worktrees);
    let Some(home) = innermost(known, cwd) else {
        return Response::Error(format!("{} is in no worktree adroit knows", cwd.display()));
    };
    let mut here = shells.iter().filter(|s| s.worktree == *home);
    match request {
        Request::List { .. } => Response::Shells(here.map(|s| s.name.clone()).collect()),
        Request::Editor { .. } | Request::Show => unreachable!(),
        Request::Screen { name, .. } => match here.find(|s| &s.name == name) {
            Some(s) => match screen(s).await {
                Some(lines) => Response::Lines(trim_bottom(lines)),
                None => Response::Error(format!("{name} has not started")),
            },
            None => Response::Error(format!("no shell named {name} in {}", home.display())),
        },
    }
}

fn trim_bottom(mut lines: Vec<String>) -> Vec<String> {
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
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

const USAGE: &str = "usage: adroit shell list\n       adroit shell screen <name>";

/// `adroit shell …`: ask the window about the shells of the current worktree.
/// Returns the process exit code.
pub fn cli(args: &[String]) -> i32 {
    // Resolved here, so the window compares real paths with real paths.
    let cwd = match std::env::current_dir().and_then(std::fs::canonicalize) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("adroit: {e}");
            return 1;
        }
    };
    let request = match args {
        [op] if op == "list" => Request::List { cwd },
        [op, name] if op == "screen" => Request::Screen {
            cwd,
            name: name.clone(),
        },
        _ => {
            eprintln!("{USAGE}");
            return 2;
        }
    };
    let socket = socket_path(&crate::paths::instance_state());
    match ask(&socket, &request) {
        Ok(Response::Taken(_) | Response::Declined | Response::Shown) => 0,
        Ok(Response::Shells(names)) | Ok(Response::Lines(names)) => {
            for n in names {
                println!("{n}");
            }
            0
        }
        Ok(Response::Error(e)) => {
            eprintln!("adroit: {e}");
            1
        }
        Err(e) => {
            eprintln!(
                "adroit: cannot reach the window at {}: {e}",
                socket.display()
            );
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(id: u128, worktree: &str) -> Shell {
        Shell {
            runs: Default::default(),
            id,
            name: format!("shell-{id}"),
            worktree: worktree.into(),
        }
    }

    fn list(cwd: &str) -> Request {
        Request::List { cwd: cwd.into() }
    }

    #[test]
    fn requests_and_answers_are_one_json_object_each() {
        let r = Request::Screen {
            cwd: "/api".into(),
            name: "dev".into(),
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"op":"screen","cwd":"/api","name":"dev"}"#
        );
        assert_eq!(
            serde_json::to_string(&Response::Shells(vec!["dev".into()])).unwrap(),
            r#"{"shells":["dev"]}"#
        );
    }

    #[test]
    fn a_window_opened_again_asks_the_open_one_to_come_to_the_front() {
        assert_eq!(
            serde_json::to_string(&Request::Show).unwrap(),
            r#"{"op":"show"}"#
        );
        assert_eq!(
            serde_json::to_string(&Response::Shown).unwrap(),
            r#""shown""#
        );
    }

    #[test]
    fn claude_code_s_editor_hands_over_its_file_and_hears_how_many_lines_were_taken() {
        let r = Request::Editor {
            path: "/tmp/claude-prompt.md".into(),
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"op":"editor","path":"/tmp/claude-prompt.md"}"#
        );
        assert_eq!(
            serde_json::to_string(&Response::Taken(2)).unwrap(),
            r#"{"taken":2}"#
        );
        assert_eq!(
            serde_json::to_string(&Response::Declined).unwrap(),
            r#""declined""#
        );
    }

    #[tokio::test]
    async fn an_editor_request_is_not_answered_from_the_shells() {
        let r = Request::Editor { path: "/f".into() };
        assert!(matches!(
            answer(&r, &[], &[], async |_| None).await,
            Response::Error(_)
        ));
    }

    #[tokio::test]
    async fn a_session_sees_the_shells_of_its_own_worktree() {
        let shells = [shell(1, "/api"), shell(2, "/web"), shell(3, "/api")];
        assert_eq!(
            answer(&list("/api/src"), &shells, &[], async |_| None).await,
            Response::Shells(vec!["shell-1".into(), "shell-3".into()])
        );
    }

    #[tokio::test]
    async fn a_nested_worktree_does_not_see_the_main_checkout_s_shells() {
        let shells = [shell(1, "/api")];
        let nested = PathBuf::from("/api/.claude/worktrees/feat");
        assert_eq!(
            answer(
                &list("/api/.claude/worktrees/feat"),
                &shells,
                &[nested],
                async |_| None
            )
            .await,
            Response::Shells(vec![])
        );
    }

    #[tokio::test]
    async fn a_directory_outside_every_worktree_is_an_error() {
        let got = answer(&list("/tmp"), &[shell(1, "/api")], &[], async |_| None).await;
        assert!(matches!(got, Response::Error(_)));
    }

    #[tokio::test]
    async fn a_screen_comes_without_the_blank_rows_below_it() {
        let req = Request::Screen {
            cwd: "/api".into(),
            name: "shell-1".into(),
        };
        let got = answer(&req, &[shell(1, "/api")], &[], async |_| {
            Some(vec![
                "$ make".into(),
                "ok".into(),
                String::new(),
                String::new(),
            ])
        })
        .await;
        assert_eq!(got, Response::Lines(vec!["$ make".into(), "ok".into()]));
    }

    #[tokio::test]
    async fn an_unknown_shell_is_an_error() {
        let req = Request::Screen {
            cwd: "/api".into(),
            name: "dev".into(),
        };
        let got = answer(&req, &[shell(1, "/api")], &[], async |_| Some(vec![])).await;
        assert_eq!(got, Response::Error("no shell named dev in /api".into()));
    }

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
            assert_eq!(req, list("/api"));
            reply.send(Response::Shells(vec!["dev".into()])).unwrap();
        });
        let p = path.clone();
        let got = tokio::task::spawn_blocking(move || ask(&p, &list("/api")))
            .await
            .unwrap()
            .unwrap();
        window.await.unwrap();
        assert_eq!(got, Response::Shells(vec!["dev".into()]));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_caller_that_says_nothing_holds_up_no_other() {
        let path = temp_socket("idle");
        let mut asked = serve(bind(&path).unwrap());
        let _idle = UnixStream::connect(&path).unwrap();
        tokio::spawn(async move {
            while let Some((_, reply)) = asked.recv().await {
                let _ = reply.send(Response::Shells(vec![]));
            }
        });
        let p = path.clone();
        let got = tokio::task::spawn_blocking(move || ask(&p, &list("/api")))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got, Response::Shells(vec![]));
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
