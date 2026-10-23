//! The felis daemon sessions run in: serving it, and the one-shot asks
//! made of it, each over a connection of its own.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use felis_client_core::{AttachIntent, ConnectError, Connection, Offer, connect};
use felis_daemon::SessionPool;
use felis_daemon::serve::DaemonCaps;
use felis_protocol::MessageKind;
use felis_protocol::messages::{
    ConnToClientMsg, GridMsg, RegionSource, RegionToClientMsg, RegionToDaemonMsg, RequestedDims,
    SpawnArgs, StopMode,
};
use felis_transport::local::{ReadHalf, WriteHalf};
use felis_transport::{Delivery, Endpoint, Incoming};

use crate::input::Event as _;
use crate::screen::{Overrides, screen_of_lines};
use crate::{Input, Palette, Screen};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Connect(#[from] ConnectError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("cannot find {0} to start")]
    NotFound(String),
    #[error("the daemon at {0} did not start")]
    NotStarted(PathBuf),
    #[error("{0} is not a folder of yours alone, as the daemon's socket needs")]
    OpenFolder(PathBuf),
    #[error("{0}")]
    Protocol(String),
    #[error("startup preparation was refused and terminal {session:032x} was stopped: {reason}")]
    PreparationAborted { session: u128, reason: String },
    #[error("the daemon, process {0}, did not stop")]
    NotStopped(u32),
    #[error("{0} never reaches a program")]
    Unsent(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// A session as the daemon lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: u128,
    pub tags: Vec<String>,
    /// The terminal title its program set.
    pub title: Option<String>,
    /// Its program has exited; the daemon lets the session go shortly.
    pub exited: bool,
    /// What its program last reported of itself, by OSC 7501.
    pub status: Option<Status>,
    /// When it was made, in the order the daemon made sessions.
    pub sequence: u64,
}

/// A program's report of how it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// `working`, `blocked`, `done`, `idle` or `error`; any other is unknown.
    pub state: String,
    /// What a `blocked` program waits on: `permission`, `question`, `auth`.
    pub kind: Option<String>,
    pub msg: Option<String>,
}

/// What to start a session with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spawn {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// Set over adroit's own environment, which the program inherits.
    pub env: Vec<(String, String)>,
    pub tags: Vec<String>,
    pub cols: u16,
    pub rows: u16,
}

/// The daemon listening at a socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Daemon {
    socket: PathBuf,
}

/// The id as the daemon takes it in a request.
pub(crate) fn hex(id: u128) -> String {
    format!("{id:032x}")
}

impl Daemon {
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
        }
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// Serve sessions at `socket` until the process ends, telling `running`
    /// every `every` how many of them have a program still running.
    pub async fn serve(
        socket: &Path,
        every: Duration,
        mut running: impl FnMut(usize) + Send + 'static,
    ) -> Result<()> {
        own_dir(socket)?;
        let pool = Arc::new(tokio::sync::Mutex::new(SessionPool::new()));
        // felis tells no one of a session starting, so the pool is counted
        // on a timer rather than on its changes.
        let counted = Arc::clone(&pool);
        let counting = tokio::spawn(async move {
            let mut tick = tokio::time::interval(every);
            loop {
                tick.tick().await;
                let handles = counted.lock().await.live_handles();
                running(handles.iter().filter(|h| !h.meta_snapshot().exited).count());
            }
        });
        let served =
            felis_daemon::serve_unix(Endpoint::unix(socket), DaemonCaps::default(), pool).await;
        counting.abort();
        served.map_err(|e| Error::Protocol(e.to_string()))
    }

    /// Make sure a daemon answers, starting `program` with `args` apart
    /// from adroit when none does.
    pub async fn start(&self, program: &Path, args: &[String]) -> Result<()> {
        if self.ops().await.is_ok() {
            return Ok(());
        }
        own_dir(&self.socket)?;
        detached(program, args)?;
        for _ in 0..100 {
            if self.ops().await.is_ok() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Err(Error::NotStarted(self.socket.clone()))
    }

    /// The process listening at the socket, whatever it answers, if any does.
    pub fn pid(&self) -> Option<u32> {
        let stream = std::os::unix::net::UnixStream::connect(&self.socket).ok()?;
        peer_pid(&stream)
    }

    /// Stop the daemon, ending every session it holds, and wait until its
    /// process is gone; whether one was listening to stop.
    pub async fn stop(&self) -> Result<bool> {
        let Some(pid) = self.pid() else {
            return Ok(false);
        };
        // A daemon from before felis could be asked to stop, or one too busy
        // to answer, is signalled instead.
        let asked = tokio::time::timeout(ASK_STOP, async {
            self.ops().await?.daemon_stop(StopMode::Force).await?;
            Ok::<_, Error>(())
        })
        .await;
        if matches!(asked, Ok(Ok(()))) && gone(pid, STOPPING).await {
            return Ok(true);
        }
        for signal in [libc::SIGTERM, libc::SIGKILL] {
            // SAFETY: kill(2) only sends a signal to the process named.
            unsafe { libc::kill(pid as libc::pid_t, signal) };
            if gone(pid, STOPPING).await {
                return Ok(true);
            }
        }
        Err(Error::NotStopped(pid))
    }

    /// Stop the daemon if one listens, then start `program` with `args` as
    /// [`Daemon::start`] does.
    pub async fn restart(&self, program: &Path, args: &[String]) -> Result<()> {
        self.stop().await?;
        self.start(program, args).await
    }

    async fn connect(&self, offer: Offer) -> Result<Connection<ReadHalf, WriteHalf>> {
        Ok(connect(Endpoint::unix(&self.socket), offer).await?)
    }

    async fn ops(&self) -> Result<Connection<ReadHalf, WriteHalf>> {
        self.connect(Offer::ops()).await
    }

    pub(crate) async fn window(&self) -> Result<Connection<ReadHalf, WriteHalf>> {
        self.connect(Offer::window(false)).await
    }

    /// Every session the daemon holds, oldest first.
    pub async fn sessions(&self) -> Result<Vec<Session>> {
        let mut listed = self.ops().await?.list_sessions().await?;
        listed.sort_by_key(|s| s.sequence);
        Ok(listed
            .into_iter()
            .map(|s| Session {
                id: s.id,
                tags: s.tags,
                title: s.title,
                exited: s.exited,
                status: s
                    .program_status
                    .into_iter()
                    .find(|r| r.id.is_empty())
                    .map(|r| Status {
                        state: r.state,
                        kind: r.kind,
                        msg: r.msg,
                    }),
                sequence: s.sequence.get(),
            })
            .collect())
    }

    /// Start a session, returning its id.
    pub async fn spawn(&self, spawn: Spawn) -> Result<u128> {
        Ok(self.ops().await?.spawn_session(spawn_args(spawn)).await?.id)
    }

    /// End a session, hanging up its program.
    pub async fn end(&self, id: u128) -> Result<()> {
        self.ops().await?.destroy_session(hex(id)).await?;
        Ok(())
    }

    /// Change a session's tags.
    pub async fn tag(&self, id: u128, add: Vec<String>, remove: Vec<String>) -> Result<()> {
        self.ops().await?.set_tags(hex(id), add, remove).await?;
        Ok(())
    }

    /// Everything a session keeps, from its oldest line scrolled off to
    /// the bottom of its screen, in its colours, as `cols` columns wide.
    pub async fn everything(&self, id: u128, cols: u16, palette: &Palette) -> Result<Screen> {
        self.everything_in(id, cols, palette, &Overrides::default())
            .await
    }

    /// [`Daemon::everything`], with the colours its program set over
    /// `palette`.
    pub(crate) async fn everything_in(
        &self,
        id: u128,
        cols: u16,
        palette: &Palette,
        overrides: &Overrides,
    ) -> Result<Screen> {
        let rows = self.rows(id, RegionSource::Scrollback, true).await?;
        Ok(screen_of_lines(
            rows.join("\n").as_bytes(),
            cols,
            palette,
            overrides,
        ))
    }

    /// The text on a session's screen, a line a row.
    pub async fn visible(&self, id: u128) -> Result<Vec<String>> {
        self.rows(id, RegionSource::Visible, false).await
    }

    /// Send `bytes` to a session's program as typed keys, with no pane
    /// attached, returning once the session has taken them.
    pub async fn press(&self, id: u128, bytes: &[u8]) -> Result<()> {
        use felis_protocol::messages::InputMsg;
        self.send_input(id, &[InputMsg::KeyBytes(bytes.to_vec())])
            .await
    }

    /// Send `input` to a session's program in order, each key as the
    /// program's modes ask, with no pane attached, returning once the
    /// session has taken it all; nothing is sent when a key never reaches a
    /// program.
    pub async fn input(&self, id: u128, input: &[Input]) -> Result<()> {
        use felis_protocol::messages::InputMsg;
        let msgs = input
            .iter()
            .map(|i| match i {
                Input::Text(text) => Ok(InputMsg::KeyBytes(text.as_bytes().to_vec())),
                Input::Paste(text) => Ok(InputMsg::Paste(text.as_bytes().to_vec())),
                Input::Key(key) => key
                    .event()
                    .map(InputMsg::Key)
                    .ok_or_else(|| Error::Unsent(key.key.clone())),
            })
            .collect::<Result<Vec<_>>>()?;
        self.send_input(id, &msgs).await
    }

    async fn send_input(
        &self,
        id: u128,
        msgs: &[felis_protocol::messages::InputMsg],
    ) -> Result<()> {
        use felis_protocol::messages::SessionToDaemonMsg;
        let mut conn = self.ops().await?;
        conn.attach(id, AttachIntent::Deliberate).await?;
        // Input sent before the attach's burst is read can be dropped when
        // the daemon finds the connection's outbox full.
        rehydrated(&mut conn).await?;
        for msg in msgs {
            conn.writer.send_unflushed(msg).await.map_err(proto)?;
        }
        conn.input_fence().await?;
        conn.writer
            .send(&SessionToDaemonMsg::Detach)
            .await
            .map_err(proto)?;
        Ok(())
    }

    async fn rows(&self, id: u128, source: RegionSource, ansi: bool) -> Result<Vec<String>> {
        let mut conn = self.ops().await?;
        conn.attach(id, AttachIntent::Deliberate).await?;
        rehydrated(&mut conn).await?;
        conn.open_stream(&RegionToDaemonMsg::Rows {
            source,
            ansi,
            max_rows: None,
        })
        .await?;
        let mut rows = Vec::new();
        loop {
            let Some(frame) = conn.next_frame().await? else {
                return Err(Error::Protocol("the daemon hung up".into()));
            };
            match conn.driver.classify(&frame).map_err(proto)? {
                Incoming::Control(ConnToClientMsg::End { .. }) => return Ok(rows),
                Incoming::Control(ConnToClientMsg::Error { detail, .. }) => {
                    return Err(Error::Protocol(detail));
                }
                Incoming::Payload(p) if p.kind == MessageKind::Region => {
                    if let Delivery::Deliver(d) =
                        conn.driver.decode::<RegionToClientMsg>(&p).map_err(proto)?
                        && let RegionToClientMsg::Row { text, ansi: a, .. } = d.msg
                    {
                        rows.push(a.unwrap_or(text));
                    }
                }
                _ => {}
            }
        }
    }
}

/// What starts a session as the daemon takes it.
pub(crate) fn spawn_args(spawn: Spawn) -> SpawnArgs {
    SpawnArgs {
        command: spawn.program,
        args: spawn.args,
        cwd: spawn.cwd.to_string_lossy().into_owned(),
        env: spawn.env,
        dims: Some(RequestedDims {
            rows: u32::from(spawn.rows.max(1)),
            cols: u32::from(spawn.cols.max(1)),
            pixel_w: 0,
            pixel_h: 0,
        }),
        env_base: felis_client_core::env_base::capture(),
        tags: spawn.tags,
    }
}

/// Start `program` with `args` apart from this process: in a session of its
/// own, and a child of the system rather than of adroit, so neither closing
/// adroit nor ending its process tree, as a tool stopping it does, ends the
/// program.
pub(crate) fn detached(program: &Path, args: &[String]) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    // The shell leads a new session, starts the program in the background
    // and exits, leaving the program to the system.
    let mut shell = std::process::Command::new("/bin/sh");
    shell
        .arg("-c")
        .arg(r#""$0" "$@" </dev/null >/dev/null 2>&1 &"#)
        .arg(program)
        .args(args);
    // SAFETY: setsid(2) only changes the forked child's own session and is
    // safe to call between fork and exec.
    unsafe {
        shell.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let status = shell.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "cannot start {}",
            program.display()
        )))
    }
}

/// How long a daemon is given to take the request to stop.
const ASK_STOP: Duration = Duration::from_secs(2);

/// How long a daemon asked or signalled to stop is given to end.
const STOPPING: Duration = Duration::from_secs(3);

/// Whether process `pid` ends within `within`.
async fn gone(pid: u32, within: Duration) -> bool {
    let until = tokio::time::Instant::now() + within;
    loop {
        // SAFETY: signal 0 only asks whether the process exists.
        let alive = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
        if !alive {
            return true;
        }
        if tokio::time::Instant::now() >= until {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The process at the other end of `stream`.
#[cfg(target_os = "macos")]
fn peer_pid(stream: &std::os::unix::net::UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let mut pid: libc::pid_t = 0;
    let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    // SAFETY: the buffer is a pid_t and `len` says its size, as
    // LOCAL_PEERPID fills.
    let read = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_LOCAL,
            libc::LOCAL_PEERPID,
            (&raw mut pid).cast(),
            &mut len,
        )
    };
    (read == 0 && pid > 0).then_some(pid as u32)
}

/// The process at the other end of `stream`.
#[cfg(not(target_os = "macos"))]
fn peer_pid(stream: &std::os::unix::net::UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: the buffer is a ucred and `len` says its size, as
    // SO_PEERCRED fills.
    let read = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut cred).cast(),
            &mut len,
        )
    };
    (read == 0 && cred.pid > 0).then_some(cred.pid as u32)
}

/// Make the folder `socket` is in, open to no one else, as the daemon
/// listens only in such a folder; one already there has to be so.
fn own_dir(socket: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let Some(dir) = socket.parent().filter(|d| !d.as_os_str().is_empty()) else {
        return Ok(());
    };
    if let Some(above) = dir.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(above)?;
    }
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        // The umask may have taken more than the others' bits away.
        Ok(()) => std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    // An existing folder is the user's to open or close: tightening it would
    // hide whatever set it loose, and a shared one such as /tmp cannot be.
    let meta = std::fs::metadata(dir)?;
    // SAFETY: geteuid(2) has no preconditions and cannot fail.
    let me = unsafe { libc::geteuid() };
    if !meta.is_dir() || meta.uid() != me || meta.mode() & 0o777 != 0o700 {
        return Err(Error::OpenFolder(dir.to_path_buf()));
    }
    Ok(())
}

fn proto(e: impl std::fmt::Display) -> Error {
    Error::Protocol(e.to_string())
}

/// Read past the burst an attach ships with the session's modes, to the
/// frame that says it is done.
async fn rehydrated(conn: &mut Connection<ReadHalf, WriteHalf>) -> Result<()> {
    loop {
        let Some(frame) = conn.next_frame().await? else {
            return Err(Error::Protocol("the daemon hung up".into()));
        };
        let Incoming::Payload(p) = conn.driver.classify(&frame).map_err(proto)? else {
            continue;
        };
        if p.kind == MessageKind::Grid
            && let Delivery::Deliver(d) = conn.driver.decode::<GridMsg>(&p).map_err(proto)?
            && d.msg == GridMsg::RehydrateEnd
        {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_started_apart_is_the_system_s_child_in_a_session_of_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("ids");
        let executable = std::env::current_exe().unwrap();
        detached(Path::new("/bin/sh"), &[
            "-c".into(),
            "ADROIT_TEST_DETACHED_OUTPUT=\"$1\" exec \"$2\" --exact daemon::tests::detached_identity_fixture --ignored --nocapture".into(),
            "adroit-detached-test".into(),
            out.to_string_lossy().into_owned(),
            executable.to_string_lossy().into_owned(),
        ]).unwrap();
        for _ in 0..100 {
            if let Ok(text) = std::fs::read_to_string(&out)
                && !text.trim().is_empty()
            {
                let ids: Vec<&str> = text.split_whitespace().collect();
                assert_eq!(ids[0], "1", "its parent is the system: {text}");
                let own = std::process::id().to_string();
                assert_ne!(ids[0], own);
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("the program never ran");
    }

    #[test]
    #[ignore = "subprocess fixture reads only its own process identity"]
    fn detached_identity_fixture() {
        std::thread::sleep(Duration::from_millis(300));
        // SAFETY: these calls only query the current process and have no preconditions.
        let (parent, session, process) =
            unsafe { (libc::getppid(), libc::getsid(0), libc::getpid()) };
        std::fs::write(
            std::env::var_os("ADROIT_TEST_DETACHED_OUTPUT").unwrap(),
            format!("{parent} {session} {process}\n"),
        )
        .unwrap();
    }

    fn mode(dir: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(dir).unwrap().permissions().mode() & 0o777
    }

    fn with_mode(dir: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn a_folder_made_for_the_socket_is_open_to_no_one_else() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("state").join("felis");
        own_dir(&dir.join("daemon.sock")).unwrap();
        assert_eq!(mode(&dir), 0o700);
    }

    #[test]
    fn a_folder_of_the_user_s_own_is_taken_as_it_is_or_refused_but_never_changed() {
        let tmp = tempfile::tempdir().unwrap();
        with_mode(tmp.path(), 0o700);
        own_dir(&tmp.path().join("daemon.sock")).unwrap();
        with_mode(tmp.path(), 0o755);
        let refused = own_dir(&tmp.path().join("daemon.sock")).unwrap_err();
        assert!(matches!(refused, Error::OpenFolder(_)), "{refused}");
        assert_eq!(mode(tmp.path()), 0o755);
    }

    #[test]
    fn a_shared_folder_such_as_tmp_is_refused_and_left_alone() {
        let before = mode(Path::new("/tmp/"));
        let refused = own_dir(Path::new("/tmp/adroit-daemon.sock")).unwrap_err();
        assert!(matches!(refused, Error::OpenFolder(_)), "{refused}");
        assert_eq!(mode(Path::new("/tmp/")), before);
    }
}
