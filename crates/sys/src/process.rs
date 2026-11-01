//! Other processes of this user: whether one runs, and ending one.

/// Whether process `pid` still runs.
#[cfg(unix)]
pub fn alive(pid: u32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    let answered = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
    answered || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Ask process `pid` to end, as SIGTERM does.
#[cfg(unix)]
pub fn terminate(pid: u32) {
    // SAFETY: kill(2) only sends a signal to the process named.
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
}

/// End process `pid` at once, as SIGKILL does.
#[cfg(unix)]
pub fn kill(pid: u32) {
    // SAFETY: kill(2) only sends a signal to the process named.
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
}

#[cfg(windows)]
use windows_sys::Win32::{Foundation, System::Threading};

/// Whether process `pid` still runs.
#[cfg(windows)]
pub fn alive(pid: u32) -> bool {
    // SAFETY: the handle is checked before use and closed after.
    unsafe {
        let handle = Threading::OpenProcess(Threading::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code = 0;
        let read = Threading::GetExitCodeProcess(handle, &mut code);
        Foundation::CloseHandle(handle);
        read != 0 && code == Foundation::STILL_ACTIVE as u32
    }
}

/// End process `pid`: Windows has no signal asking a process to end, so this
/// ends it as [`kill`] does.
#[cfg(windows)]
pub fn terminate(pid: u32) {
    kill(pid)
}

/// End process `pid` at once.
#[cfg(windows)]
pub fn kill(pid: u32) {
    // SAFETY: the handle is checked before use and closed after.
    unsafe {
        let handle = Threading::OpenProcess(Threading::PROCESS_TERMINATE, 0, pid);
        if !handle.is_null() {
            Threading::TerminateProcess(handle, 1);
            Foundation::CloseHandle(handle);
        }
    }
}
