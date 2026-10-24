//! A connection's login, which its agent's CLI keeps: asking the CLI whether
//! it is logged in, and what starts the CLI's own login in a terminal.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use adroit_model::profile::LoginStanding;

use crate::{AgentContext, Started, adapter};

/// How long the CLI has to say whether it is logged in.
const ASKED_FOR: Duration = Duration::from_secs(20);

/// What starts the CLI's own login in `context`'s connection, for a terminal
/// to run.
pub fn login(context: &AgentContext) -> Started {
    let adapter = adapter(context.agent());
    adapter.launch(context, adapter.login_args(), Vec::new())
}

/// Whether the CLI says `context`'s connection is logged in, asked in `dir`.
pub async fn standing(context: &AgentContext, dir: &Path) -> LoginStanding {
    let adapter = adapter(context.agent());
    let (program, args, env) = adapter.launch(context, adapter.status_args(), Vec::new());
    let child = tokio::process::Command::new(&program)
        .args(args)
        .envs(env)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return LoginStanding::Missing,
        Err(_) => return LoginStanding::Unknown,
    };
    match tokio::time::timeout(ASKED_FOR, child.wait_with_output()).await {
        Ok(Ok(out)) => adapter.standing(out.status.code(), &String::from_utf8_lossy(&out.stdout)),
        _ => LoginStanding::Unknown,
    }
}

/// What a CLI that answers whether it is logged in by its exit status alone
/// says: 0 when it is, 1 when it is not; 126 or 127, as a shell and
/// `adroit in-connection` end when the program cannot be started, when it is
/// not installed.
pub(crate) fn by_status(code: Option<i32>) -> LoginStanding {
    match code {
        Some(0) => LoginStanding::LoggedIn,
        Some(1) => LoginStanding::LoggedOut,
        Some(126 | 127) => LoginStanding::Missing,
        _ => LoginStanding::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use adroit_model::Agent;

    use super::*;

    /// A context whose CLI is a script that prints `out` and ends with `code`.
    fn answering(agent: Agent, dir: &Path, out: &str, code: i32) -> AgentContext {
        let script = dir.join("cli");
        std::fs::write(
            &script,
            format!("#!/bin/sh\nprintf '%s' '{out}'\nexit {code}\n"),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut context = AgentContext::builtin(agent);
        context.launch.command = vec![script.display().to_string()];
        context
    }

    #[tokio::test]
    async fn claude_code_is_logged_in_only_when_its_status_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let asked = |out: &str, code| {
            let context = answering(Agent::Claude, dir.path(), out, code);
            async move { standing(&context, Path::new("/")).await }
        };
        assert_eq!(
            asked(r#"{"loggedIn": true}"#, 0).await,
            LoginStanding::LoggedIn
        );
        assert_eq!(
            asked(r#"{"loggedIn": false}"#, 1).await,
            LoginStanding::LoggedOut
        );
        assert_eq!(
            asked(r#"{"loggedIn": false}"#, 0).await,
            LoginStanding::LoggedOut
        );
        // A shell's startup files may print ahead of the answer.
        assert_eq!(
            asked("welcome\n{\"loggedIn\": true}", 0).await,
            LoginStanding::LoggedIn
        );
        assert_eq!(asked("fine", 0).await, LoginStanding::Unknown);
        // A CLI that fails without answering, too old for `--json` say.
        assert_eq!(
            asked("error: unknown option '--json'", 1).await,
            LoginStanding::Unknown
        );
        assert_eq!(asked("", 3).await, LoginStanding::Unknown);
    }

    #[tokio::test]
    async fn codex_is_logged_in_when_its_status_ends_well() {
        let dir = tempfile::tempdir().unwrap();
        for (code, standing_is) in [
            (0, LoginStanding::LoggedIn),
            (1, LoginStanding::LoggedOut),
            (2, LoginStanding::Unknown),
        ] {
            let context = answering(Agent::Codex, dir.path(), "", code);
            assert_eq!(standing(&context, Path::new("/")).await, standing_is);
        }
    }

    #[tokio::test]
    async fn a_cli_that_is_not_installed_is_told_apart() {
        for agent in Agent::ALL {
            let mut context = AgentContext::builtin(agent);
            context.launch.command = vec!["/nonexistent/adroit-test-cli".into()];
            assert_eq!(
                standing(&context, Path::new("/")).await,
                LoginStanding::Missing
            );
            // Through the shell, which ends with 127 when it finds no program.
            context.launch.via_shell = true;
            context.shell = "/bin/sh".into();
            assert_eq!(
                standing(&context, Path::new("/")).await,
                LoginStanding::Missing
            );
        }
    }

    #[test]
    fn the_login_and_its_status_are_the_cli_s_own_commands() {
        let claude = login(&AgentContext::builtin(Agent::Claude));
        assert_eq!(claude.1, ["auth", "login"]);
        let codex = login(&AgentContext::builtin(Agent::Codex));
        assert_eq!(
            (codex.0.as_str(), codex.1.as_slice()),
            ("codex", &["login".to_string()][..])
        );
        assert_eq!(
            adapter(Agent::Claude).status_args(),
            ["auth", "status", "--json"]
        );
        assert_eq!(adapter(Agent::Codex).status_args(), ["login", "status"]);
    }

    #[test]
    fn a_login_in_a_connection_of_its_own_runs_in_its_state_directory() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("work");
        let context = AgentContext {
            home: Some(home.clone()),
            adroit: "/adroit".into(),
            ..AgentContext::builtin(Agent::Codex)
        };
        let (program, args, _) = login(&context);
        assert_eq!(program, "/adroit");
        let line = args.join(" ");
        assert!(
            line.starts_with(&format!("in-connection codex {} codex", home.display())),
            "{line}"
        );
        assert!(line.ends_with("login"), "{line}");
    }
}
