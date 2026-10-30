//! A startup lease whose owner survives the work host.

use crate::{Error, Result, Spawn};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
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
    let listener = UnixListener::bind(socket)?;
    hold(&listener, Instant::now() + Duration::from_millis(timeout))?;
    drop(listener);
    std::fs::remove_file(socket)?;
    Err(std::process::Command::new(program).args(rest).exec())
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    address(socket)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
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
                // An expired lease has already started the program; never type into it.
                if tokio::time::Instant::now() >= deadline {
                    return Ok(());
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

    #[test]
    fn a_lease_nobody_releases_lets_the_program_go_at_its_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let listener = UnixListener::bind(dir.path().join("lease.sock")).unwrap();
        let started = Instant::now();
        assert!(!hold(&listener, started + Duration::from_millis(50)).unwrap());
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
