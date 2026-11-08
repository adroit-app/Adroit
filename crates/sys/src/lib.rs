//! The Unix file and process calls Adroit makes, under one name on every
//! platform.
//!
//! On Unix these are the system's own. On Windows a file is kept from other
//! users by the access list of the profile it lives in, not by mode bits, so
//! what stands in sets no mode and reports every file as the user's own and
//! closed to others: the checks written against the Unix calls pass, and the
//! profile does what they guard.

pub mod fs;
pub mod local;
pub mod process;

/// The real user of this process.
#[cfg(unix)]
pub fn uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

/// The effective user of this process.
#[cfg(unix)]
pub fn euid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

/// The user every file is reported as owned by, as [`fs::MetadataExt::uid`]
/// says.
#[cfg(windows)]
pub fn uid() -> u32 {
    0
}

/// The same user as [`uid`].
#[cfg(windows)]
pub fn euid() -> u32 {
    uid()
}

/// This machine's name on the network, if it has one.
#[cfg(unix)]
pub fn host_name() -> Option<String> {
    let mut buffer = [0u8; 256];
    // SAFETY: the buffer and its length match, as gethostname requires.
    let read = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    let end = buffer.iter().position(|b| *b == 0).unwrap_or(buffer.len());
    (read == 0).then(|| String::from_utf8_lossy(&buffer[..end]).into_owned())
}

/// This machine's name on the network, if it has one.
#[cfg(windows)]
pub fn host_name() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}
