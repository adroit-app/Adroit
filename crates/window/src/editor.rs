//! Claude Code's external editor, through which the prompt takes back text
//! typed into Claude Code's own input.
//!
//! adroit sends `Ctrl-g` to a session, Claude Code runs `$VISUAL` on a file
//! holding its input, and `adroit editor` offers that file to each window
//! showing the session's environment in turn; the window that asked moves
//! the text into its prompt and leaves the file empty.

use std::path::{Path, PathBuf};

use adroit_connect::Target;
use adroit_connect::environments::{Binding, Catalog, Environment};
use adroit_model::EnvironmentId;
use adroit_model::paths::{USER_EDITOR, editor_script, instance_dir, window_state};

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

fn write_script(path: &Path, program: &Path, windows: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, script(program, windows))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

/// The script the sessions of `binding`'s environment name as their editor:
/// the windows' script of the state its work host keeps for it, with
/// instances' folders under `state`. None for one on a work machine, whose
/// sessions name the work machine's.
pub fn alias(state: &Path, binding: &Binding) -> Option<PathBuf> {
    if binding.target != Target::Local {
        return None;
    }
    let service = instance_dir(state, binding.work_instance.as_deref().unwrap_or("default"));
    let base = match &binding.scope {
        Some(scope) => service.join("environments").join(scope).join("state"),
        None => service,
    };
    Some(editor_script(&base.join("windows")))
}

/// Write the script for the windows answering in `windows`.
pub fn install(program: &Path, windows: &Path) -> std::io::Result<PathBuf> {
    let path = editor_script(windows);
    write_script(&path, program, windows)?;
    if let Ok(Some(binding)) = adroit_connect::environments::active()
        && let Some(alias) = alias(&adroit_model::paths::state_dir(), &binding)
    {
        write_script(&alias, program, windows)?;
    }
    Ok(path)
}

/// Write the script the sessions of `environment`, shown beside the
/// window's own, name as their editor, unless one is written: a script the
/// environment's own windows wrote is not taken from them, as `adroit
/// editor` finds this window by the environments it shows.
pub fn install_shown(
    program: &Path,
    windows: &Path,
    environment: &Environment,
) -> std::io::Result<()> {
    let Ok(binding) = environment.bind(&adroit_model::paths::instance()) else {
        return Ok(());
    };
    match alias(&adroit_model::paths::state_dir(), &binding) {
        Some(alias) if !alias.exists() => write_script(&alias, program, windows),
        _ => Ok(()),
    }
}

/// The windows of an instance, as `adroit editor` looks at them: where they
/// answer, the environment the instance is bound to, and those its windows
/// show beside it.
#[derive(Debug)]
pub(crate) struct Instance {
    pub(crate) windows: PathBuf,
    pub(crate) binding: Option<Binding>,
    pub(crate) shows: Vec<EnvironmentId>,
}

impl Instance {
    fn at(state: &Path) -> Self {
        let windows = state.join("windows");
        let shows = std::fs::read_dir(&windows)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
            .filter_map(|n| {
                std::fs::read_to_string(window_state(&windows, n).join("environments")).ok()
            })
            .flat_map(|text| crate::kept_environments::read(&text))
            .collect();
        Self {
            binding: Binding::load(state).ok().flatten(),
            windows,
            shows,
        }
    }
}

/// The windows' folders the file of a session of the environment with
/// `scope`, on the work instance `work`, is offered to, in turn: `own`
/// first for a session of no environment, then those of each instance bound
/// to that environment or showing it beside its own.
pub(crate) fn offered(
    scope: Option<&str>,
    work: &str,
    own: PathBuf,
    instances: impl IntoIterator<Item = Instance>,
    catalog: &Catalog,
) -> Vec<PathBuf> {
    let reaches = |binding: &Binding| {
        binding.scope.as_deref() == scope
            && binding.target == Target::Local
            && binding.work_instance.as_deref().unwrap_or("default") == work
    };
    let shown = |id: &EnvironmentId| {
        catalog.environments.iter().any(|environment| {
            EnvironmentId::new(&environment.id) == *id
                && !environment.archived
                && environment
                    .bind(adroit_model::paths::DEFAULT_INSTANCE)
                    .is_ok_and(|binding| reaches(&binding))
        })
    };
    let mut offered = if scope.is_none() {
        vec![own]
    } else {
        Vec::new()
    };
    for instance in instances {
        let answers =
            instance.binding.as_ref().is_some_and(reaches) || instance.shows.iter().any(shown);
        if answers && !offered.contains(&instance.windows) {
            offered.push(instance.windows);
        }
    }
    offered
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
    let work_instance =
        std::env::var("ADROIT_WORK_INSTANCE").unwrap_or_else(|_| crate::paths::instance());
    let candidates = {
        let instances = crate::paths::state_dir().join("adroit/instances");
        let mut states = vec![crate::paths::state_dir().join("adroit")];
        states.extend(
            std::fs::read_dir(instances)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path()),
        );
        let catalog = Catalog::load(&Catalog::path()).unwrap_or_default();
        offered(
            scope.as_deref(),
            &work_instance,
            windows(),
            states.iter().map(|state| Instance::at(state)),
            &catalog,
        )
    };
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

    fn there(scope: &str) -> Environment {
        Environment {
            id: "there".into(),
            name: "There".into(),
            target: Target::Local,
            scope: Some(scope.into()),
            archived: false,
            legacy_client: None,
            work_instance: None,
        }
    }

    fn bound(scope: &str) -> Binding {
        there(scope).bind("client").unwrap()
    }

    fn instance(windows: &str, binding: Option<Binding>, shows: &[&str]) -> Instance {
        Instance {
            windows: windows.into(),
            binding,
            shows: shows.iter().map(EnvironmentId::new).collect(),
        }
    }

    #[test]
    fn an_environment_s_sessions_name_the_script_its_work_host_keeps_for_its_scope() {
        assert_eq!(
            alias(Path::new("/state"), &bound("scope-b")),
            Some(PathBuf::from(
                "/state/adroit/environments/scope-b/state/windows.editor"
            ))
        );
        let mut remote = bound("scope-b");
        remote.target = Target::Native {
            address: "work.example:7100".into(),
            server: "ab".repeat(32),
        };
        assert_eq!(alias(Path::new("/state"), &remote), None);
    }

    #[test]
    fn a_session_of_an_environment_a_window_shows_beside_its_own_is_offered_to_that_window() {
        let catalog = Catalog {
            environments: vec![there("scope-b")],
            ..Catalog::default()
        };
        let instances = [
            instance("/a/windows", Some(bound("scope-a")), &[]),
            instance("/b/windows", Some(bound("scope-a")), &["there"]),
            instance("/c/windows", Some(bound("scope-b")), &[]),
        ];
        assert_eq!(
            offered(
                Some("scope-b"),
                "default",
                "/own".into(),
                instances,
                &catalog
            ),
            [PathBuf::from("/b/windows"), PathBuf::from("/c/windows")]
        );
    }

    #[test]
    fn a_window_that_closed_an_environment_is_no_longer_offered_its_sessions() {
        let catalog = Catalog {
            environments: vec![there("scope-b")],
            ..Catalog::default()
        };
        let closed = [instance("/b/windows", Some(bound("scope-a")), &[])];
        assert!(offered(Some("scope-b"), "default", "/own".into(), closed, &catalog).is_empty());
    }

    #[test]
    fn a_session_of_no_environment_is_offered_to_its_own_windows_first() {
        let offered = offered(None, "default", "/own".into(), [], &Catalog::default());
        assert_eq!(offered, [PathBuf::from("/own")]);
    }

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
