//! Claude Code's external editor, through which the prompt takes back text
//! typed into Claude Code's own input.
//!
//! adroit sends `Ctrl-g` to a session, Claude Code runs `$VISUAL` on a file
//! holding its input, and `adroit editor` hands that file to the window, which
//! moves the text into the prompt and leaves the file empty.

use std::path::{Path, PathBuf};

use crate::paths::instance_state;
use crate::shells::{Request, Response, ask, socket_path};

/// The lines typed into Claude Code's input, from the file it hands its
/// editor.
pub fn typed(text: &str) -> Vec<String> {
    // With `externalEditorContext` on, Claude Code puts its last reply first
    // as `#` lines, but they cannot be told from `#` lines the user typed, and
    // the file is emptied once read: taking a reply along is better than
    // losing what was typed.
    let mut lines: Vec<&str> = text.lines().collect();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines.into_iter().map(String::from).collect()
}

fn quoted(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

/// The script Claude Code runs as `$VISUAL`. It runs one program without
/// arguments, so the two words of `adroit editor` go through this, and the
/// socket is fixed so the session finds this window whatever its own
/// environment says.
pub fn script(program: &Path, socket: &Path) -> String {
    format!(
        "#!/bin/sh\nADROIT_SOCKET={} exec {} editor \"$@\"\n",
        quoted(socket),
        quoted(program)
    )
}

/// Where the script for the window answering on `socket` lives: beside the
/// socket, so windows on sockets of their own do not share one.
pub fn script_path(socket: &Path) -> PathBuf {
    socket.with_extension("editor")
}

/// Write the script for this window.
pub fn install(program: &Path, socket: &Path) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = script_path(socket);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, script(program, socket))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// What `adroit editor` hands the user's own editor, `$ADROIT_USER_EDITOR`,
/// when the window did not ask for the text: the shell runs it, so it may
/// carry arguments of its own.
pub const USER_EDITOR: &str = "ADROIT_USER_EDITOR";

/// The arguments for `sh` that open `path` in `editor`.
pub fn user_editor(editor: &str, path: &str) -> Vec<String> {
    vec![
        "-c".into(),
        format!("{editor} \"$1\""),
        "sh".into(),
        path.into(),
    ]
}

/// `adroit editor <file>`, as Claude Code's external editor.
pub fn cli(args: &[String]) -> i32 {
    let [path] = args else {
        eprintln!("usage: adroit editor <file>");
        return 2;
    };
    let answer = ask(
        &socket_path(&instance_state()),
        &Request::Editor {
            path: PathBuf::from(path),
        },
    );
    if let Ok(Response::Taken(_)) = answer {
        return 0;
    }
    // The window did not ask, or is not there: the user opened the editor.
    // With no editor of theirs, the file is left as it is and Claude Code
    // keeps its input.
    match std::env::var(USER_EDITOR) {
        Ok(editor) if !editor.trim().is_empty() => std::process::Command::new("sh")
            .args(user_editor(&editor, path))
            .status()
            .map_or(1, |s| s.code().unwrap_or(1)),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn what_was_typed_keeps_its_lines_without_blank_ones_at_the_end() {
        assert_eq!(typed("one\ntwo\n\n"), ["one", "two"]);
        assert!(typed("\n \n").is_empty());
    }

    #[test]
    fn lines_that_look_like_claude_code_s_reply_are_kept_as_typed() {
        assert_eq!(typed("# Plan\n\nDo X\n"), ["# Plan", "", "Do X"]);
    }

    #[test]
    fn the_user_s_editor_is_run_through_the_shell_on_the_file() {
        assert_eq!(
            user_editor("code -w", "/tmp/claude prompt.md"),
            ["-c", "code -w \"$1\"", "sh", "/tmp/claude prompt.md"]
        );
    }

    #[test]
    fn each_socket_has_a_script_of_its_own_beside_it() {
        assert_eq!(
            script_path(Path::new("/run/adroit/adroit.sock")),
            Path::new("/run/adroit/adroit.editor")
        );
        assert_ne!(
            script_path(Path::new("/tmp/one.sock")),
            script_path(Path::new("/tmp/two.sock"))
        );
    }

    #[test]
    fn the_script_runs_adroit_editor_against_this_window_s_socket() {
        let s = script(Path::new("/opt/adroit"), Path::new("/run/it's.sock"));
        assert_eq!(
            s,
            "#!/bin/sh\nADROIT_SOCKET='/run/it'\\''s.sock' exec '/opt/adroit' editor \"$@\"\n"
        );
    }
}
