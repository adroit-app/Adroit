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
        let pipe = FileSystem::CreateFileW(
            name.as_ptr(),
            Foundation::GENERIC_READ,
            0,
            std::ptr::null(),
            FileSystem::OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if pipe == Foundation::INVALID_HANDLE_VALUE {
            return None;
        }
        let mut pid = 0;
        let read = Pipes::GetNamedPipeServerProcessId(pipe, &mut pid);
        Foundation::CloseHandle(pipe);
        (read != 0 && pid > 0).then_some(pid)
    }
}
