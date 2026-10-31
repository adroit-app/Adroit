//! Claude Code's external editor, through which the prompt takes back text
//! typed into Claude Code's own input.
//!
//! adroit sends `Ctrl-g` to a session, Claude Code runs `$VISUAL` on a file
//! holding its input, and `adroit editor` offers that file to each window of
//! the instance in turn; the window that asked moves the text into its prompt
//! and leaves the file empty.

use std::path::{Path, PathBuf};

use adroit_model::paths::{USER_EDITOR, editor_script};

use crate::window_socket::{Request, Response, ask_each, windows};

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
/// windows' folder is fixed so the session finds the windows of its instance
/// whatever its own environment says.
pub fn script(program: &Path, windows: &Path) -> String {
    format!(
        "#!/bin/sh\nADROIT_WINDOWS={} exec {} editor \"$@\"\n",
        quoted(windows),
        quoted(program)
    )
}

/// Write the script for the windows answering in `windows`.
pub fn install(program: &Path, windows: &Path) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = editor_script(windows);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, script(program, windows))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    if let Ok(Some(binding)) = adroit_connect::environments::active()
        && binding.target == adroit_connect::Target::Local
    {
        let service = adroit_model::paths::instance_dir(
            &adroit_model::paths::state_dir(),
            binding.work_instance.as_deref().unwrap_or("default"),
        );
        let base = binding.scope.map_or_else(
            || service.clone(),
            |scope| service.join("environments").join(scope).join("state"),
        );
        let alias = editor_script(&base.join("windows"));
        std::fs::create_dir_all(alias.parent().expect("editor script parent"))?;
        std::fs::write(&alias, script(program, windows))?;
        std::fs::set_permissions(alias, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(path)
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
    let scope = std::env::var("ADROIT_ENVIRONMENT").ok();
    let mut candidates = if scope.is_some() {
        Vec::new()
    } else {
        vec![windows()]
    };
    {
        let work_instance =
            std::env::var("ADROIT_WORK_INSTANCE").unwrap_or_else(|_| crate::paths::instance());
        let instances = crate::paths::state_dir().join("adroit/instances");
        let mut states = vec![crate::paths::state_dir().join("adroit")];
        states.extend(
            std::fs::read_dir(instances)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path()),
        );
        for state in states {
            if let Ok(Some(binding)) = adroit_connect::environments::Binding::load(&state)
                && binding.scope == scope
                && binding.target == adroit_connect::Target::Local
                && binding.work_instance.as_deref().unwrap_or("default") == work_instance
            {
                candidates.push(state.join("windows"));
            }
        }
    }
    let answer = candidates.iter().find_map(|windows| {
        ask_each(
            windows,
            &Request::Editor {
                path: PathBuf::from(path),
            },
            |r| matches!(r, Response::Taken(_)),
        )
    });
    if answer.is_some() {
        return 0;
    }
    // No window asked, or none is there: the user opened the editor.
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
    fn the_script_runs_adroit_editor_against_the_windows_of_its_instance() {
        let s = script(Path::new("/opt/adroit"), Path::new("/run/it's/windows"));
        assert_eq!(
            s,
            "#!/bin/sh\nADROIT_WINDOWS='/run/it'\\''s/windows' exec '/opt/adroit' editor \"$@\"\n"
        );
    }
}
