//! Claude Code's external editor as an agent session is started with it:
//! the window's script, through which the prompt takes back text typed into
//! Claude Code's own input, with the user's own editor kept beside it.

use std::path::{Path, PathBuf};

use adroit_model::paths::{USER_EDITOR, editor_script, instance_state, window_socket};

/// The script of this instance's window, whether or not it is written.
pub fn current_script() -> PathBuf {
    editor_script(&window_socket(&instance_state()))
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

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
}
