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
    let env: Vec<(String, String)> = env
        .filter(|(name, _)| !decides(adapter, home, name))
        .collect();
    let mut line = vec![
        context.adroit.display().to_string(),
        SUBCOMMAND.to_string(),
        context.agent().key().to_string(),
        home.display().to_string(),
    ];
    // What it would remove of what it inherits but was handed here.
    let mut kept: Vec<&str> = env
        .iter()
        .map(|(name, _)| name.as_str())
        .filter(|name| adapter.removes(name))
        .collect();
    kept.sort_unstable();
    kept.dedup();
    for name in kept {
        line.extend([KEEP.to_string(), name.to_string()]);
    }
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

/// What names a variable [`SUBCOMMAND`] leaves as it is, one the launch sets
/// that its agent would otherwise remove of what it inherits.
const KEEP: &str = "--keep";

/// The prefix under which [`SUBCOMMAND`] keeps the value from outside the
/// connection of a variable the connection sets, for each its adapter
/// [`AgentAdapter::kept_outside`] names, so what the agent runs can put it
/// back.
pub const OUTSIDE: &str = "ADROIT_OUTSIDE_";

/// Whether a connection of its own in `home` decides on `name` alone: one of
/// the variables it sets, or one known to carry a login or to move the
/// agent's state.
fn decides<A: AgentAdapter + ?Sized>(adapter: &A, home: &Path, name: &str) -> bool {
    adapter.denies(name)
        || adapter
            .connection_variables(home)
            .iter()
            .any(|(set, _)| set == name)
}

/// Runs `adroit in-connection <agent> <home> [--keep <name>]… <program>
/// [<arg>…]`: replaces this process with `program`, with none of the
/// variables `agent`'s connection removes but those kept, and the variables
/// it sets naming `home` and the folders in it. Returns only when that fails,
/// with the exit status to end with.
pub fn run(args: &[String]) -> i32 {
    use std::os::unix::process::CommandExt;
    match command(args, std::env::vars_os()) {
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

/// What [`run`] replaces itself with, given `args` and the variables it has.
fn command(
    args: &[String],
    vars: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> Result<std::process::Command, String> {
    let usage =
        || format!("usage: adroit {SUBCOMMAND} <agent> <home> [{KEEP} <name>]… <program> [<arg>…]");
    let [agent, home, rest @ ..] = args else {
        return Err(usage());
    };
    let mut rest: &[String] = rest;
    let mut kept = Vec::new();
    while let [flag, name, more @ ..] = rest
        && flag == KEEP
    {
        kept.push(name.as_str());
        rest = more;
    }
    let [program, rest @ ..] = rest else {
        return Err(usage());
    };
    let adapter = crate::adapter(adroit_model::Agent::parse(agent).ok_or_else(usage)?);
    let home = Path::new(home);
    let set = adapter.connection_variables(home);
    let mut command = std::process::Command::new(program);
    command.args(rest);
    // Empty for one unset outside, which is put back unset.
    for name in adapter.kept_outside() {
        command.env(format!("{OUTSIDE}{name}"), "");
    }
    for (name, value) in vars {
        let Some(n) = name.to_str() else {
            continue;
        };
        if adapter.kept_outside().contains(&n) {
            command.env(format!("{OUTSIDE}{n}"), &value);
        }
        if decides(adapter, home, n) || (adapter.removes(n) && !kept.contains(&n)) {
            command.env_remove(&name);
        }
    }
    command.envs(set);
    Ok(command)
}

/// What puts back, in a process started inside an agent's connection, the
/// variables from outside it that [`SUBCOMMAND`] kept, given the variables the
/// process has: each with its value, or none to remove it. What adroit runs
/// there, such as its MCP server or `adroit terminal`, then finds the
/// instance it belongs to rather than the agent's state.
pub fn outside(
    vars: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> Vec<(std::ffi::OsString, Option<std::ffi::OsString>)> {
    let mut put = Vec::new();
    for (name, value) in vars {
        let Some(outside) = name.to_str().and_then(|n| n.strip_prefix(OUTSIDE)) else {
            continue;
        };
        let value = (!value.is_empty()).then_some(value);
        put.push((outside.into(), value));
        put.push((name, None));
    }
    put
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
    let home = context.home.as_ref()?;
    context
        .launch
        .env
        .keys()
        .find(|name| decides(adapter, home, name))
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
        let command = command(&args, names.iter().map(|n| (n.into(), "outside".into()))).unwrap();
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

    #[test]
    fn opencode_in_an_account_of_its_own_is_told_to_keep_what_its_launch_sets() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("oc");
        let mut context = in_connection(Agent::OpenCode, &home);
        context
            .launch
            .env
            .insert("OLLAMA_API_KEY".into(), "mine".into());
        let (program, args, env) = adapter(Agent::OpenCode).launch(
            &context,
            vec!["--standalone".into()],
            vec![("TERMINFO_DIRS".into(), "/t".into())],
        );
        assert_eq!(program, "/adroit");
        let home = home.display().to_string();
        assert_eq!(
            args,
            [
                SUBCOMMAND,
                "opencode",
                &home,
                KEEP,
                "OLLAMA_API_KEY",
                "opencode",
                "--standalone"
            ]
        );
        assert!(env.contains(&("OLLAMA_API_KEY".into(), "mine".into())));
        assert_eq!(conflict(adapter(Agent::OpenCode), &context), None);
        context
            .launch
            .env
            .insert("XDG_DATA_HOME".into(), "/x".into());
        assert_eq!(
            conflict(adapter(Agent::OpenCode), &context).as_deref(),
            Some("XDG_DATA_HOME")
        );
    }

    #[test]
    fn opencode_s_account_removes_all_it_does_not_allow_and_keeps_its_roots_and_theirs_outside() {
        let args: Vec<String> = ["opencode", "/w/oc", KEEP, "OLLAMA_API_KEY", "program", "a"]
            .map(String::from)
            .to_vec();
        let vars = [
            ("PATH", "/bin"),
            ("ANTHROPIC_API_KEY", "k"),
            ("GITHUB_TOKEN", "t"),
            ("OLLAMA_API_KEY", "mine"),
            ("OPENCODE_CONFIG_DIR", "/x"),
            ("OPENCODE_CONFIG_CONTENT", "{}"),
            ("XDG_CONFIG_HOME", "/me/config"),
            ("XDG_RUNTIME_DIR", "/r"),
            ("ADROIT_DELEGATION_TOKEN", "tok"),
        ];
        let command = command(&args, vars.iter().map(|(n, v)| (n.into(), v.into()))).unwrap();
        assert_eq!(command.get_program(), "program");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["a"]);
        let envs: std::collections::BTreeMap<String, Option<String>> = command
            .get_envs()
            .map(|(n, v)| {
                (
                    n.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        let value = |name: &str| envs.get(name).cloned();
        for removed in [
            "ANTHROPIC_API_KEY",
            "GITHUB_TOKEN",
            "OPENCODE_CONFIG_DIR",
            "OPENCODE_CONFIG_CONTENT",
        ] {
            assert_eq!(value(removed), Some(None), "{removed}");
        }
        for left in [
            "PATH",
            "OLLAMA_API_KEY",
            "XDG_RUNTIME_DIR",
            "ADROIT_DELEGATION_TOKEN",
        ] {
            assert_eq!(value(left), None, "{left}");
        }
        assert_eq!(value("XDG_CONFIG_HOME"), Some(Some("/w/oc/config".into())));
        assert_eq!(value("XDG_STATE_HOME"), Some(Some("/w/oc/state".into())));
        assert_eq!(
            value(&format!("{OUTSIDE}XDG_CONFIG_HOME")),
            Some(Some("/me/config".into()))
        );
        assert_eq!(
            value(&format!("{OUTSIDE}XDG_STATE_HOME")),
            Some(Some(String::new()))
        );
    }

    #[test]
    fn what_a_connection_kept_from_outside_it_is_put_back_and_one_unset_removed() {
        let put = outside(
            [
                (format!("{OUTSIDE}XDG_STATE_HOME"), "/me/state"),
                (format!("{OUTSIDE}XDG_DATA_HOME"), ""),
                ("XDG_STATE_HOME".into(), "/w/oc/state"),
            ]
            .into_iter()
            .map(|(n, v)| (n.into(), v.into())),
        );
        let put: Vec<(String, Option<String>)> = put
            .into_iter()
            .map(|(n, v)| {
                (
                    n.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        assert_eq!(
            put,
            [
                ("XDG_STATE_HOME".into(), Some("/me/state".into())),
                (format!("{OUTSIDE}XDG_STATE_HOME"), None),
                ("XDG_DATA_HOME".into(), None),
                (format!("{OUTSIDE}XDG_DATA_HOME"), None),
            ]
        );
    }

    #[test]
    fn a_deny_list_agent_keeps_nothing_it_would_remove_and_keeps_no_value_from_outside() {
        let dir = tempfile::tempdir().unwrap();
        let context = in_connection(Agent::Pi, dir.path());
        let (_, args, _) = adapter(Agent::Pi).launch(
            &context,
            vec![],
            vec![("ADROIT_TEST_OUT".into(), "/o".into())],
        );
        assert!(!args.iter().any(|a| a == KEEP), "{args:?}");
        let (_, set) = cleared(Agent::Pi, "/w/pi", &["PI_CODING_AGENT_DIR"]);
        assert_eq!(set, ["PI_CODING_AGENT_DIR=/w/pi"]);
    }
}
