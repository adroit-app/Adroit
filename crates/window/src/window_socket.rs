//! The sockets the windows of an instance answer on, one for each window by
//! the number it holds: an `adroit` opened again asks one to come to the
//! front, and `adroit editor` offers each the input a session handed its
//! external editor.
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use adroit_model::paths::{window_socket, windows_dir};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Request {
    /// Take the text Claude Code handed its external editor in `path`.
    Editor { path: PathBuf },
    /// Come to the front, for adroit opened again.
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

/// How long a window is given to answer before it is passed over.
const ANSWER_WITHIN: Duration = Duration::from_secs(5);

/// The folder the windows of this process's instance answer in.
pub fn windows() -> PathBuf {
    windows_dir(&adroit_model::paths::instance_state())
}

/// Send one request to the window answering on `socket` and read its answer.
pub fn ask(socket: &Path, request: &Request) -> std::io::Result<Response> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(ANSWER_WITHIN))?;
    stream.set_write_timeout(Some(ANSWER_WITHIN))?;
    let mut line = serde_json::to_string(request).map_err(std::io::Error::other)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply)?;
    serde_json::from_str(&reply).map_err(std::io::Error::other)
}

/// The numbers of the windows open in `windows`, lowest first: those whose
/// socket takes a connection.
pub fn open_numbers(windows: &Path) -> Vec<u32> {
    let mut numbers: Vec<u32> = std::fs::read_dir(windows)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".sock")?.parse().ok())
        .filter(|n| UnixStream::connect(window_socket(windows, *n)).is_ok())
        .collect();
    numbers.sort_unstable();
    numbers
}

/// Ask each window open in `windows`, lowest number first, until one gives
/// an answer `wanted` takes; that answer.
pub fn ask_each(
    windows: &Path,
    request: &Request,
    wanted: impl Fn(&Response) -> bool,
) -> Option<Response> {
    open_numbers(windows)
        .into_iter()
        .filter_map(|n| ask(&window_socket(windows, n), request).ok())
        .find(|answer| wanted(answer))
}

/// Whether no window open in `windows` holds a number lower than `number`.
pub fn first_open(windows: &Path, number: u32) -> bool {
    open_numbers(windows).first().is_none_or(|n| *n >= number)
}

/// A window this process opens: the number it holds and the socket it
/// answers on.
#[derive(Debug)]
pub struct Claimed {
    pub number: u32,
    pub listener: UnixListener,
}

/// Claim the lowest number no open window in `windows` holds, for a window
/// about to open. Unless `new`, an open window is asked to come to the front
/// instead, and nothing is claimed. A socket left by a window that has gone
/// is taken over; one another window still answers on is left alone.
/// Windows opening together take turns, so no two claim one number.
#[cfg(test)]
pub fn claim(windows: &Path, new: bool) -> std::io::Result<Option<Claimed>> {
    claim_with(windows, new, || Ok(()))
}

fn claim_with(
    windows: &Path,
    new: bool,
    before_first: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<Option<Claimed>> {
    std::fs::create_dir_all(windows)?;
    // The folder is locked, not a file beside the sockets: a lock file is
    // what a cleaner of old files takes away, and two windows then hold
    // different locks.
    let turn = std::fs::File::open(windows)?;
    turn.lock()?;
    if !new && ask_each(windows, &Request::Show, |r| *r == Response::Shown).is_some() {
        return Ok(None);
    }
    if open_numbers(windows).is_empty() {
        before_first()?;
    }
    for number in 1.. {
        let socket = window_socket(windows, number);
        if UnixStream::connect(&socket).is_ok() {
            continue;
        }
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket)?;
        return Ok(Some(Claimed { number, listener }));
    }
    unreachable!("a number is free")
}

/// The window this process opened, once it claimed a number.
static CLAIMED: OnceLock<u32> = OnceLock::new();
/// Its socket, until the window starts answering on it.
static LISTENER: Mutex<Option<UnixListener>> = Mutex::new(None);

/// Open a window of this process's instance: claim a number for it, or,
/// unless `new`, bring an open window to the front instead. Whether this
/// process is to open the window.
pub fn open_window(new: bool) -> std::io::Result<bool> {
    let active = adroit_model::paths::instance_state().join("execution.toml");
    let Some(claimed) = claim_with(&windows(), new, || {
        if adroit_connect::profiles::active()
            .map_err(std::io::Error::other)?
            .is_none()
        {
            adroit_connect::activate_saved(&active).map_err(std::io::Error::other)?;
        }
        Ok(())
    })?
    else {
        return Ok(false);
    };
    let _ = CLAIMED.set(claimed.number);
    *LISTENER.lock().unwrap_or_else(|p| p.into_inner()) = Some(claimed.listener);
    Ok(true)
}

/// The number this process's window holds; 1 for one that claimed none.
pub(crate) fn number() -> u32 {
    CLAIMED.get().copied().unwrap_or(1)
}

/// Where this process's window keeps `name` from one run to the next, under
/// the number it holds.
pub(crate) fn kept_path(name: &str) -> PathBuf {
    adroit_model::paths::window_state(&windows(), number()).join(name)
}

/// The socket this process's window answers on, taken once.
pub(crate) fn take_listener() -> Option<UnixListener> {
    LISTENER.lock().unwrap_or_else(|p| p.into_inner()).take()
}

/// A request the window is asked, with where to send its answer.
pub type Asked = (Request, oneshot::Sender<Response>);

/// Read requests from every connection to `listener`, each on its own task,
/// so a slow caller holds up no other. The window answers them in order.
pub fn serve(listener: UnixListener) -> std::io::Result<mpsc::UnboundedReceiver<Asked>> {
    listener.set_nonblocking(true)?;
    let listener = tokio::net::UnixListener::from_std(listener)?;
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(reply(stream, tx.clone()));
        }
    });
    Ok(rx)
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

    fn temp_windows(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("adroit-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("windows")
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

    fn claimed(windows: &Path) -> Claimed {
        claim(windows, true).unwrap().expect("a new window claims")
    }

    /// A window answering every request with `answer`.
    fn answering(claimed: Claimed, answer: Response) {
        let mut asked = serve(claimed.listener).unwrap();
        tokio::spawn(async move {
            while let Some((_, reply)) = asked.recv().await {
                let _ = reply.send(answer.clone());
            }
        });
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_request_crosses_the_socket_and_its_answer_comes_back() {
        let windows = temp_windows("ask");
        let mut asked = serve(claimed(&windows).listener).unwrap();
        let window = tokio::spawn(async move {
            let (req, reply) = asked.recv().await.unwrap();
            assert_eq!(req, Request::Show);
            reply.send(Response::Shown).unwrap();
        });
        let socket = window_socket(&windows, 1);
        let got = tokio::task::spawn_blocking(move || ask(&socket, &Request::Show))
            .await
            .unwrap()
            .unwrap();
        window.await.unwrap();
        assert_eq!(got, Response::Shown);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_caller_that_says_nothing_holds_up_no_other() {
        let windows = temp_windows("idle");
        answering(claimed(&windows), Response::Shown);
        let _idle = UnixStream::connect(window_socket(&windows, 1)).unwrap();
        let socket = window_socket(&windows, 1);
        let got = tokio::task::spawn_blocking(move || ask(&socket, &Request::Show))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got, Response::Shown);
    }

    #[tokio::test]
    async fn a_number_left_by_a_window_gone_is_taken_again_but_a_live_one_is_not() {
        let windows = temp_windows("stale");
        drop(stale_socket(&window_socket(&windows, 1)));
        assert_eq!(claimed(&windows).number, 1);

        let windows = temp_windows("live");
        let _first = claimed(&windows);
        assert_eq!(claimed(&windows).number, 2);
    }

    #[tokio::test]
    async fn the_stale_fixture_can_be_replaced_while_a_child_inherits_its_descriptor() {
        let windows = temp_windows("inherited-stale");
        let first = stale_socket(&window_socket(&windows, 1));
        let replacement = while_a_child_is_before_exec(|| {
            drop(first);
            claim(&windows, true)
        });
        assert_eq!(replacement.unwrap().unwrap().number, 1);
    }

    #[tokio::test]
    async fn a_live_listener_inherited_by_a_child_is_not_unlinked() {
        let windows = temp_windows("inherited-live");
        let first = claimed(&windows);
        let path = window_socket(&windows, 1);
        let inode = std::fs::metadata(&path).unwrap().ino();
        let (replacement, connection, metadata) = while_a_child_is_before_exec(|| {
            drop(first);
            (
                claim(&windows, true),
                UnixStream::connect(&path),
                std::fs::metadata(&path),
            )
        });
        assert_eq!(replacement.unwrap().unwrap().number, 2);
        connection.unwrap();
        assert_eq!(metadata.unwrap().ino(), inode);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn windows_opening_together_each_hold_a_number_of_their_own() {
        const WINDOWS: usize = 8;
        for round in 0..20 {
            let windows = temp_windows(&format!("race-{round}"));
            let start = std::sync::Arc::new(std::sync::Barrier::new(WINDOWS));
            let opening: Vec<_> = (0..WINDOWS)
                .map(|_| {
                    let (windows, start) = (windows.clone(), start.clone());
                    tokio::task::spawn_blocking(move || {
                        start.wait();
                        claim(&windows, true)
                    })
                })
                .collect();
            let mut numbers = Vec::new();
            let mut held = Vec::new();
            for w in opening {
                let claimed = w.await.unwrap().unwrap().unwrap();
                numbers.push(claimed.number);
                held.push(claimed);
            }
            numbers.sort_unstable();
            assert_eq!(
                numbers,
                (1..=WINDOWS as u32).collect::<Vec<_>>(),
                "round {round}"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn opening_again_brings_an_open_window_forward_and_a_new_window_opens_beside_it() {
        let windows = temp_windows("again");
        answering(claimed(&windows), Response::Shown);
        let w = windows.clone();
        let again = tokio::task::spawn_blocking(move || claim(&w, false))
            .await
            .unwrap()
            .unwrap();
        assert!(again.is_none(), "the open window came forward");
        let w = windows.clone();
        let new = tokio::task::spawn_blocking(move || claim(&w, true))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(new.map(|c| c.number), Some(2));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn each_window_is_asked_in_turn_until_one_takes_what_is_offered() {
        let windows = temp_windows("each");
        answering(claimed(&windows), Response::Declined);
        answering(claimed(&windows), Response::Taken(2));
        let w = windows.clone();
        let got = tokio::task::spawn_blocking(move || {
            ask_each(
                &w,
                &Request::Editor {
                    path: "/tmp/p.md".into(),
                },
                |r| matches!(r, Response::Taken(_)),
            )
        })
        .await
        .unwrap();
        assert_eq!(got, Some(Response::Taken(2)));
    }

    #[tokio::test]
    async fn the_window_holding_the_lowest_number_open_is_the_first() {
        let windows = temp_windows("first");
        assert!(first_open(&windows, 1), "no window is open");
        let one = claimed(&windows);
        let two = claimed(&windows);
        assert!(first_open(&windows, one.number));
        assert!(!first_open(&windows, two.number));

        // A socket a window left behind holds no number.
        let windows = temp_windows("first-gone");
        let _gone = stale_socket(&window_socket(&windows, 1));
        let _two = UnixListener::bind(window_socket(&windows, 2)).unwrap();
        assert!(first_open(&windows, 2));
    }
}
#[test]
fn a_next_connection_waits_until_every_old_window_has_closed() {
    let dir = tempfile::Builder::new()
        .prefix("adroit-target-")
        .tempdir_in("/tmp")
        .unwrap();
    let active = adroit_connect::execution_path(dir.path(), "default").unwrap();
    let windows = active.parent().unwrap().join("windows");
    let remote = adroit_connect::Target::Native {
        address: "work.local:47130".into(),
        server: "ab".repeat(32),
    };
    remote.save_next(&active).unwrap();
    let activate = || adroit_connect::activate_saved(&active).map_err(std::io::Error::other);
    let first = claim_with(&windows, true, activate).unwrap().unwrap();
    assert_eq!(adroit_connect::Target::load(&active).unwrap(), remote);
    adroit_connect::Target::Local.save_next(&active).unwrap();
    let second = claim_with(&windows, true, activate).unwrap().unwrap();
    drop(first);
    let third = claim_with(&windows, true, activate).unwrap().unwrap();
    assert_eq!(
        adroit_connect::Target::load(&active).unwrap(),
        remote,
        "filling a window-number hole activated the next choice"
    );
    drop(second);
    drop(third);
    // A parallel pre-exec fixture may briefly inherit these listeners.
    let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !open_numbers(&windows).is_empty() {
        assert!(
            std::time::Instant::now() < until,
            "closed windows stayed live"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let _reopened = claim_with(&windows, true, activate).unwrap().unwrap();
    assert_eq!(
        adroit_connect::Target::load(&active).unwrap(),
        adroit_connect::Target::Local
    );
}
