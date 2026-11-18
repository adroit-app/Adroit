//! The console window Windows opens for the window's program when nothing
//! else gave it a console.

/// Start this program again with the same arguments, on a console that has
/// no window, when Windows opened a console window for it alone, as it does
/// for a program started from Explorer or a shortcut; whether it was, and
/// this process is to end. The window and every console program it starts,
/// Neovim and git among them, run on that console and show no window of
/// their own. A program started from a terminal stays on the terminal's.
pub fn reopened_without_console_window() -> bool {
    use std::os::windows::process::CommandExt;
    if !adroit_sys::process::console_window_alone() {
        return false;
    }
    let Ok(program) = std::env::current_exe() else {
        return false;
    };
    std::process::Command::new(program)
        .args(std::env::args_os().skip(1))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(adroit_sys::process::CREATE_NO_WINDOW)
        .spawn()
        .is_ok()
}
