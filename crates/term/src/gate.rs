//! A startup lease whose owner survives the work host.

use crate::{Error, Result, Spawn};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(crate) const TAG: &str = "adroit:startup-lease=";

fn root() -> PathBuf {
    PathBuf::from(format!("/tmp/adroit-g-{}", unsafe { libc::geteuid() }))
}

fn address(path: &Path) -> std::io::Result<()> {
    if path.parent() != Some(root().as_path())
        || !path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| {
                let Some(id) = name.strip_suffix(".sock") else {
                    return false;
                };
                id.len() == 32
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
    {
        return Err(std::io::Error::other("invalid startup lease address"));
    }
    let metadata = std::fs::symlink_metadata(root())?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(std::io::Error::other(
            "startup lease directory is not private",
        ));
    }
    Ok(())
}

pub(crate) fn leased(spawn: Spawn, program: &Path, timeout: Duration) -> Result<Spawn> {
    match std::fs::DirBuilder::new().mode(0o700).create(root()) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    let mut random = [0; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    let id: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let socket = root().join(format!("{id}.sock"));
    address(&socket)?;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(pending(&socket))?;
    let socket = socket.to_string_lossy().into_owned();
    let mut tags = spawn.tags.clone();
    let previous = crate::tags::holding(&tags, TAG.trim_end_matches('='));
    tags.retain(|tag| !previous.contains(tag));
    tags.extend(crate::tags::tagged(TAG.trim_end_matches('='), &socket));
    let args = [
        "__startup_lease".into(),
        socket,
        timeout.as_millis().clamp(1, 10000).to_string(),
        spawn.program,
    ]
    .into_iter()
    .chain(spawn.args)
    .collect();
    Ok(Spawn {
        program: program.to_string_lossy().into_owned(),
        args,
        tags,
        ..spawn
    })
}

/// Run only as the held program, before the application's environment setup.
pub fn run(args: &[String]) -> i32 {
    match run_inner(args) {
        Ok(never) => match never {},
        Err(e) => {
            eprintln!("adroit: startup lease: {e}");
            1
        }
    }
}

fn run_inner(args: &[String]) -> std::io::Result<std::convert::Infallible> {
    let [socket, timeout, program, rest @ ..] = args else {
        return Err(std::io::Error::other("missing startup lease arguments"));
    };
    let socket = Path::new(socket);
    address(socket)?;
    let timeout = timeout
        .parse::<u64>()
        .ok()
        .filter(|t| (1..=10000).contains(t))
        .ok_or_else(|| std::io::Error::other("invalid startup lease deadline"))?;
    let listener = listening(socket)?;
    hold(&listener, Instant::now() + Duration::from_millis(timeout))?;
    drop(listener);
    std::fs::remove_file(socket)?;
    Err(std::process::Command::new(program).args(rest).exec())
}

/// Marks a lease made but not yet listening, so a release can tell it from
/// one that listened and expired.
fn pending(socket: &Path) -> PathBuf {
    socket.with_extension("pending")
}

fn listening(socket: &Path) -> std::io::Result<UnixListener> {
    let listener = UnixListener::bind(socket)?;
    match std::fs::remove_file(pending(socket)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(listener),
    }
}

/// Waits on `listener` until a release comes in, returning true, or until
/// `deadline`, returning false.
fn hold(listener: &UnixListener, deadline: Instant) -> std::io::Result<bool> {
    listener.set_nonblocking(true)?;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // BSD hands out accepted sockets with the listener's
                // O_NONBLOCK, under which the timeouts below never apply.
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_secs(1)))?;
                stream.set_write_timeout(Some(Duration::from_secs(1)))?;
                let mut message = [0; 8];
                if stream.read_exact(&mut message).is_ok() && message == *b"release\n" {
                    let _ = stream.write_all(b"released\n");
                    return Ok(true);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(e) => return Err(e),
        }
    }
    Ok(false)
}

pub(crate) async fn release(socket: &Path) -> Result<()> {
    // Past the longest lease deadline, so a lease that listens late is still
    // released before it would let the program go by itself.
    release_within(socket, Duration::from_secs(12)).await
}

async fn release_within(socket: &Path, wait: Duration) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    address(socket)?;
    let deadline = tokio::time::Instant::now() + wait;
    let mut gone = false;
    loop {
        match tokio::net::UnixStream::connect(socket).await {
            Ok(mut stream) => {
                return tokio::time::timeout(Duration::from_secs(1), async {
                    stream.write_all(b"release\n").await?;
                    let mut answer = [0; 9];
                    stream.read_exact(&mut answer).await?;
                    if answer != *b"released\n" {
                        return Err(std::io::Error::other("invalid startup lease answer"));
                    }
                    Ok(())
                })
                .await
                .map_err(|_| Error::Io(std::io::Error::other("startup lease release timed out")))?
                .map_err(Into::into);
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                // A lease that listened and went away has already started the
                // program; never type into it. Its marker is seen gone twice,
                // since a lease removes it only after it listens.
                let listened = !pending(socket).exists();
                if listened && gone {
                    return Ok(());
                }
                gone = listened;
                if tokio::time::Instant::now() >= deadline {
                    let _ = std::fs::remove_file(pending(socket));
                    return Err(Error::Io(std::io::Error::other(
                        "startup lease never started listening",
                    )));
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_lease_keeps_the_original_program_and_a_private_unique_release_address() {
        let original = crate::Spawn {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "echo started".into()],
            ..Default::default()
        };
        let first = leased(
            original.clone(),
            Path::new("/opt/adroit"),
            Duration::from_secs(10),
        )
        .unwrap();
        let second = leased(original, Path::new("/opt/adroit"), Duration::from_secs(10)).unwrap();
        assert_eq!(first.program, "/opt/adroit");
        assert_eq!(
            &first.args[2..],
            &["10000", "/bin/sh", "-c", "echo started"]
        );
        assert_ne!(first.args[1], second.args[1]);
        assert!(address(Path::new(&first.args[1])).is_ok());
        assert_eq!(
            crate::tags::value(&first.tags, TAG.trim_end_matches('=')),
            Some(first.args[1].clone())
        );
        assert!(address(Path::new("/tmp/arbitrary.sock")).is_err());
        for lease in [&first, &second] {
            std::fs::remove_file(pending(Path::new(&lease.args[1]))).unwrap();
        }
    }

    #[test]
    fn a_release_written_after_its_connection_still_lets_the_program_go() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("lease.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let client = std::thread::spawn(move || {
            let mut stream = std::os::unix::net::UnixStream::connect(&socket).unwrap();
            std::thread::sleep(Duration::from_millis(20));
            stream.write_all(b"release\n").unwrap();
            let mut answer = [0; 9];
            stream.read_exact(&mut answer).unwrap();
            answer
        });
        assert!(hold(&listener, Instant::now() + Duration::from_secs(5)).unwrap());
        assert_eq!(&client.join().unwrap(), b"released\n");
    }

    fn lease() -> PathBuf {
        let spawn = leased(
            crate::Spawn {
                program: "/bin/sh".into(),
                ..Default::default()
            },
            Path::new("/opt/adroit"),
            Duration::from_secs(10),
        )
        .unwrap();
        PathBuf::from(&spawn.args[1])
    }

    #[tokio::test]
    async fn a_release_waits_for_a_lease_slower_to_listen_than_two_seconds() {
        let socket = lease();
        let listening = {
            let socket = socket.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(2500));
                let listener = listening(&socket).unwrap();
                hold(&listener, Instant::now() + Duration::from_secs(5)).unwrap()
            })
        };
        release(&socket).await.unwrap();
        assert!(listening.join().unwrap());
        std::fs::remove_file(&socket).unwrap();
    }

    #[tokio::test]
    async fn a_release_of_an_expired_lease_returns_at_once() {
        let socket = lease();
        drop(listening(&socket).unwrap());
        std::fs::remove_file(&socket).unwrap();
        let started = std::time::Instant::now();
        release(&socket).await.unwrap();
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[tokio::test]
    async fn a_lease_that_never_listens_fails_its_release() {
        let socket = lease();
        assert!(
            release_within(&socket, Duration::from_millis(300))
                .await
                .is_err()
        );
        assert!(!pending(&socket).exists());
    }

    #[test]
    fn a_lease_nobody_releases_lets_the_program_go_at_its_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let listener = UnixListener::bind(dir.path().join("lease.sock")).unwrap();
        let started = Instant::now();
        assert!(!hold(&listener, started + Duration::from_millis(50)).unwrap());
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
