//! How an agent is started in a connection of its own: in its state
//! directory, with nothing of another login reaching it.

use std::path::Path;

use crate::{AgentAdapter, AgentContext, Started};

/// What runs the agent of `context` with `args` and `env`: in the agent's own
/// environment, as its launch says; in a connection of its own, through
/// adroit's [`SUBCOMMAND`], which, once the shell the launch may start it
/// through has read its startup files, removes every variable that would
/// carry another login and names the connection's state directory.
pub(crate) fn started<A: AgentAdapter + ?Sized>(
    adapter: &A,
    context: &AgentContext,
    args: Vec<String>,
    env: Vec<(String, String)>,
) -> Started {
    let launch = &context.launch;
    let env = env.into_iter().chain(launch.env.clone());
    let Some(home) = &context.home else {
        let (program, args) = launch.command_line(&adapter.program(), &args, &context.shell);
        return (program, args, env.collect());
    };
    // Made, never filled: the agent's program logs in and keeps its state
    // there itself.
    let _ = make_private(home);
    let state = adapter.state_variable();
    let own = |name: &str| name == state || adapter.denies(name);
    let env: Vec<(String, String)> = env.filter(|(name, _)| !own(name)).collect();
    let mut line = vec![
        context.adroit.display().to_string(),
        SUBCOMMAND.to_string(),
        context.agent().key().to_string(),
        home.display().to_string(),
    ];
    match launch.command.is_empty() {
        true => line.push(adapter.program()),
        false => line.extend(launch.command.iter().cloned()),
    }
    let args: Vec<String> = adapter.connection_args().into_iter().chain(args).collect();
    let through = adroit_model::config::Launch {
        command: line,
        ..launch.clone()
    };
    let (program, args) = through.command_line("", &args, &context.shell);
    (program, args, env)
}

/// The `adroit` subcommand that clears and sets the variables last, run by
/// adroit's whole path so a `PATH` the shell's startup files set cannot
/// replace it. `env -u` would remove only the variables named to it, not
/// every one of a family, nor those the terminal host alone holds.
pub const SUBCOMMAND: &str = "in-connection";

/// Runs `adroit in-connection <agent> <home> <program> [<arg>…]`: replaces
/// this process with `program`, with none of the variables `agent`'s
/// connection removes and its state variable naming `home`. Returns only
/// when that fails, with the exit status to end with.
pub fn run(args: &[String]) -> i32 {
    use std::os::unix::process::CommandExt;
    match command(args, std::env::vars_os().map(|(name, _)| name)) {
        Ok(mut command) => {
            let error = command.exec();
            eprintln!(
                "adroit: cannot start {}: {error}",
                command.get_program().to_string_lossy()
            );
            127
        }
        Err(usage) => {
            eprintln!("adroit: {usage}");
            2
        }
    }
}

/// What [`run`] replaces itself with, given `args` and the names of the
/// variables it has.
fn command(
    args: &[String],
    names: impl Iterator<Item = std::ffi::OsString>,
) -> Result<std::process::Command, String> {
    let usage = || format!("usage: adroit {SUBCOMMAND} <agent> <home> <program> [<arg>…]");
    let [agent, home, program, rest @ ..] = args else {
        return Err(usage());
    };
    let adapter = crate::adapter(adroit_model::Agent::parse(agent).ok_or_else(usage)?);
    let state = adapter.state_variable();
    let mut command = std::process::Command::new(program);
    command.args(rest);
    for name in names {
        if name
            .to_str()
            .is_some_and(|n| n == state || adapter.denies(n))
        {
            command.env_remove(name);
        }
    }
    command.env(state, home);
    Ok(command)
}

/// Makes `dir` and the directories above it that are missing, readable by
/// the user alone.
pub fn make_private(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

/// The first variable `context`'s launch sets that its connection decides on
/// alone: its state directory, or one carrying another login; none in the
/// agent's own environment, where the launch sets what it likes.
pub fn conflict<A: AgentAdapter + ?Sized>(adapter: &A, context: &AgentContext) -> Option<String> {
    context.home.as_ref()?;
    let state = adapter.state_variable();
    context
        .launch
        .env
        .keys()
        .find(|name| *name == state || adapter.denies(name))
        .cloned()
}

#[cfg(test)]
mod tests {
    use adroit_model::Agent;

    use super::*;
    use crate::adapter;

    fn in_connection(agent: Agent, home: &Path) -> AgentContext {
        AgentContext {
            home: Some(home.to_path_buf()),
            shell: "/bin/zsh".into(),
            adroit: "/adroit".into(),
            ..AgentContext::builtin(agent)
        }
    }

    /// What `adroit in-connection` started for `agent` in `home`, with the
    /// variables `names`, removes, and what it sets, each in name order.
    fn cleared(agent: Agent, home: &str, names: &[&str]) -> (Vec<String>, Vec<String>) {
        let args: Vec<String> = [agent.key(), home, "program", "a"]
            .map(String::from)
            .to_vec();
        let command = command(&args, names.iter().map(Into::into)).unwrap();
        assert_eq!(command.get_program(), "program");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["a"]);
        let (mut removed, mut set) = (Vec::new(), Vec::new());
        for (name, value) in command.get_envs() {
            let name = name.to_string_lossy().into_owned();
            match value {
                None => removed.push(name),
                Some(value) => set.push(format!("{name}={}", value.to_string_lossy())),
            }
        }
        (removed, set)
    }

    #[test]
    fn claude_code_in_a_connection_of_its_own_keeps_its_state_there_with_no_other_login() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("claude-work");
        let context = in_connection(Agent::Claude, &home);
        let (program, args, env) = adapter(Agent::Claude).launch(
            &context,
            vec!["--x".into()],
            vec![("ANTHROPIC_API_KEY".into(), "k".into())],
        );
        assert_eq!(program, "/adroit");
        let home = home.display().to_string();
        assert_eq!(
            args,
            [
                SUBCOMMAND,
                "claude",
                &home,
                &crate::claude::claude_program(),
                "--x"
            ]
        );
        assert!(env.iter().all(|(name, _)| !name.starts_with("ANTHROPIC_")));
        let mode = std::fs::metadata(&home).unwrap().permissions();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(mode.mode() & 0o777, 0o700);
    }

    #[test]
    fn every_variable_of_claude_code_s_families_is_removed_and_its_state_directory_named() {
        let names = [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_PROFILE",
            "ANTHROPIC_CUSTOM_HEADERS",
            "ANTHROPIC_VERTEX_ANYTHING",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_OAUTH_REFRESH_TOKEN",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_SOMETHING_NEW",
            "CLAUDE_CODE_SKIP_NEW_AUTH",
            "CLAUDE_SECURESTORAGE_CONFIG_DIR",
            "CLAUDE_CODE_CUSTOM_OAUTH_URL",
            "CLAUDE_CODE_PROJECT_DIR_NAME",
        ];
        let (removed, set) = cleared(
            Agent::Claude,
            "/w/claude",
            &[&names[..], &["CLAUDE_CONFIG_DIR", "PATH", "HOME"]].concat(),
        );
        let mut names = names.to_vec();
        names.sort();
        assert_eq!(removed, names);
        assert_eq!(set, ["CLAUDE_CONFIG_DIR=/w/claude"]);
    }

    #[test]
    fn codex_in_a_connection_of_its_own_keeps_its_logins_and_mcp_tokens_there() {
        let dir = tempfile::tempdir().unwrap();
        let context = in_connection(Agent::Codex, dir.path());
        let (_, args, _) =
            adapter(Agent::Codex).launch(&context, vec!["app-server".into()], vec![]);
        let home = dir.path().display().to_string();
        assert_eq!(
            args,
            [
                SUBCOMMAND,
                "codex",
                &home,
                "codex",
                "-c",
                "mcp_oauth_credentials_store=\"file\"",
                "app-server"
            ]
        );
        let (removed, set) = cleared(
            Agent::Codex,
            "/w/codex",
            &[
                "OPENAI_API_KEY",
                "OPENAI_ANYTHING",
                "CODEX_ACCESS_TOKEN",
                "CODEX_SQLITE_HOME",
                "CODEX_HOME",
                "PATH",
            ],
        );
        assert_eq!(
            removed,
            [
                "CODEX_ACCESS_TOKEN",
                "CODEX_SQLITE_HOME",
                "OPENAI_ANYTHING",
                "OPENAI_API_KEY"
            ]
        );
        assert_eq!(set, ["CODEX_HOME=/w/codex"]);
    }

    #[test]
    fn the_state_directory_is_named_exactly_as_written() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("a b");
        let context = in_connection(Agent::Claude, &home);
        let (_, args, _) = adapter(Agent::Claude).launch(&context, vec![], vec![]);
        assert_eq!(args[2], home.display().to_string());
        assert!(home.is_dir());
        let (_, set) = cleared(Agent::Claude, "/w/a b/", &[]);
        assert_eq!(set, ["CLAUDE_CONFIG_DIR=/w/a b/"]);
    }

    #[test]
    fn a_connection_s_last_step_names_an_agent_its_home_and_a_program() {
        for args in [&[][..], &["claude", "/w"], &["nobody", "/w", "program"]] {
            let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            assert!(command(&args, std::iter::empty()).is_err(), "{args:?}");
        }
    }

    #[test]
    fn started_through_the_shell_the_variables_are_cleared_after_its_startup_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut context = in_connection(Agent::Claude, dir.path());
        context.launch.via_shell = true;
        context.launch.command = vec!["/opt/claude".into()];
        let (program, args, _) = adapter(Agent::Claude).launch(&context, vec![], vec![]);
        assert_eq!(program, "/bin/zsh");
        let exec = args.iter().position(|a| a == "exec \"$@\"").unwrap();
        assert_eq!(args[exec + 2..exec + 4], ["/adroit", SUBCOMMAND]);
        assert!(args.ends_with(&["/opt/claude".to_string()]));
    }

    #[test]
    fn in_the_agent_s_own_environment_nothing_is_cleared_or_named() {
        for agent in Agent::ALL {
            let mut context = AgentContext::builtin(agent);
            context
                .launch
                .env
                .insert("ANTHROPIC_API_KEY".into(), "mine".into());
            let (_, args, env) = adapter(agent).launch(&context, vec!["a".into()], vec![]);
            assert_eq!(args, ["a"]);
            assert!(env.contains(&("ANTHROPIC_API_KEY".into(), "mine".into())));
            assert_eq!(conflict(adapter(agent), &context), None);
        }
    }

    #[test]
    fn a_launch_that_sets_what_its_connection_decides_on_is_named() {
        let mut context = in_connection(Agent::Codex, Path::new("/c"));
        assert_eq!(conflict(adapter(Agent::Codex), &context), None);
        context.launch.env.insert("FOO".into(), "1".into());
        assert_eq!(conflict(adapter(Agent::Codex), &context), None);
        context
            .launch
            .env
            .insert("OPENAI_ORGANIZATION".into(), "o".into());
        assert_eq!(
            conflict(adapter(Agent::Codex), &context).as_deref(),
            Some("OPENAI_ORGANIZATION")
        );
        let mut context = in_connection(Agent::Claude, Path::new("/c"));
        context
            .launch
            .env
            .insert("CLAUDE_CONFIG_DIR".into(), "/x".into());
        assert_eq!(
            conflict(adapter(Agent::Claude), &context).as_deref(),
            Some("CLAUDE_CONFIG_DIR")
        );
    }
}
