//! `adroitw`: opens Adroit's window as `adroit` does. Built for the Windows
//! subsystem, it is given no console, so none flashes up when it is started
//! from Explorer or a shortcut; it starts the `adroit` beside it on a console
//! that has no window, and every program the window runs shares that one.
//! Elsewhere it is `adroit` itself.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::{Path, PathBuf};

fn main() {
    std::process::exit(launch(std::env::args_os().skip(1).collect()));
}

/// The `adroit` beside the launcher at `exe`.
fn beside(exe: &Path) -> PathBuf {
    exe.with_file_name(format!("adroit{}", std::env::consts::EXE_SUFFIX))
}

/// Start `adroit` with `args` on a console with no window and leave it
/// running; tell the user in a message box when it cannot start, as nothing
/// else of this program can be seen.
#[cfg(windows)]
fn launch(args: Vec<std::ffi::OsString>) -> i32 {
    use std::os::windows::process::CommandExt;
    let started = std::env::current_exe().and_then(|exe| {
        let adroit = beside(&exe);
        std::process::Command::new(&adroit)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(adroit_sys::process::CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| std::io::Error::new(e.kind(), format!("{}: {e}", adroit.display())))
    });
    match started {
        Ok(_) => 0,
        Err(e) => {
            tell(&format!("Adroit cannot start: {e}"));
            1
        }
    }
}

/// Show `text` in a message box of the system's own.
#[cfg(windows)]
fn tell(text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    let wide = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let (text, caption) = (wide(text), wide("Adroit"));
    // SAFETY: both strings are NUL-terminated UTF-16 that outlive the call,
    // and no owner window is passed.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONERROR,
        )
    };
}

/// Become `adroit` with `args`, as no console stands in the way here.
#[cfg(unix)]
fn launch(args: Vec<std::ffi::OsString>) -> i32 {
    use std::os::unix::process::CommandExt;
    let error = match std::env::current_exe() {
        Ok(exe) => std::process::Command::new(beside(&exe)).args(args).exec(),
        Err(e) => e,
    };
    eprintln!("adroitw: cannot start adroit: {error}");
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_adroit_started_is_the_one_beside_the_launcher() {
        let dir = Path::new("apps").join("Adroit");
        let launcher = dir.join(format!("adroitw{}", std::env::consts::EXE_SUFFIX));
        assert_eq!(
            beside(&launcher),
            dir.join(format!("adroit{}", std::env::consts::EXE_SUFFIX))
        );
    }
}
