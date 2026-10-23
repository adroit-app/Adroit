//! Which build of adroit the daemon listening is, told apart from this one.

use std::path::{Path, PathBuf};

use adroit_term::Daemon;

/// Which source a debug build was made from, as `git describe` tells it;
/// none for a release, which its version names.
pub const DESCRIBED: Option<&str> = option_env!("ADROIT_DESCRIBE");

/// The build of adroit this is, to tell a daemon of another from.
pub fn build() -> String {
    build_of(env!("CARGO_PKG_VERSION"), DESCRIBED)
}

fn build_of(version: &str, described: Option<&str>) -> String {
    match described {
        Some(d) => format!("{version} ({d})"),
        None => version.into(),
    }
}

/// How the daemon listening stands against this build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// None listens.
    Absent,
    Current,
    /// Of another build: the one it says, or `None` when it is too old to
    /// say.
    Old(Option<String>),
}

/// How the daemon at `daemon` stands against this build.
pub fn standing(daemon: &Daemon) -> Standing {
    judge(daemon.pid(), stamped(daemon.socket()), &build())
}

/// [`standing`], giving a daemon that has only just started a moment to say
/// its build.
pub async fn settled(daemon: &Daemon) -> Standing {
    let mut stands = standing(daemon);
    for _ in 0..20 {
        if stands != Standing::Old(None) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        stands = standing(daemon);
    }
    stands
}

/// How a daemon stands, from the process listening and the build last said
/// beside its socket, by the process that said it.
fn judge(listening: Option<u32>, said: Option<(u32, String)>, build: &str) -> Standing {
    let Some(pid) = listening else {
        return Standing::Absent;
    };
    match said {
        Some((by, said)) if by == pid && said == build => Standing::Current,
        Some((by, said)) if by == pid => Standing::Old(Some(said)),
        // What another process said is left from a daemon gone before.
        _ => Standing::Old(None),
    }
}

/// Where the daemon at `socket` says which build it is.
fn stamp_path(socket: &Path) -> PathBuf {
    socket.with_extension("build")
}

/// Say beside `socket` that process `pid` serving there is `build`.
fn stamp(socket: &Path, pid: u32, build: &str) -> std::io::Result<()> {
    let path = stamp_path(socket);
    // Written aside and moved in, so a window never reads it half written.
    let aside = path.with_extension(format!("build.{pid}"));
    std::fs::write(&aside, format!("{pid}\n{build}\n"))?;
    std::fs::rename(&aside, &path)
}

/// The process and build said last beside `socket`.
fn stamped(socket: &Path) -> Option<(u32, String)> {
    let text = std::fs::read_to_string(stamp_path(socket)).ok()?;
    let (pid, build) = text.trim_end().split_once('\n')?;
    Some((pid.parse().ok()?, build.into()))
}

/// Say `build` beside the socket once this process listens there; nothing
/// when another process does, as this one is then about to fail.
pub(crate) async fn stamp_when_listening(daemon: &Daemon, build: &str) {
    let me = std::process::id();
    for _ in 0..500 {
        match daemon.pid() {
            Some(pid) if pid == me => {
                if let Err(e) = stamp(daemon.socket(), me, build) {
                    eprintln!("adroit: cannot say the daemon's build: {e}");
                }
                return;
            }
            Some(_) => return,
            None => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_is_named_by_its_version_and_what_git_said_of_its_source() {
        assert_eq!(build_of("0.3.0", None), "0.3.0");
        assert_eq!(
            build_of("0.3.0", Some("v0.2.0-4-gabc-dirty")),
            "0.3.0 (v0.2.0-4-gabc-dirty)"
        );
    }

    #[test]
    fn a_daemon_of_this_build_stands_current_and_one_of_another_stands_old() {
        let ours = Some((42, "0.3.0".to_string()));
        assert_eq!(judge(Some(42), ours.clone(), "0.3.0"), Standing::Current);
        assert_eq!(
            judge(Some(42), ours, "0.4.0"),
            Standing::Old(Some("0.3.0".into()))
        );
    }

    #[test]
    fn a_daemon_too_old_to_say_its_build_stands_old() {
        assert_eq!(judge(Some(42), None, "0.3.0"), Standing::Old(None));
        let another_process = Some((7, "0.3.0".to_string()));
        assert_eq!(
            judge(Some(42), another_process, "0.3.0"),
            Standing::Old(None)
        );
    }

    #[test]
    fn where_no_daemon_listens_nothing_stands() {
        let left = Some((42, "0.3.0".to_string()));
        assert_eq!(judge(None, left, "0.3.0"), Standing::Absent);
    }

    #[test]
    fn the_build_a_daemon_writes_beside_its_socket_is_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("daemon.sock");
        assert_eq!(stamped(&socket), None);
        stamp(&socket, 42, "0.3.0 (v0.2.0-4-gabc)").unwrap();
        assert_eq!(stamped(&socket), Some((42, "0.3.0 (v0.2.0-4-gabc)".into())));
    }

    /// A folder for a socket, whose path has to be short.
    fn short_dir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("adroit-")
            .tempdir_in("/tmp")
            .unwrap()
    }

    #[tokio::test]
    async fn a_daemon_says_its_build_once_it_listens_itself() {
        let dir = short_dir();
        let socket = dir.path().join("d.sock");
        let _listening = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        stamp_when_listening(&Daemon::new(&socket), "0.3.0").await;
        assert_eq!(stamped(&socket), Some((std::process::id(), "0.3.0".into())));
    }

    #[tokio::test]
    async fn a_daemon_just_started_is_given_a_moment_to_say_its_build() {
        let dir = short_dir();
        let socket = dir.path().join("d.sock");
        let _listening = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let saying = {
            let socket = socket.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                stamp(&socket, std::process::id(), &build()).unwrap();
            })
        };
        assert_eq!(settled(&Daemon::new(&socket)).await, Standing::Current);
        saying.await.unwrap();
    }

    #[tokio::test]
    async fn a_daemon_that_finds_another_listening_says_nothing_over_it() {
        let dir = short_dir();
        let socket = dir.path().join("d.sock");
        let mut other = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "standing::tests::listening_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("ADROIT_TEST_LISTEN_SOCKET", &socket)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut listened = false;
        for _ in 0..200 {
            if Daemon::new(&socket).pid().is_some() {
                listened = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(listened, "the other daemon never listened");
        stamp_when_listening(&Daemon::new(&socket), "0.3.0").await;
        let _ = other.kill();
        let _ = other.wait();
        assert_eq!(stamped(&socket), None);
    }

    #[test]
    #[ignore = "invoked in a subprocess as another daemon listening"]
    fn listening_fixture() {
        let socket = std::env::var_os("ADROIT_TEST_LISTEN_SOCKET").unwrap();
        let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
        let mut held = Vec::new();
        for stream in listener.incoming() {
            held.push(stream.unwrap());
        }
    }
}
