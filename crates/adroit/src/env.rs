//! What adroit's children must not inherit from where adroit was started.

/// What a Claude Code session sets for the commands it runs, marking them as
/// its own. A session started under them counts as that session's child: it
/// keeps no transcript, so it cannot be resumed.
const CLAUDE_SESSION: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_BRIDGE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_EFFORT",
    "CLAUDE_PID",
];

/// The shell the account logs in with, from the user database. `$SHELL` is
/// left aside: it is whatever the program that started adroit had set, such
/// as a development shell's own.
pub fn login_shell() -> Option<String> {
    // SAFETY: getpwuid returns a pointer into static storage, or null; the
    // name is copied out at once, before anything else can call it.
    unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() || (*entry).pw_shell.is_null() {
            return None;
        }
        let shell = std::ffi::CStr::from_ptr((*entry).pw_shell)
            .to_string_lossy()
            .into_owned();
        (!shell.is_empty()).then_some(shell)
    }
}

/// The `PATH` the user's login `shell` sets up, which a window opened from
/// the Finder does not get: where `claude`, `nvim` and `git` are found.
pub fn login_path(shell: &str) -> Option<String> {
    let out = std::process::Command::new(shell)
        .args(["-l", "-c", r#"printf '%s' "$PATH""#])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let path = String::from_utf8(out.stdout).ok()?;
    (out.status.success() && !path.trim().is_empty()).then(|| path.trim().to_string())
}

/// The variables among `names` to remove before anything is started.
///
/// Started from a terminal inside an editor, `NVIM` and the like point at that
/// editor, and plugins in the embedded prompt or the terminals (vim-guise among
/// them) would talk to it instead of their own. Started from inside Claude
/// Code, its session's marks would make every session adroit starts a child
/// of it. They are only dropped when `CLAUDECODE` says so, since a user may
/// set some of them, such as `CLAUDE_EFFORT`, on purpose.
pub fn inherited(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let names: Vec<String> = names.into_iter().collect();
    let in_claude = names.iter().any(|n| n == "CLAUDECODE");
    names
        .into_iter()
        .filter(|n| {
            matches!(
                n.as_str(),
                "ADROIT_CONTROL_TOKEN" | "ADROIT_SESSION_ID" | "ADROIT_SESSION_GENERATION"
            ) || n == "CODEX_THREAD_ID"
                || n == "NVIM"
                || n == "NVIM_LISTEN_ADDRESS"
                || n.starts_with("GUISE_")
                || (in_claude && CLAUDE_SESSION.contains(&n.as_str()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_application_does_not_inherit_its_callers_control_identity() {
        assert_eq!(
            dropped(&[
                "ADROIT_CONTROL_TOKEN",
                "ADROIT_SESSION_ID",
                "ADROIT_SESSION_GENERATION",
                "ADROIT_INSTANCE"
            ]),
            [
                "ADROIT_CONTROL_TOKEN",
                "ADROIT_SESSION_ID",
                "ADROIT_SESSION_GENERATION"
            ]
        );
    }

    #[test]
    fn the_account_s_login_shell_is_its_own_whatever_shell_started_adroit() {
        let shell = login_shell().unwrap();
        assert!(shell.starts_with('/'), "{shell}");
        assert!(std::path::Path::new(&shell).exists(), "{shell}");
    }

    #[test]
    fn a_login_shell_tells_the_path_it_sets_up() {
        let path = login_path("/bin/sh").unwrap();
        assert!(path.split(':').any(|d| d == "/usr/bin"), "{path}");
        assert_eq!(login_path("/no/such/shell"), None);
    }

    fn dropped(vars: &[&str]) -> Vec<String> {
        inherited(vars.iter().map(|v| v.to_string()))
    }

    #[test]
    fn an_outer_editor_is_forgotten() {
        assert_eq!(
            dropped(&["NVIM", "NVIM_LISTEN_ADDRESS", "GUISE_ADDR", "NVIM_APPNAME"]),
            ["NVIM", "NVIM_LISTEN_ADDRESS", "GUISE_ADDR"]
        );
    }

    #[test]
    fn started_inside_claude_code_its_session_is_forgotten() {
        assert_eq!(
            dropped(&[
                "CLAUDECODE",
                "CLAUDE_CODE_CHILD_SESSION",
                "CLAUDE_CODE_SESSION_ID",
                "CLAUDE_CODE_ENTRYPOINT",
                "CLAUDE_PID",
                "PATH",
            ]),
            [
                "CLAUDECODE",
                "CLAUDE_CODE_CHILD_SESSION",
                "CLAUDE_CODE_SESSION_ID",
                "CLAUDE_CODE_ENTRYPOINT",
                "CLAUDE_PID",
            ]
        );
    }

    #[test]
    fn claude_code_settings_the_user_chose_are_kept() {
        assert_eq!(
            dropped(&["CLAUDECODE", "CLAUDE_CODE_USE_BEDROCK", "ANTHROPIC_MODEL"]),
            ["CLAUDECODE"]
        );
    }

    #[test]
    fn a_codex_child_forgets_its_parent_thread_but_keeps_user_configuration() {
        assert_eq!(
            dropped(&["CODEX_THREAD_ID", "CODEX_HOME", "OPENAI_API_KEY"]),
            ["CODEX_THREAD_ID"]
        );
    }

    #[test]
    fn outside_claude_code_nothing_of_it_is_dropped() {
        assert!(dropped(&["CLAUDE_EFFORT", "CLAUDE_CODE_SESSION_ID"]).is_empty());
    }
}
