//! The work host, `adroit host`: the process apart from the terminal host
//! that holds the work, starts and lists the sessions, lets sessions start
//! and message sessions and judges how each stands, so that it can be
//! replaced without ending a session.

use std::path::{Path, PathBuf};
use std::time::Duration;

use adroit_protocol::client::{ask_stop, hello};
use adroit_protocol::{Hello, SPOKEN, WorkHost, judge};

use crate::cli::HostCommand;
use crate::paths::instance_state;
use crate::serve::Served;
use crate::standing::build;

/// `$ADROIT_HOST`, or `host/host.sock` under the instance's state folder.
pub fn socket() -> PathBuf {
    std::env::var_os("ADROIT_HOST")
        .map(PathBuf::from)
        .unwrap_or_else(|| instance_state().join("host").join("host.sock"))
}

/// The arguments that have adroit serve the work host at its socket.
pub fn serve_args() -> Vec<String> {
    vec!["host".into()]
}

/// What this build says first on every connection.
pub fn hello_of_ours() -> Hello {
    Hello {
        speaks: SPOKEN.to_vec(),
        build: build(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Term(#[from] adroit_term::Error),
    #[error("a work host already serves at {0}")]
    Taken(PathBuf),
    #[error("the work host at {0} did not start")]
    NotStarted(PathBuf),
}

pub type Result<T> = std::result::Result<T, Error>;

/// A work host's socket, bound and answering until asked to stop.
pub struct WorkHostServer {
    listener: tokio::net::UnixListener,
    socket: PathBuf,
}

impl Drop for WorkHostServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket);
    }
}

impl WorkHostServer {
    /// Listen at `socket`, in a folder open to no one else; refused where
    /// another work host answers.
    pub async fn bind(socket: impl AsRef<Path>) -> Result<Self> {
        let socket = socket.as_ref();
        adroit_term::own_dir(socket)?;
        if tokio::net::UnixStream::connect(socket).await.is_ok() {
            return Err(Error::Taken(socket.to_path_buf()));
        }
        // What is left is a socket of a host gone before.
        match std::fs::remove_file(socket) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let listener = tokio::net::UnixListener::bind(socket)?;
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(Self {
            listener,
            socket: socket.to_path_buf(),
        })
    }

    /// Say `hello` to everyone who connects and answer them from
    /// `served`, until one asks this host to stop.
    pub async fn serve(&self, hello: Hello, served: Served) {
        let (stop, mut stopped) = tokio::sync::mpsc::channel::<()>(1);
        loop {
            let stream = tokio::select! {
                accepted = self.listener.accept() => match accepted {
                    Ok((stream, _)) => stream,
                    Err(_) => return,
                },
                _ = stopped.recv() => return,
            };
            let (hello, served, stop) = (hello.clone(), served.clone(), stop.clone());
            tokio::spawn(async move { served.connection(stream, &hello, stop).await });
        }
    }
}

/// How the work host at `socket` stands against this build.
pub async fn standing(socket: &Path) -> WorkHost {
    let said = hello(socket).await;
    judge(said.as_ref().map(Option::as_ref), &build())
}

/// Stop the work host at `socket` and wait until it is gone; whether one was
/// listening to stop. No session ends with it.
pub async fn stop(socket: &Path) -> Result<bool> {
    Ok(adroit_term::stop_listening(socket, || async { Ok(ask_stop(socket).await?) }).await?)
}

/// Start `program` with `args` apart from adroit as the work host at
/// `socket`, and wait until it says hello.
pub async fn start(socket: &Path, program: &Path, args: &[String]) -> Result<()> {
    adroit_term::own_dir(socket)?;
    adroit_term::detached(program, args)?;
    for _ in 0..100 {
        if let Some(Some(_)) = hello(socket).await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(Error::NotStarted(socket.to_path_buf()))
}

/// Make sure a work host of this build answers at `socket`: start one where
/// none does, and replace one of another build, or one this build cannot
/// work with, without asking; whether one was started.
pub async fn ensure(socket: &Path, program: &Path, args: &[String]) -> Result<bool> {
    match standing(socket).await {
        WorkHost::Keep => return Ok(false),
        WorkHost::Replace => {
            stop(socket).await?;
        }
        WorkHost::Absent => {}
    }
    start(socket, program, args).await?;
    Ok(true)
}

/// This program, as the work host is started with.
pub fn adroit_program() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| "adroit".into())
}

/// How long the work host waits before trying again to take delegation over
/// from a terminal host of a build that still serves it.
const CLAIM_AGAIN: Duration = Duration::from_secs(2);

/// Serve delegation and judge sessions, for as long as the future is polled;
/// until the state can be claimed, try again now and then, as a terminal
/// host of a build from before the work host still holds it.
async fn delegation() {
    let mut told = false;
    let _service = loop {
        match crate::delegation::service::Service::start(
            &crate::delegation::app::root(),
            crate::daemon::daemon(),
            adroit_program(),
            crate::settings::try_load,
        )
        .await
        {
            Ok(service) => break service,
            Err(error) => {
                if !told {
                    eprintln!("adroit: sessions cannot start or message sessions yet: {error}");
                    told = true;
                }
                tokio::time::sleep(CLAIM_AGAIN).await;
            }
        }
    };
    std::future::pending::<()>().await;
}

/// Serve as the work host at `socket` until asked to stop: the work, read
/// from the configuration first and then kept read once the terminal host
/// the sessions are listed from runs.
async fn serve(socket: &Path) -> Result<()> {
    let server = WorkHostServer::bind(socket).await?;
    let host = crate::work::Host::new(crate::work::Work::configured(crate::config::read()));
    let judged = std::sync::Arc::new(tokio::sync::watch::Sender::new(None));
    let served = Served::new(host, |watched| crate::asks::asks(watched, judged.clone()));
    let watched = served.watched();
    crate::watch::start_config(watched.clone());
    tokio::spawn(async move {
        crate::terminal_host::start(&judged).await;
        crate::watch::start(watched);
    });
    tokio::select! {
        () = server.serve(hello_of_ours(), served) => {}
        () = delegation() => {}
    }
    Ok(())
}

/// `adroit host`: serve as the work host at the socket until stopped, or
/// stop or restart the work host serving there. Returns the process exit
/// code.
pub fn cli(asked: HostCommand) -> i32 {
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("adroit: {e}");
            return 1;
        }
    };
    let socket = socket();
    let done = rt.block_on(async {
        match asked {
            HostCommand::Serve => serve(&socket).await,
            HostCommand::Stop => {
                let stopped = stop(&socket).await?;
                println!(
                    "{}",
                    if stopped {
                        "stopped the work host"
                    } else {
                        "no work host was running"
                    }
                );
                Ok(())
            }
            HostCommand::Restart => {
                stop(&socket).await?;
                start(&socket, &adroit_program(), &serve_args()).await?;
                println!("restarted the work host");
                Ok(())
            }
        }
    });
    // Tasks still running once the host has stopped serving are not waited
    // for: the process is to end with it.
    rt.shutdown_background();
    match done {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("adroit: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use adroit_protocol::{
        Answer, Ask, Change, Event, PromptChange, Request, Seq, Version, WorkPatch, decode, encode,
    };

    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    use super::*;

    /// A folder for a socket, whose path has to be short.
    fn short_dir() -> tempfile::TempDir {
        let dir = tempfile::Builder::new()
            .prefix("adroit-")
            .tempdir_in("/tmp")
            .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        dir
    }

    fn hello_of(build: &str, speaks: &[u32]) -> Hello {
        Hello {
            speaks: speaks.iter().copied().map(Version).collect(),
            build: build.into(),
        }
    }

    /// A host holding no work, answering every ask as done.
    fn served() -> Served {
        Served::new(crate::work::Host::default(), |_| {
            std::sync::Arc::new(|_| Box::pin(async { Answer::Done }))
        })
    }

    #[tokio::test]
    async fn a_work_host_says_its_versions_and_build_before_it_is_asked_anything() {
        let dir = short_dir();
        let socket = dir.path().join("h.sock");
        let server = WorkHostServer::bind(&socket).await.unwrap();
        let said = hello_of("0.3.0", &[1, 2]);
        let serving = {
            let said = said.clone();
            async move { server.serve(said, served()).await }
        };
        tokio::select! {
            () = serving => panic!("the host stopped unasked"),
            got = hello(&socket) => assert_eq!(got, Some(Some(said))),
        }
    }

    #[tokio::test]
    async fn where_no_work_host_listens_none_says_anything() {
        let dir = short_dir();
        assert_eq!(hello(&dir.path().join("h.sock")).await, None);
        assert_eq!(standing(&dir.path().join("h.sock")).await, WorkHost::Absent);
    }

    #[tokio::test]
    async fn a_work_host_of_this_build_is_kept_and_one_of_another_is_to_be_replaced() {
        let dir = short_dir();
        let socket = dir.path().join("h.sock");
        let server = WorkHostServer::bind(&socket).await.unwrap();
        let ours = hello_of_ours();
        tokio::select! {
            () = server.serve(ours.clone(), served()) => panic!("the host stopped unasked"),
            got = standing(&socket) => assert_eq!(got, WorkHost::Keep),
        }
        let other = hello_of("0.0.1", &[1]);
        tokio::select! {
            () = server.serve(other.clone(), served()) => panic!("the host stopped unasked"),
            got = standing(&socket) => assert_eq!(got, WorkHost::Replace),
        }
    }

    #[tokio::test]
    async fn a_listener_that_says_nothing_is_a_work_host_to_replace() {
        let dir = short_dir();
        let socket = dir.path().join("h.sock");
        let _mute = tokio::net::UnixListener::bind(&socket).unwrap();
        assert_eq!(standing(&socket).await, WorkHost::Replace);
    }

    #[tokio::test]
    async fn a_work_host_asked_to_stop_stops_serving_and_leaves_no_socket() {
        let dir = short_dir();
        let socket = dir.path().join("h.sock");
        let server = WorkHostServer::bind(&socket).await.unwrap();
        let serving = tokio::spawn(async move {
            server.serve(hello_of_ours(), served()).await;
        });
        let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
        stream.write_all(b"{\"op\":\"stop\"}\n").await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), serving)
            .await
            .expect("the host went on serving")
            .unwrap();
        assert!(!socket.exists());
    }

    #[tokio::test]
    async fn a_work_host_asked_to_stop_by_one_who_hangs_up_at_once_still_stops() {
        let dir = short_dir();
        let socket = dir.path().join("h.sock");
        let server = WorkHostServer::bind(&socket).await.unwrap();
        let serving = tokio::spawn(async move {
            server.serve(hello_of_ours(), served()).await;
        });
        {
            let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
            stream.write_all(b"{\"op\":\"stop\"}\n").await.unwrap();
        }
        tokio::time::timeout(Duration::from_secs(5), serving)
            .await
            .expect("the host went on serving")
            .unwrap();
    }

    #[tokio::test]
    async fn asking_a_work_host_to_stop_returns_once_it_has_taken_the_request() {
        let dir = short_dir();
        let socket = dir.path().join("h.sock");
        let server = WorkHostServer::bind(&socket).await.unwrap();
        let serving = tokio::spawn(async move {
            server.serve(hello_of_ours(), served()).await;
        });
        ask_stop(&socket).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), serving)
            .await
            .expect("the host went on serving")
            .unwrap();
    }

    #[tokio::test]
    async fn a_second_work_host_refuses_to_serve_where_one_answers() {
        let dir = short_dir();
        let socket = dir.path().join("h.sock");
        let _first = WorkHostServer::bind(&socket).await.unwrap();
        let second = WorkHostServer::bind(&socket).await;
        assert!(matches!(second, Err(Error::Taken(_))));
    }

    #[tokio::test]
    async fn a_socket_left_by_a_work_host_gone_before_is_taken_over() {
        let dir = short_dir();
        let socket = dir.path().join("h.sock");
        drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());
        assert!(socket.exists());
        assert!(WorkHostServer::bind(&socket).await.is_ok());
    }

    /// One connected to a work host, as a window or a command is.
    struct Peer {
        lines: tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
        write: tokio::net::unix::OwnedWriteHalf,
    }

    impl Peer {
        /// Connected to the host at `socket`, its hello read.
        async fn to(socket: &Path) -> Self {
            let stream = tokio::net::UnixStream::connect(socket).await.unwrap();
            let (read, write) = stream.into_split();
            let mut peer = Self {
                lines: BufReader::new(read).lines(),
                write,
            };
            let hello: Hello = decode(peer.line().await).unwrap();
            assert_eq!(hello, hello_of_ours());
            peer
        }

        async fn line(&mut self) -> String {
            tokio::time::timeout(Duration::from_secs(5), self.lines.next_line())
                .await
                .expect("the host said nothing")
                .unwrap()
                .expect("the host hung up")
        }

        async fn ask(&mut self, request: &Request) {
            self.write
                .write_all(encode(request).as_bytes())
                .await
                .unwrap();
        }

        async fn told(&mut self) -> Event {
            decode(self.line().await).unwrap()
        }
    }

    fn greeting() -> Request {
        Request::Hello {
            versions: SPOKEN.to_vec(),
        }
    }

    /// A work host serving `served` at a socket of its own.
    async fn serving(served: Served) -> (tempfile::TempDir, PathBuf) {
        let dir = short_dir();
        let socket = dir.path().join("h.sock");
        let server = WorkHostServer::bind(&socket).await.unwrap();
        tokio::spawn(async move { server.serve(hello_of_ours(), served).await });
        (dir, socket)
    }

    fn composed(asked: u64) -> Vec<Change> {
        vec![Change::Prompt(PromptChange::Composed {
            composed: adroit_protocol::Composed {
                asked,
                ..Default::default()
            },
        })]
    }

    #[tokio::test]
    async fn a_window_is_welcomed_with_the_work_and_then_told_each_change_in_order() {
        let served = served();
        let watched = served.watched();
        let (_dir, socket) = serving(served).await;
        let mut window = Peer::to(&socket).await;
        window.ask(&greeting()).await;
        assert_eq!(
            window.told().await,
            Event::Welcome {
                version: *SPOKEN.iter().max().unwrap()
            }
        );
        assert_eq!(window.told().await, Event::Snapshot(Box::default()));
        watched.record(composed(1));
        watched.record(composed(2));
        for seq in 1..=2 {
            assert_eq!(
                window.told().await,
                Event::Patch(WorkPatch {
                    seq: Seq(seq),
                    changes: composed(seq),
                })
            );
        }
    }

    #[tokio::test]
    async fn the_host_answers_a_window_on_its_connection_in_the_order_asked() {
        let (_dir, socket) = serving(served()).await;
        let mut window = Peer::to(&socket).await;
        window.ask(&greeting()).await;
        window.ask(&Request::Snapshot).await;
        let mut got = Vec::new();
        for _ in 0..3 {
            got.push(window.told().await);
        }
        assert_eq!(
            got,
            vec![
                Event::Welcome {
                    version: *SPOKEN.iter().max().unwrap()
                },
                Event::Snapshot(Box::default()),
                Event::Snapshot(Box::default()),
            ]
        );
    }

    #[tokio::test]
    async fn a_closed_window_does_not_stop_the_host() {
        let served = served();
        let watched = served.watched();
        let (_dir, socket) = serving(served).await;
        {
            let mut gone = Peer::to(&socket).await;
            gone.ask(&greeting()).await;
            gone.told().await;
        }
        watched.record(composed(1));
        let mut window = Peer::to(&socket).await;
        window.ask(&Request::Snapshot).await;
        assert!(matches!(window.told().await, Event::Snapshot(s) if s.seq == Seq(1)));
    }

    #[tokio::test]
    async fn an_ask_is_answered_by_its_number_to_the_one_that_asked_alone() {
        let served = Served::new(crate::work::Host::default(), |_| {
            std::sync::Arc::new(|ask| {
                Box::pin(async move {
                    match ask {
                        Ask::KeepImage { mime, .. } => Answer::Failed { text: mime },
                        _ => Answer::Done,
                    }
                })
            })
        });
        let (_dir, socket) = serving(served).await;
        let mut other = Peer::to(&socket).await;
        other.ask(&greeting()).await;
        other.told().await;
        other.told().await;
        let mut asker = Peer::to(&socket).await;
        asker
            .ask(&Request::Ask {
                asked: 7,
                ask: Box::new(Ask::KeepImage {
                    mime: "image/tiff".into(),
                    data: String::new(),
                }),
            })
            .await;
        assert_eq!(
            asker.told().await,
            Event::Answer {
                asked: 7,
                answer: Answer::Failed {
                    text: "image/tiff".into()
                },
            }
        );
        other.ask(&Request::Snapshot).await;
        assert!(matches!(other.told().await, Event::Snapshot(_)));
    }
}
