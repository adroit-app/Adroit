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

/// The time now in the local time zone, as a year, month, day, hour and
/// minute, the month and day counted from one.
#[cfg(unix)]
pub fn local_time() -> (i32, u32, u32, u32, u32) {
    // SAFETY: `time` and `localtime_r` only write to the values passed.
    let tm = unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm
    };
    (
        tm.tm_year + 1900,
        (tm.tm_mon + 1) as u32,
        tm.tm_mday as u32,
        tm.tm_hour as u32,
        tm.tm_min as u32,
    )
}

/// The time now in the local time zone, as a year, month, day, hour and
/// minute, the month and day counted from one.
#[cfg(windows)]
pub fn local_time() -> (i32, u32, u32, u32, u32) {
    // SAFETY: GetLocalTime only writes the structure passed.
    let t = unsafe {
        let mut t = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t);
        t
    };
    (
        i32::from(t.wYear),
        u32::from(t.wMonth),
        u32::from(t.wDay),
        u32::from(t.wHour),
        u32::from(t.wMinute),
    )
}
