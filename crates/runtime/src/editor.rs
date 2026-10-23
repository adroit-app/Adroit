//! Claude Code's external editor, through which the prompt takes back text
//! typed into Claude Code's own input.
//!
//! adroit sends `Ctrl-g` to a session, Claude Code runs `$VISUAL` on a file
//! holding its input, and `adroit editor` hands that file to the window, which
//! moves the text into the prompt and leaves the file empty.

use std::path::{Path, PathBuf};

use crate::paths::instance_state;
use crate::window_socket::{Request, Response, ask, socket_path};

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

/// The script of this instance's window, whether or not it is written.
pub fn current_script() -> PathBuf {
    script_path(&socket_path(&instance_state()))
}

/// What an agent session's environment holds so that Claude Code's external
/// editor is `script`, with the user's own editor, read through `var`, kept
/// in `$ADROIT_USER_EDITOR`. Nothing when there is no script to name.
pub fn session_env(
    script: Option<&Path>,
    var: impl Fn(&str) -> Option<String>,
) -> Vec<(String, String)> {
    let Some(script) = script else {
        return Vec::new();
    };
    let script = script.display().to_string();
    let mut env = Vec::new();
    // A session started from inside another one inherits the script as its
    // editor; naming the script as the user's own would make it call itself.
    let own = var("VISUAL")
        .or_else(|| var("EDITOR"))
        .filter(|own| *own != script)
        .or_else(|| var(USER_EDITOR));
    if let Some(own) = own {
        env.push((USER_EDITOR.into(), own));
    }
    env.push(("VISUAL".into(), script.clone()));
    env.push(("EDITOR".into(), script));
    env
}

/// The editor environment for an agent session this process starts.
///
/// The script's presence is the only condition, not whether a window
/// answers right now: sessions outlive the window that started them and are
/// started by the daemon without one, and the script opens the user's own
/// editor whenever no window asked for the input.
pub fn environment() -> Vec<(String, String)> {
    let script = current_script();
    session_env(script.exists().then_some(script.as_path()), |name| {
        std::env::var(name).ok()
    })
}

/// Whether a session started with `env` hands its input to `script`: Claude
/// Code runs `$VISUAL`, or else `$EDITOR`, and a later entry of `env`
/// overrides an earlier one.
pub fn runs_script(env: &[(String, String)], script: &Path) -> bool {
    let last = |name: &str| env.iter().rev().find(|(k, _)| k == name).map(|(_, v)| v);
    last("VISUAL")
        .or_else(|| last("EDITOR"))
        .is_some_and(|editor| Path::new(editor) == script)
}

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

    fn vars<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        }
    }

    fn pairs(env: &[(String, String)]) -> Vec<(&str, &str)> {
        env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
    }

    #[test]
    fn a_session_names_the_script_as_its_editor_and_keeps_the_user_s_own() {
        let env = session_env(
            Some(Path::new("/run/adroit.editor")),
            vars(&[("VISUAL", "code -w"), ("EDITOR", "vi")]),
        );
        assert_eq!(
            pairs(&env),
            [
                (USER_EDITOR, "code -w"),
                ("VISUAL", "/run/adroit.editor"),
                ("EDITOR", "/run/adroit.editor"),
            ]
        );
    }

    #[test]
    fn the_user_s_editor_falls_back_to_editor_when_visual_is_unset() {
        let env = session_env(Some(Path::new("/s.editor")), vars(&[("EDITOR", "vi")]));
        assert_eq!(pairs(&env)[0], (USER_EDITOR, "vi"));
    }

    #[test]
    fn a_session_without_an_editor_of_the_user_s_carries_none_to_fall_back_to() {
        let env = session_env(Some(Path::new("/s.editor")), vars(&[]));
        assert_eq!(
            pairs(&env),
            [("VISUAL", "/s.editor"), ("EDITOR", "/s.editor")]
        );
    }

    #[test]
    fn a_session_started_from_one_running_the_script_keeps_the_user_s_editor_not_the_script() {
        let env = session_env(
            Some(Path::new("/s.editor")),
            vars(&[
                ("VISUAL", "/s.editor"),
                ("EDITOR", "/s.editor"),
                (USER_EDITOR, "nvim"),
            ]),
        );
        assert_eq!(pairs(&env)[0], (USER_EDITOR, "nvim"));
    }

    #[test]
    fn without_the_script_a_session_keeps_the_user_s_editor_untouched() {
        assert!(session_env(None, vars(&[("VISUAL", "vi")])).is_empty());
    }

    #[test]
    fn a_session_runs_the_script_when_its_effective_editor_is_the_script() {
        let script = Path::new("/s.editor");
        let env = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        assert!(runs_script(&session_env(Some(script), vars(&[])), script));
        assert!(runs_script(&env(&[("EDITOR", "/s.editor")]), script));
        assert!(!runs_script(&env(&[]), script));
        assert!(!runs_script(&env(&[("VISUAL", "/other.editor")]), script));
        // Claude Code prefers VISUAL, and a later entry overrides an earlier one.
        assert!(!runs_script(
            &env(&[("VISUAL", "vi"), ("EDITOR", "/s.editor")]),
            script
        ));
        assert!(!runs_script(
            &env(&[("VISUAL", "/s.editor"), ("VISUAL", "vi")]),
            script
        ));
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
