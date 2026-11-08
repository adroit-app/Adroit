//! Sockets on this machine, named by a path: Unix sockets, and on Windows
//! the named pipes such a path names, as felis takes them.

use std::path::Path;

#[cfg(unix)]
pub type Stream = tokio::net::UnixStream;

/// Either end of a pipe, its halves boxed so both ends are one type.
#[cfg(windows)]
pub type Stream = tokio::io::Join<
    Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    Box<dyn tokio::io::AsyncWrite + Send + Unpin>,
>;

#[cfg(unix)]
pub struct Listener(tokio::net::UnixListener);

#[cfg(unix)]
impl Listener {
    pub fn bind(path: impl AsRef<Path>) -> std::io::Result<Self> {
        tokio::net::UnixListener::bind(path).map(Self)
    }

    pub fn from_std(listener: std::os::unix::net::UnixListener) -> std::io::Result<Self> {
        tokio::net::UnixListener::from_std(listener).map(Self)
    }

    pub async fn accept(&self) -> std::io::Result<Stream> {
        self.0.accept().await.map(|(stream, _)| stream)
    }
}

#[cfg(unix)]
pub async fn connect(path: impl AsRef<Path>) -> std::io::Result<Stream> {
    tokio::net::UnixStream::connect(path).await
}

#[cfg(windows)]
pub struct Listener(felis_transport::local::Listener);

#[cfg(windows)]
impl Listener {
    pub fn bind(path: impl AsRef<Path>) -> std::io::Result<Self> {
        felis_transport::local::Listener::bind(&address(path).into())
            .map(Self)
            .map_err(std::io::Error::other)
    }

    pub async fn accept(&self) -> std::io::Result<Stream> {
        let (read, write) = tokio::io::split(self.0.accept().await.map_err(std::io::Error::other)?);
        Ok(joined(read, write))
    }
}

#[cfg(windows)]
pub async fn connect(path: impl AsRef<Path>) -> std::io::Result<Stream> {
    let (read, write) = felis_transport::local::connect(address(path)).await?;
    Ok(joined(read, write))
}

#[cfg(windows)]
fn joined(
    read: impl tokio::io::AsyncRead + Send + Unpin + 'static,
    write: impl tokio::io::AsyncWrite + Send + Unpin + 'static,
) -> Stream {
    tokio::io::join(Box::new(read), Box::new(write))
}

/// Where the socket at `path` is listened at and reached: the path itself.
#[cfg(unix)]
pub fn address(path: impl AsRef<Path>) -> std::path::PathBuf {
    path.as_ref().to_path_buf()
}

/// Where the socket at `path` is listened at and reached: a pipe named after
/// the path, as a pipe is in no folder; a pipe's own name is kept. The path
/// still names the folder the files kept beside the socket are in.
#[cfg(windows)]
pub fn address(path: impl AsRef<Path>) -> std::path::PathBuf {
    const PIPES: &str = r"\\.\pipe\";
    let path = path.as_ref();
    let text = path.to_string_lossy();
    if text.starts_with(PIPES) {
        return path.to_path_buf();
    }
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.to_lowercase().bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    std::path::PathBuf::from(format!("{PIPES}adroit-{hash:016x}-{name}"))
}

/// The folder a temporary socket's own folder is made in: `/tmp`, as a
/// socket's path has to be short and the user's temporary folder may be deep.
#[cfg(unix)]
pub fn temp_dir() -> std::path::PathBuf {
    "/tmp".into()
}

/// The folder a temporary socket's own folder is made in: the user's
/// temporary folder, as a pipe's name is short whatever path names it.
#[cfg(windows)]
pub fn temp_dir() -> std::path::PathBuf {
    std::env::temp_dir()
}

/// The process listening at the socket at `path`, if one does.
#[cfg(unix)]
pub fn server_pid(path: impl AsRef<Path>) -> Option<u32> {
    let stream = std::os::unix::net::UnixStream::connect(path).ok()?;
    peer_pid(&stream)
}

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

#[cfg(all(unix, not(target_os = "macos")))]
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

/// The process listening at the pipe the socket at `path` names, if one does.
#[cfg(windows)]
pub fn server_pid(path: impl AsRef<Path>) -> Option<u32> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{Foundation, Storage::FileSystem, System::Pipes};
    let name: Vec<u16> = address(path)
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `name` ends in a nul; the handle is checked before use and
    // closed after.
    unsafe {
        let open = || {
            FileSystem::CreateFileW(
                name.as_ptr(),
                Foundation::GENERIC_READ,
                0,
                std::ptr::null(),
                FileSystem::OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        let mut pipe = open();
        // Each connection takes a pipe instance until the listener makes
        // the next, as a socket's backlog would not make it wait.
        for _ in 0..20 {
            if pipe != Foundation::INVALID_HANDLE_VALUE
                || Foundation::GetLastError() != Foundation::ERROR_PIPE_BUSY
            {
                break;
            }
            Pipes::WaitNamedPipeW(name.as_ptr(), 100);
            pipe = open();
        }
        if pipe == Foundation::INVALID_HANDLE_VALUE {
            return None;
        }
        let mut pid = 0;
        let read = Pipes::GetNamedPipeServerProcessId(pipe, &mut pid);
        Foundation::CloseHandle(pipe);
        (read != 0 && pid > 0).then_some(pid)
    }
}

/// What tells the listener at the socket at `path` from any listener there
/// before or after it, if one listens: its process, and the socket it bound,
/// as a process id alone is given again after the machine restarts.
#[cfg(unix)]
pub fn server_generation(path: impl AsRef<Path>) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let path = path.as_ref();
    let pid = server_pid(path)?;
    let socket = std::fs::symlink_metadata(path).ok()?;
    Some(format!(
        "{pid}-{}-{}.{}",
        socket.ino(),
        socket.mtime(),
        socket.mtime_nsec()
    ))
}

/// What tells the listener at the pipe the socket at `path` names from any
/// listener there before or after it, if one listens: its process, and when
/// that process was created, as a process id alone is given again.
#[cfg(windows)]
pub fn server_generation(path: impl AsRef<Path>) -> Option<String> {
    use windows_sys::Win32::{Foundation, System::Threading};
    let pid = server_pid(path)?;
    // SAFETY: the handle is checked before use and closed after; the times
    // are plain structures GetProcessTimes fills.
    unsafe {
        let process = Threading::OpenProcess(Threading::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut times: [Foundation::FILETIME; 4] = std::mem::zeroed();
        let [created, exited, kernel, user] = &mut times;
        let read = Threading::GetProcessTimes(process, created, exited, kernel, user);
        Foundation::CloseHandle(process);
        (read != 0).then(|| {
            let created =
                (u64::from(times[0].dwHighDateTime) << 32) | u64::from(times[0].dwLowDateTime);
            format!("{pid}-{created}")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn socket_dir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("adroit-sys-")
            .tempdir_in(temp_dir())
            .unwrap()
    }

    #[tokio::test]
    async fn bytes_cross_a_socket_both_ways() {
        let dir = socket_dir();
        let socket = dir.path().join("s.sock");
        let listener = Listener::bind(&socket).unwrap();
        let serving = tokio::spawn(async move {
            let mut stream = listener.accept().await.unwrap();
            let mut asked = [0; 4];
            stream.read_exact(&mut asked).await.unwrap();
            stream.write_all(b"pong").await.unwrap();
            asked
        });
        let mut stream = connect(&socket).await.unwrap();
        stream.write_all(b"ping").await.unwrap();
        let mut answer = [0; 4];
        stream.read_exact(&mut answer).await.unwrap();
        assert_eq!(&answer, b"pong");
        assert_eq!(&serving.await.unwrap(), b"ping");
    }

    #[tokio::test]
    async fn where_nothing_listens_nothing_is_reached() {
        let dir = socket_dir();
        let socket = dir.path().join("s.sock");
        assert!(connect(&socket).await.is_err());
        assert_eq!(server_pid(&socket), None);
    }

    #[tokio::test]
    async fn the_process_listening_at_a_socket_is_found() {
        let dir = socket_dir();
        let socket = dir.path().join("s.sock");
        let _listener = Listener::bind(&socket).unwrap();
        assert_eq!(server_pid(&socket), Some(std::process::id()));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_process_is_found_however_often_it_is_asked_in_a_row() {
        let dir = socket_dir();
        let socket = dir.path().join("s.sock");
        let listener = Listener::bind(&socket).unwrap();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok(stream) = listener.accept().await {
                held.push(stream);
            }
        });
        for _ in 0..20 {
            assert_eq!(server_pid(&socket), Some(std::process::id()));
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_listener_keeps_its_generation_while_it_serves() {
        let dir = socket_dir();
        let socket = dir.path().join("s.sock");
        let listener = Listener::bind(&socket).unwrap();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok(stream) = listener.accept().await {
                held.push(stream);
            }
        });
        let first = server_generation(&socket).expect("a listener has a generation");
        let mut stream = connect(&socket).await.unwrap();
        stream.write_all(b"ping").await.unwrap();
        assert_eq!(server_generation(&socket), Some(first));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_listener_bound_again_is_of_another_generation() {
        let dir = socket_dir();
        let socket = dir.path().join("s.sock");
        let listener = Listener::bind(&socket).unwrap();
        let first = server_generation(&socket).expect("a listener has a generation");
        drop(listener);
        let _ = std::fs::remove_file(&socket);
        let _listener = Listener::bind(&socket).unwrap();
        assert_ne!(server_generation(&socket), Some(first));
    }

    #[tokio::test]
    async fn where_nothing_listens_there_is_no_generation() {
        let dir = socket_dir();
        assert_eq!(server_generation(dir.path().join("s.sock")), None);
    }

    #[test]
    fn the_folder_for_sockets_exists() {
        assert!(temp_dir().is_dir());
    }

    #[cfg(windows)]
    #[test]
    fn a_path_names_one_pipe_whatever_its_case_and_another_folder_names_another() {
        let pipe = address(r"C:\Users\me\state\work.sock");
        let text = pipe.to_string_lossy().into_owned();
        assert!(text.starts_with(r"\\.\pipe\adroit-"), "{text}");
        assert!(text.ends_with("-work.sock"), "{text}");
        assert_eq!(address(r"c:\users\ME\state\work.sock"), pipe);
        assert_ne!(address(r"C:\Users\me\other\work.sock"), pipe);
    }

    #[cfg(windows)]
    #[test]
    fn a_pipe_s_own_name_is_kept() {
        assert_eq!(
            address(r"\\.\pipe\felis-1"),
            std::path::PathBuf::from(r"\\.\pipe\felis-1")
        );
    }
}
