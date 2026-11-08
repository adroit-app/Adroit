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

/// Leave this process running when the user interrupts, from the terminal it
/// shares, the program it waits on, as a shell does for the program it runs.
/// Programs it starts afterwards inherit this; start them first.
#[cfg(unix)]
pub fn ignore_interrupts() {
    for signal in [libc::SIGINT, libc::SIGQUIT] {
        // SAFETY: SIG_IGN installs no handler of this process's own.
        unsafe { libc::signal(signal, libc::SIG_IGN) };
    }
}

#[cfg(windows)]
use windows_sys::Win32::{Foundation, System::Threading};

/// Leave this process running when the user interrupts, from the console it
/// shares, the program it waits on.
#[cfg(windows)]
pub fn ignore_interrupts() {
    // SAFETY: no handler is passed; the call only sets this process to ignore Ctrl+C.
    unsafe { windows_sys::Win32::System::Console::SetConsoleCtrlHandler(None, 1) };
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting() -> std::process::Child {
        #[cfg(unix)]
        let mut command = std::process::Command::new("sleep");
        #[cfg(unix)]
        command.arg("30");
        #[cfg(windows)]
        let mut command = std::process::Command::new("ping");
        #[cfg(windows)]
        command.args(["-n", "30", "127.0.0.1"]);
        command.stdout(std::process::Stdio::null()).spawn().unwrap()
    }

    #[test]
    fn this_process_runs() {
        assert!(alive(std::process::id()));
    }

    #[test]
    fn a_process_killed_runs_no_more() {
        let mut child = waiting();
        assert!(alive(child.id()));
        kill(child.id());
        child.wait().unwrap();
        assert!(!alive(child.id()));
    }

    #[test]
    fn a_process_asked_to_end_ends() {
        let mut child = waiting();
        terminate(child.id());
        assert!(!child.wait().unwrap().success());
    }
}
