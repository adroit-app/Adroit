//! What the `adroit` command was asked to do.

/// What `adroit` does, by its arguments.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Open the window.
    Window,
    Version,
    /// A subcommand, and the arguments that follow it.
    Shell(Vec<String>),
    Editor(Vec<String>),
    Session(Vec<String>),
    Daemon,
}

pub const USAGE: &str = "usage: adroit [--version | shell … | editor … | session … | daemon]";

/// The command `args` ask for; `None` for one adroit does not know, which
/// must not open a window: a program that ran an older adroit's subcommand
/// would open one each time.
pub fn command(args: &[String]) -> Option<Command> {
    let rest = || args[1..].to_vec();
    match args.first().map(String::as_str) {
        None => Some(Command::Window),
        // What macOS once added when it launched an app from the Finder.
        Some(a) if a.starts_with("-psn_") && args.len() == 1 => Some(Command::Window),
        Some("--version") if args.len() == 1 => Some(Command::Version),
        Some("shell") => Some(Command::Shell(rest())),
        Some("editor") => Some(Command::Editor(rest())),
        Some("session") => Some(Command::Session(rest())),
        Some("daemon") if args.len() == 1 => Some(Command::Daemon),
        _ => None,
    }
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
        assert_eq!(asked(&[]), Some(Command::Window));
    }

    #[test]
    fn the_process_serial_number_macos_once_passed_opens_the_window() {
        assert_eq!(asked(&["-psn_0_12345"]), Some(Command::Window));
    }

    #[test]
    fn a_subcommand_takes_the_arguments_after_it() {
        assert_eq!(
            asked(&["shell", "screen", "dev"]),
            Some(Command::Shell(vec!["screen".into(), "dev".into()]))
        );
        assert_eq!(
            asked(&["editor", "/tmp/p.md"]),
            Some(Command::Editor(vec!["/tmp/p.md".into()]))
        );
        assert_eq!(
            asked(&["session", "start"]),
            Some(Command::Session(vec!["start".into()]))
        );
        assert_eq!(asked(&["daemon"]), Some(Command::Daemon));
        assert_eq!(asked(&["--version"]), Some(Command::Version));
    }

    #[test]
    fn an_unknown_subcommand_or_option_opens_no_window() {
        assert_eq!(asked(&["hook"]), None);
        assert_eq!(asked(&["--bogus"]), None);
        assert_eq!(asked(&["daemon", "extra"]), None);
    }
}
