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

/// The creation flag that starts a console program on a console of its own
/// that has no window; the console programs it starts share that console, so
/// they show no window either.
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = Threading::CREATE_NO_WINDOW;

/// Whether this process is the only one on a console that has a window: one
/// Windows opened for it when it was started from Explorer or a shortcut,
/// rather than a terminal's, which the terminal's shell shares, or one with
/// no window, as [`CREATE_NO_WINDOW`] gives.
#[cfg(windows)]
pub fn console_window_alone() -> bool {
    use windows_sys::Win32::System::Console::{GetConsoleProcessList, GetConsoleWindow};
    let mut ids = [0u32; 2];
    // SAFETY: the list holds as many ids as the length passed says, and
    // asking for the console's window has no preconditions.
    unsafe { GetConsoleProcessList(ids.as_mut_ptr(), 2) == 1 && !GetConsoleWindow().is_null() }
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

    /// Where [`reports_whether_its_console_is_its_own`] writes what it found.
    #[cfg(windows)]
    const REPORT: &str = "ADROIT_TEST_CONSOLE_REPORT";

    #[cfg(windows)]
    #[test]
    #[ignore = "run by a test that starts it on a console of its own"]
    fn reports_whether_its_console_is_its_own() {
        if let Some(report) = std::env::var_os(REPORT) {
            std::fs::write(report, console_window_alone().to_string()).unwrap();
        }
    }

    /// What [`console_window_alone`] says in this test program started with
    /// the creation flags `flags`.
    #[cfg(windows)]
    fn console_window_alone_started_with(flags: u32) -> String {
        use std::os::windows::process::CommandExt;
        let dir = tempfile::tempdir().unwrap();
        let report = dir.path().join("report");
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "process::tests::reports_whether_its_console_is_its_own",
            ])
            .env(REPORT, &report)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(flags)
            .status()
            .unwrap();
        assert!(status.success());
        std::fs::read_to_string(report).unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn a_program_given_a_console_window_of_its_own_is_alone_on_it() {
        const CREATE_NEW_CONSOLE: u32 = Threading::CREATE_NEW_CONSOLE;
        assert_eq!(
            console_window_alone_started_with(CREATE_NEW_CONSOLE),
            "true"
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_program_on_a_console_with_no_window_has_no_console_window_to_leave() {
        assert_eq!(console_window_alone_started_with(CREATE_NO_WINDOW), "false");
    }

    #[test]
    fn a_process_asked_to_end_ends() {
        let mut child = waiting();
        terminate(child.id());
        assert!(!child.wait().unwrap().success());
    }
}
