//! A command terminal's program run under adroit, which keeps the terminal and
//! its screen once the program ends and tags the terminal with how it ended,
//! so whoever started it can still read what it did.

use std::io::Write;
use std::process::ExitStatus;

use adroit_term::{Daemon, tags};

/// The argument that starts this executable as [`keep`].
pub const KEEP: &str = "__keep";

/// `program` and `args` as adroit starts them, to keep their terminal.
pub(crate) fn kept(program: String, args: Vec<String>) -> (String, Vec<String>) {
    let adroit = crate::work_host::adroit_program()
        .to_string_lossy()
        .into_owned();
    let args = [KEEP.to_string(), program]
        .into_iter()
        .chain(args)
        .collect();
    (adroit, args)
}

/// Run as a command terminal's program, with `args` the program and its
/// arguments: run it on this terminal, tag the terminal with how it ended,
/// say so on the screen, then wait to be stopped.
pub fn keep(args: &[String]) -> i32 {
    let Some((program, rest)) = args.split_first() else {
        eprintln!("adroit: nothing to keep");
        return 2;
    };
    let code = match std::process::Command::new(program).args(rest).spawn() {
        Ok(mut child) => {
            // Only once the program has started, which would inherit it.
            adroit_sys::process::ignore_interrupts();
            child.wait().map_or(1, status_code)
        }
        Err(error) => {
            eprintln!("adroit: cannot start {program}: {error}");
            127
        }
    };
    if let Err(error) = record(code) {
        eprintln!("adroit: cannot record how {program} ended: {error}");
    }
    let mut out = std::io::stdout();
    let _ = out
        .write_all(ended(code).as_bytes())
        .and_then(|()| out.flush());
    loop {
        std::thread::park();
    }
}

/// The status a shell gives a program that ended so: its exit code, or 128
/// and the number of the signal that ended it.
fn status_code(status: ExitStatus) -> i32 {
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
        return 128 + signal;
    }
    status.code().unwrap_or(1)
}

/// The line the screen is left with, on a line of its own whatever the
/// program left the terminal in.
fn ended(code: i32) -> String {
    format!("\r\n[exited with status {code}]\r\n")
}

/// Tag the terminal this runs in with `code`, through the daemon that
/// started it, which names both itself and the terminal to its programs.
fn record(code: i32) -> Result<(), String> {
    let socket = std::env::var("FELIS_SOCKET").map_err(|e| e.to_string())?;
    let id = std::env::var("FELIS_SESSION_ID").map_err(|e| e.to_string())?;
    let id = u128::from_str_radix(&id, 16).map_err(|e| e.to_string())?;
    let add = tags::tagged(crate::sessions::EXIT, &code.to_string());
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?
        .block_on(Daemon::new(socket).tag(id, add, Vec::new()))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_terminal_runs_adroit_which_runs_the_program_as_it_was_given() {
        let (program, args) = kept("/bin/zsh".into(), vec!["-c".into(), "exec make".into()]);
        assert_eq!(
            program,
            crate::work_host::adroit_program().to_string_lossy()
        );
        assert_eq!(args, [KEEP, "/bin/zsh", "-c", "exec make"]);
    }

    #[test]
    #[cfg(unix)]
    fn a_program_s_end_is_told_as_a_shell_tells_it() {
        let ran = |script: &str| {
            std::process::Command::new("/bin/sh")
                .args(["-c", script])
                .status()
                .unwrap()
        };
        assert_eq!(status_code(ran("exit 0")), 0);
        assert_eq!(status_code(ran("exit 3")), 3);
        assert_eq!(status_code(ran("kill -TERM $$")), 128 + 15);
    }

    #[test]
    fn the_screen_is_left_saying_how_the_program_ended_on_a_line_of_its_own() {
        assert_eq!(ended(1), "\r\n[exited with status 1]\r\n");
    }
}
