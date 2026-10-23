//! The work host, `adroit host`: the process apart from the terminal host
//! that lets sessions start and message sessions and judges how each stands,
//! so that it can be replaced without ending a session.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::cli::HostCommand;
use crate::paths::instance_state;
use crate::standing::{WorkHost, build, judge_work_host};

/// The versions of the work host's protocol this build speaks. A version is
/// added, and one no longer spoken dropped, only when what is sent changes
/// shape.
pub const SPEAKS: &[u32] = &[1];

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

/// What a work host says first on every connection, before it is asked
/// anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub speaks: Vec<u32>,
    pub build: String,
}

impl Hello {
    /// What this build says.
    pub fn ours() -> Self {
        Self {
            speaks: SPEAKS.to_vec(),
            build: build(),
        }
    }
}

/// What a work host is asked once it has said hello. Its shape is the same
/// in every version, so any window can replace any work host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Ask {
    Stop,
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

/// How long a work host is given to say hello.
const HELLO: Duration = Duration::from_secs(2);

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

    /// Say `hello` to everyone who connects, until one asks this host to
    /// stop.
    pub async fn serve(&self, hello: &Hello) {
        let mut line = serde_json::to_vec(hello).expect("a hello is JSON");
        line.push(b'\n');
        let (stop, mut stopped) = tokio::sync::mpsc::channel::<()>(1);
        loop {
            let stream = tokio::select! {
                accepted = self.listener.accept() => match accepted {
                    Ok((stream, _)) => stream,
                    Err(_) => return,
                },
                _ = stopped.recv() => return,
            };
            let (line, stop) = (line.clone(), stop.clone());
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                // One who asks without waiting for hello may have hung up
                // already; what it asked is read all the same.
                let _ = write.write_all(&line).await;
                let mut lines = BufReader::new(read).lines();
                while let Ok(Some(asked)) = lines.next_line().await {
                    if let Ok(Ask::Stop) = serde_json::from_str(&asked) {
                        let _ = write.write_all(b"{\"stopping\":true}\n").await;
                        let _ = stop.send(()).await;
                        return;
                    }
                }
            });
        }
    }
}

/// What the work host at `socket` says: `None` where none listens, and
/// `Some(None)` where one listens but says nothing it can be judged by.
pub async fn hello(socket: &Path) -> Option<Option<Hello>> {
    let stream = tokio::net::UnixStream::connect(socket).await.ok()?;
    let said = tokio::time::timeout(HELLO, async {
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).await.ok()?;
        serde_json::from_str::<Hello>(&line).ok()
    })
    .await;
    Some(said.ok().flatten())
}

/// How the work host at `socket` stands against this build.
pub async fn standing(socket: &Path) -> WorkHost {
    let said = hello(socket).await;
    let said = said.as_ref().map(|said| {
        said.as_ref()
            .map(|h| (h.speaks.as_slice(), h.build.as_str()))
    });
    judge_work_host(said, SPEAKS, &build())
}

/// Stop the work host at `socket` and wait until it is gone; whether one was
/// listening to stop. No session ends with it.
pub async fn stop(socket: &Path) -> Result<bool> {
    Ok(adroit_term::stop_listening(socket, || async { Ok(ask_stop(socket).await?) }).await?)
}

/// Ask the work host at `socket` to stop, returning once it has taken the
/// request.
async fn ask_stop(socket: &Path) -> std::io::Result<()> {
    let stream = tokio::net::UnixStream::connect(socket).await?;
    let (read, mut write) = stream.into_split();
    let mut asked = serde_json::to_vec(&Ask::Stop)?;
    asked.push(b'\n');
    write.write_all(&asked).await?;
    let mut lines = BufReader::new(read).lines();
    // Its hello, then its word that it stops, or its hanging up.
    while lines
        .next_line()
        .await?
        .is_some_and(|line| !line.contains("stopping"))
    {}
    Ok(())
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

/// Serve as the work host at `socket` until asked to stop.
async fn serve(socket: &Path) -> Result<()> {
    let server = WorkHostServer::bind(socket).await?;
    let hello = Hello::ours();
    tokio::select! {
        () = server.serve(&hello) => {}
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
            speaks: speaks.to_vec(),
            build: build.into(),
        }
    }

    #[tokio::test]
    async fn a_work_host_says_its_versions_and_build_before_it_is_asked_anything() {
        let dir = short_dir();
        let socket = dir.path().join("h.sock");
        let server = WorkHostServer::bind(&socket).await.unwrap();
        let said = hello_of("0.3.0", &[1, 2]);
        let serving = {
            let said = said.clone();
            async move { server.serve(&said).await }
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
        let ours = Hello::ours();
        tokio::select! {
            () = server.serve(&ours) => panic!("the host stopped unasked"),
            got = standing(&socket) => assert_eq!(got, WorkHost::Keep),
        }
        let other = hello_of("0.0.1", SPEAKS);
        tokio::select! {
            () = server.serve(&other) => panic!("the host stopped unasked"),
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
            server.serve(&Hello::ours()).await;
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
            server.serve(&Hello::ours()).await;
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
            server.serve(&Hello::ours()).await;
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
}
