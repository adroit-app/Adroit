//! What the `adroit` command was asked to do.

use crate::paths::{DEFAULT_INSTANCE, INSTANCE_ENV, instance_state, is_instance};
use crate::window_socket::{Request, Response, ask, socket_path};

/// What `adroit` does, by its arguments.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Open the window of the instance named, or of the one the environment
    /// names.
    Window(Option<String>),
    Version,
    /// A subcommand, and the arguments that follow it.
    Terminal(Vec<String>),
    /// Serve a session's `adroit` MCP server on standard input and output.
    Mcp(Vec<String>),
    Editor(Vec<String>),
    Session(Vec<String>),
    Daemon(DaemonCommand),
}

/// What `adroit daemon` is asked to do with the daemon of the instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonCommand {
    Serve,
    Stop,
    Restart,
}

pub const USAGE: &str = "usage: adroit [--instance <name> | --version | terminal … | mcp … | editor … | session … | daemon [stop | restart]]";

/// The command `args` ask for; `None` for one adroit does not know, which
/// must not open a window: a program that ran an older adroit's subcommand
/// would open one each time.
pub fn command(args: &[String]) -> Option<Command> {
    let rest = || args[1..].to_vec();
    match args.first().map(String::as_str) {
        None => Some(Command::Window(None)),
        // What macOS once added when it launched an app from the Finder.
        Some(a) if a.starts_with("-psn_") && args.len() == 1 => Some(Command::Window(None)),
        Some("--instance") => match &args[1..] {
            [name] if is_instance(name) => Some(Command::Window(Some(name.clone()))),
            _ => None,
        },
        Some("--version") if args.len() == 1 => Some(Command::Version),
        Some("mcp") => Some(Command::Mcp(rest())),
        Some("terminal") => Some(Command::Terminal(rest())),
        Some("editor") => Some(Command::Editor(rest())),
        Some("session") => Some(Command::Session(rest())),
        Some("daemon") => match &args[1..] {
            [] => Some(Command::Daemon(DaemonCommand::Serve)),
            [verb] if verb == "stop" => Some(Command::Daemon(DaemonCommand::Stop)),
            [verb] if verb == "restart" => Some(Command::Daemon(DaemonCommand::Restart)),
            _ => None,
        },
        _ => None,
    }
}

/// The instance named `given` on the command line, else the one `env` names,
/// else the default one.
fn chosen(given: Option<String>, env: Option<String>) -> Result<String, String> {
    match given.or(env.filter(|n| !n.is_empty())) {
        None => Ok(DEFAULT_INSTANCE.into()),
        Some(name) if is_instance(&name) => Ok(name),
        Some(name) => Err(format!("{INSTANCE_ENV} names no instance: {name:?}")),
    }
}

/// Settle which instance this process belongs to, and name it in the
/// environment every process it starts inherits. Call before any other
/// thread exists.
pub fn settle_instance(given: Option<String>) -> Result<(), String> {
    let name = chosen(given, std::env::var(INSTANCE_ENV).ok())?;
    // SAFETY: called before any other thread exists.
    unsafe { std::env::set_var(INSTANCE_ENV, name) };
    Ok(())
}

/// Ask the window of this instance, if one is open, to come to the front;
/// whether it did, leaving this process nothing to open.
pub fn hand_to_open_window() -> bool {
    let socket = socket_path(&instance_state());
    matches!(ask(&socket, &Request::Show), Ok(Response::Shown))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(words: &[&str]) -> Option<Command> {
        let args: Vec<String> = words.iter().map(|w| w.to_string()).collect();
        command(&args)
    }

    #[test]
    fn no_arguments_open_the_window() {
        assert_eq!(asked(&[]), Some(Command::Window(None)));
    }

    #[test]
    fn the_process_serial_number_macos_once_passed_opens_the_window() {
        assert_eq!(asked(&["-psn_0_12345"]), Some(Command::Window(None)));
    }

    #[test]
    fn a_subcommand_takes_the_arguments_after_it() {
        assert_eq!(
            asked(&["terminal", "screen", "dev"]),
            Some(Command::Terminal(vec!["screen".into(), "dev".into()]))
        );
        assert_eq!(
            asked(&["editor", "/tmp/p.md"]),
            Some(Command::Editor(vec!["/tmp/p.md".into()]))
        );
        assert_eq!(
            asked(&["session", "start"]),
            Some(Command::Session(vec!["start".into()]))
        );
        assert_eq!(
            asked(&["daemon"]),
            Some(Command::Daemon(DaemonCommand::Serve))
        );
        assert_eq!(
            asked(&["daemon", "stop"]),
            Some(Command::Daemon(DaemonCommand::Stop))
        );
        assert_eq!(
            asked(&["daemon", "restart"]),
            Some(Command::Daemon(DaemonCommand::Restart))
        );
        assert_eq!(asked(&["--version"]), Some(Command::Version));
    }

    #[test]
    fn the_window_of_a_named_instance_is_opened_by_its_name() {
        assert_eq!(
            asked(&["--instance", "work"]),
            Some(Command::Window(Some("work".into())))
        );
        assert_eq!(asked(&["--instance"]), None);
        assert_eq!(asked(&["--instance", "../up"]), None);
        assert_eq!(asked(&["--instance", "work", "extra"]), None);
    }

    #[test]
    fn the_instance_named_on_the_command_line_wins_over_the_environment() {
        assert_eq!(
            chosen(Some("work".into()), Some("dev".into())),
            Ok("work".into())
        );
        assert_eq!(chosen(None, Some("dev".into())), Ok("dev".into()));
    }

    #[test]
    fn without_a_name_the_instance_is_the_default_one() {
        assert_eq!(chosen(None, None), Ok("default".into()));
        assert_eq!(chosen(None, Some(String::new())), Ok("default".into()));
    }

    #[test]
    fn an_instance_the_environment_names_badly_is_refused() {
        assert!(chosen(None, Some("../up".into())).is_err());
    }

    #[test]
    fn an_unknown_subcommand_or_option_opens_no_window() {
        assert_eq!(asked(&["hook"]), None);
        assert_eq!(asked(&["--bogus"]), None);
        assert_eq!(asked(&["daemon", "extra"]), None);
        assert_eq!(asked(&["daemon", "stop", "extra"]), None);
    }
}
