//! Whether the window can work with the hosts listening, judged by the
//! versions of their protocols they say they speak, and which build each is.

use std::path::{Path, PathBuf};

use adroit_term::Daemon;

/// Which source this build was made from, as `git describe` tells it; none
/// for one built outside a checkout, which its version alone names.
pub const DESCRIBED: Option<&str> = option_env!("ADROIT_DESCRIBE");

/// The build of adroit this is, as a host says it.
pub fn build() -> String {
    build_of(env!("CARGO_PKG_VERSION"), DESCRIBED)
}

fn build_of(version: &str, described: Option<&str>) -> String {
    match described {
        Some(d) => format!("{version} ({d})"),
        None => version.into(),
    }
}

/// Whether two peers share a version of a protocol.
pub fn shares(theirs: &[u32], ours: &[u32]) -> bool {
    theirs.iter().any(|v| ours.contains(v))
}

/// How the terminal host listening stands against this build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// None listens.
    Absent,
    /// It speaks a version of the terminal protocol this build does.
    Compatible,
    /// It speaks none this build does: of the build it says, or `None` when
    /// it is too old to say which versions it speaks.
    Incompatible(Option<String>),
}

/// How the terminal host at `daemon` stands against this build.
pub fn standing(daemon: &Daemon) -> Standing {
    judge(daemon.pid(), said(daemon.socket()), adroit_term::SPEAKS)
}

/// [`standing`], giving a terminal host that has only just started a moment
/// to say its versions.
pub async fn settled(daemon: &Daemon) -> Standing {
    let mut stands = standing(daemon);
    for _ in 0..20 {
        if stands != Standing::Incompatible(None) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        stands = standing(daemon);
    }
    stands
}

/// What a terminal host said beside its socket.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Said {
    /// The process that said it.
    pid: u32,
    build: String,
    speaks: Vec<u32>,
}

/// How a terminal host stands, from the process listening and what was said
/// last beside its socket.
fn judge(listening: Option<u32>, said: Option<Said>, ours: &[u32]) -> Standing {
    let Some(pid) = listening else {
        return Standing::Absent;
    };
    match said {
        Some(said) if said.pid == pid && shares(&said.speaks, ours) => Standing::Compatible,
        Some(said) if said.pid == pid => Standing::Incompatible(Some(said.build)),
        // What another process said is left from a host gone before.
        _ => Standing::Incompatible(None),
    }
}

/// Where the terminal host at `socket` says what it speaks. A host from
/// before protocols had versions wrote its build elsewhere, and so says none.
fn says_path(socket: &Path) -> PathBuf {
    socket.with_extension("speaks")
}

/// Say beside `socket` what the process serving there speaks.
fn say(socket: &Path, said: &Said) -> std::io::Result<()> {
    let path = says_path(socket);
    // Written aside and moved in, so a window never reads it half written.
    let aside = path.with_extension(format!("speaks.{}", said.pid));
    let versions: Vec<String> = said.speaks.iter().map(u32::to_string).collect();
    std::fs::write(
        &aside,
        format!(
            "pid {}\nbuild {}\nspeaks {}\n",
            said.pid,
            said.build,
            versions.join(" ")
        ),
    )?;
    std::fs::rename(&aside, &path)
}

/// What was said last beside `socket`.
fn said(socket: &Path) -> Option<Said> {
    let text = std::fs::read_to_string(says_path(socket)).ok()?;
    let field = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(' '))
    };
    Some(Said {
        pid: field("pid")?.parse().ok()?,
        build: field("build")?.into(),
        speaks: field("speaks")?
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?,
    })
}

/// Say `build` and the terminal protocol's versions beside the socket once
/// this process listens there; nothing when another process does, as this
/// one is then about to fail.
pub(crate) async fn say_when_listening(daemon: &Daemon, build: &str, speaks: &[u32]) {
    let me = std::process::id();
    for _ in 0..500 {
        match daemon.pid() {
            Some(pid) if pid == me => {
                let said = Said {
                    pid: me,
                    build: build.into(),
                    speaks: speaks.to_vec(),
                };
                if let Err(e) = say(daemon.socket(), &said) {
                    eprintln!("adroit: cannot say what the terminal host speaks: {e}");
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

    fn said(pid: u32, build: &str, speaks: &[u32]) -> Option<Said> {
        Some(Said {
            pid,
            build: build.into(),
            speaks: speaks.to_vec(),
        })
    }

    #[test]
    fn a_build_is_named_by_its_version_and_what_git_said_of_its_source() {
        assert_eq!(build_of("0.3.0", None), "0.3.0");
        assert_eq!(
            build_of("0.3.0", Some("v0.2.0-4-gabc-dirty")),
            "0.3.0 (v0.2.0-4-gabc-dirty)"
        );
    }

    #[test]
    fn a_terminal_host_sharing_a_version_is_worked_with_whatever_its_build() {
        assert_eq!(
            judge(Some(42), said(42, "0.2.0", &[1]), &[1]),
            Standing::Compatible
        );
        assert_eq!(
            judge(Some(42), said(42, "0.4.0", &[1, 2]), &[2, 3]),
            Standing::Compatible
        );
    }

    #[test]
    fn a_terminal_host_sharing_no_version_is_one_the_window_cannot_work_with() {
        assert_eq!(
            judge(Some(42), said(42, "0.2.0", &[1]), &[2]),
            Standing::Incompatible(Some("0.2.0".into()))
        );
        assert_eq!(
            judge(Some(42), said(42, "0.2.0", &[]), &[1]),
            Standing::Incompatible(Some("0.2.0".into()))
        );
    }

    #[test]
    fn a_terminal_host_too_old_to_say_its_versions_is_one_the_window_cannot_work_with() {
        assert_eq!(judge(Some(42), None, &[1]), Standing::Incompatible(None));
        assert_eq!(
            judge(Some(42), said(7, "0.3.0", &[1]), &[1]),
            Standing::Incompatible(None)
        );
    }

    #[test]
    fn where_no_terminal_host_listens_nothing_stands() {
        assert_eq!(judge(None, said(42, "0.3.0", &[1]), &[1]), Standing::Absent);
    }

    #[test]
    fn what_a_terminal_host_says_beside_its_socket_is_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("daemon.sock");
        assert_eq!(super::said(&socket), None);
        let saying = said(42, "0.3.0 (v0.2.0-4-gabc)", &[1, 2]).unwrap();
        say(&socket, &saying).unwrap();
        assert_eq!(super::said(&socket), Some(saying));
    }

    #[test]
    fn the_build_a_host_from_before_versions_wrote_says_no_version() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("daemon.sock");
        std::fs::write(socket.with_extension("build"), "42\n0.3.0\n").unwrap();
        assert_eq!(super::said(&socket), None);
    }

    /// A folder for a socket, whose path has to be short.
    fn short_dir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("adroit-")
            .tempdir_in("/tmp")
            .unwrap()
    }

    #[tokio::test]
    async fn a_terminal_host_says_its_versions_once_it_listens_itself() {
        let dir = short_dir();
        let socket = dir.path().join("d.sock");
        let _listening = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        say_when_listening(&Daemon::new(&socket), "0.3.0", &[1]).await;
        assert_eq!(
            super::said(&socket),
            said(std::process::id(), "0.3.0", &[1])
        );
        assert_eq!(standing(&Daemon::new(&socket)), Standing::Compatible);
    }

    #[tokio::test]
    async fn a_terminal_host_just_started_is_given_a_moment_to_say_its_versions() {
        let dir = short_dir();
        let socket = dir.path().join("d.sock");
        let _listening = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let saying = {
            let socket = socket.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                let said = said(std::process::id(), &build(), adroit_term::SPEAKS).unwrap();
                say(&socket, &said).unwrap();
            })
        };
        assert_eq!(settled(&Daemon::new(&socket)).await, Standing::Compatible);
        saying.await.unwrap();
    }

    #[tokio::test]
    async fn a_terminal_host_that_finds_another_listening_says_nothing_over_it() {
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
        assert!(listened, "the other terminal host never listened");
        say_when_listening(&Daemon::new(&socket), "0.3.0", &[1]).await;
        let _ = other.kill();
        let _ = other.wait();
        assert_eq!(super::said(&socket), None);
    }

    #[test]
    #[ignore = "invoked in a subprocess as another terminal host listening"]
    fn listening_fixture() {
        let socket = std::env::var_os("ADROIT_TEST_LISTEN_SOCKET").unwrap();
        let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
        let mut held = Vec::new();
        for stream in listener.incoming() {
            held.push(stream.unwrap());
        }
    }
}
