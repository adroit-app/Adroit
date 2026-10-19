//! How the programs adroit runs in its console are started: the shell a
//! shell tab runs, and Claude Code, each as the settings say.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// How a shell tab's shell is started.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ShellLaunch {
    /// The shell and its arguments; empty for the account's login shell,
    /// started as a login shell.
    pub command: Vec<String>,
    /// Variables set for it over adroit's environment.
    pub env: BTreeMap<String, String>,
}

impl ShellLaunch {
    /// The program and arguments a shell tab runs, `login` being the
    /// account's login shell.
    pub fn command_line(&self, login: &str) -> (String, Vec<String>) {
        match self.command.split_first() {
            Some((program, args)) => (program.clone(), args.to_vec()),
            None => (login.to_string(), vec!["-l".into()]),
        }
    }

    /// The shell itself, which a program started through the shell runs in.
    pub fn program(&self, login: &str) -> String {
        self.command_line(login).0
    }
}

/// How a program other than the shell is started.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Launch {
    /// The program and the arguments it always gets, ahead of adroit's own;
    /// empty for adroit's choice of program.
    pub command: Vec<String>,
    /// Start it through the shell, as an interactive login shell, so what
    /// the user's startup files set reaches it.
    pub via_shell: bool,
    /// Variables set for it over adroit's environment.
    pub env: BTreeMap<String, String>,
}

impl Launch {
    /// The program and arguments that run `args` for this program, `fallback`
    /// standing in for an empty command and `shell` being the shell it is
    /// started through.
    pub fn command_line(
        &self,
        fallback: &str,
        args: &[String],
        shell: &str,
    ) -> (String, Vec<String>) {
        let mut line: Vec<String> = if self.command.is_empty() {
            vec![fallback.to_string()]
        } else {
            self.command.clone()
        };
        line.extend(args.iter().cloned());
        if !self.via_shell {
            let program = line.remove(0);
            return (program, line);
        }
        let fish = Path::new(shell).file_name().is_some_and(|n| n == "fish");
        // Handed over as arguments rather than written into the script, the
        // words need no quoting for the shell.
        let mut through: Vec<String> = vec!["-l".into(), "-i".into(), "-c".into()];
        if fish {
            through.push("exec $argv".into());
        } else {
            through.push("exec \"$@\"".into());
            through.push("adroit".into());
        }
        through.extend(line);
        (shell.to_string(), through)
    }
}

/// How sessions of one kind run: Claude Code's model, its permission mode and
/// the tools denied; what is left empty is Claude Code's own choice.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SessionOptions {
    pub model: String,
    pub permission_mode: String,
    pub disallowed_tools: Vec<String>,
}

impl SessionOptions {
    pub fn options(&self) -> adroit_claude::Options {
        adroit_claude::Options {
            model: self.model.trim().to_string(),
            permission_mode: self.permission_mode.trim().to_string(),
            disallowed_tools: self
                .disallowed_tools
                .iter()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect(),
        }
    }
}

/// How a reader runs: as [`SessionOptions`], but what a table leaves out is
/// a reader's own, planning with the tools that edit files denied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReaderOptions {
    pub model: String,
    pub permission_mode: String,
    pub disallowed_tools: Vec<String>,
}

impl Default for ReaderOptions {
    fn default() -> Self {
        Self {
            model: String::new(),
            permission_mode: "plan".into(),
            disallowed_tools: ["Edit", "Write", "NotebookEdit"].map(String::from).to_vec(),
        }
    }
}

impl From<ReaderOptions> for SessionOptions {
    fn from(r: ReaderOptions) -> Self {
        Self {
            model: r.model,
            permission_mode: r.permission_mode,
            disallowed_tools: r.disallowed_tools,
        }
    }
}

impl From<SessionOptions> for ReaderOptions {
    fn from(s: SessionOptions) -> Self {
        Self {
            model: s.model,
            permission_mode: s.permission_mode,
            disallowed_tools: s.disallowed_tools,
        }
    }
}

impl ReaderOptions {
    pub fn options(&self) -> adroit_claude::Options {
        SessionOptions::from(self.clone()).options()
    }
}

/// A command written as a shell would split it into words; `None` while a
/// quote is left open.
pub fn parse_command(text: &str) -> Option<Vec<String>> {
    shell_words::split(text).ok()
}

/// `command` written as [`parse_command`] reads it.
pub fn format_command(command: &[String]) -> String {
    shell_words::join(command)
}

/// Variables written one a line as `NAME=value`; a line without `=`, a
/// blank one or one starting with `#` says nothing.
pub fn parse_env(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

/// `env` written as [`parse_env`] reads it.
pub fn format_env(env: &BTreeMap<String, String>) -> String {
    env.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The account's login shell, or `/bin/sh` when it cannot be told.
pub fn login_shell() -> String {
    crate::env::login_shell().unwrap_or_else(|| "/bin/sh".into())
}

/// What starts Claude Code with `args`, as the settings say: the program,
/// its arguments, and `env` with the variables the settings add.
pub fn claude(
    args: Vec<String>,
    env: Vec<(String, String)>,
) -> (String, Vec<String>, Vec<(String, String)>) {
    let s = crate::settings::current();
    let shell = s.shell.program(&login_shell());
    let (program, args) = s
        .claude
        .command_line(&adroit_claude::claude_program(), &args, &shell);
    (program, args, env.into_iter().chain(s.claude.env).collect())
}

/// What starts Claude Code with `args` to ask it something outside a console,
/// as the settings start a session.
pub fn claude_asked(args: Vec<String>) -> adroit_claude::Started {
    claude(args, Vec::new())
}

/// What starts a Neovim tab's Neovim, as the settings say: the program, its
/// arguments, and the variables the settings set for it.
pub fn neovim() -> (String, Vec<String>, Vec<(String, String)>) {
    let s = crate::settings::current();
    let shell = s.shell.program(&login_shell());
    let (program, args) = s.neovim.command_line("nvim", &[], &shell);
    (program, args, s.neovim.env.into_iter().collect())
}

/// What starts a shell tab's shell, as the settings say: the program, its
/// arguments, and the variables it gets, `SHELL` naming it.
pub fn shell() -> (String, Vec<String>, Vec<(String, String)>) {
    let s = crate::settings::current();
    let (program, args) = s.shell.command_line(&login_shell());
    // What runs in it reads the shell it runs in from here.
    let env = [("SHELL".to_string(), program.clone())]
        .into_iter()
        .chain(s.shell.env)
        .collect();
    (program, args, env)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(w: &[&str]) -> Vec<String> {
        w.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_shell_tab_runs_the_login_shell_as_a_login_shell_unless_one_is_named() {
        let own = ShellLaunch::default();
        assert_eq!(
            own.command_line("/bin/zsh"),
            ("/bin/zsh".to_string(), words(&["-l"]))
        );
        let named = ShellLaunch {
            command: words(&["/opt/homebrew/bin/fish", "--login"]),
            ..ShellLaunch::default()
        };
        assert_eq!(
            named.command_line("/bin/zsh"),
            ("/opt/homebrew/bin/fish".to_string(), words(&["--login"]))
        );
        assert_eq!(named.program("/bin/zsh"), "/opt/homebrew/bin/fish");
        assert_eq!(own.program("/bin/zsh"), "/bin/zsh");
    }

    #[test]
    fn a_program_started_directly_runs_its_command_before_adroit_s_arguments() {
        let claude = Launch {
            command: words(&["/usr/local/bin/claude", "--model", "opus"]),
            ..Launch::default()
        };
        assert_eq!(
            claude.command_line("claude", &words(&["--name", "fix"]), "/bin/zsh"),
            (
                "/usr/local/bin/claude".to_string(),
                words(&["--model", "opus", "--name", "fix"])
            )
        );
        // With no command of its own, adroit's program runs.
        assert_eq!(
            Launch::default().command_line("claude", &words(&["--name", "fix"]), "/bin/zsh"),
            ("claude".to_string(), words(&["--name", "fix"]))
        );
    }

    #[test]
    fn a_program_started_through_the_shell_is_handed_over_as_the_shell_s_arguments() {
        let claude = Launch {
            via_shell: true,
            ..Launch::default()
        };
        // An interactive login shell reads every startup file the user sets
        // variables in; the program and its arguments go through untouched.
        assert_eq!(
            claude.command_line("claude", &words(&["--name", "it's"]), "/bin/zsh"),
            (
                "/bin/zsh".to_string(),
                words(&[
                    "-l",
                    "-i",
                    "-c",
                    "exec \"$@\"",
                    "adroit",
                    "claude",
                    "--name",
                    "it's"
                ])
            )
        );
        // fish takes what follows its command as `$argv`, with no `$0`.
        assert_eq!(
            claude.command_line("claude", &words(&["-c"]), "/opt/homebrew/bin/fish"),
            (
                "/opt/homebrew/bin/fish".to_string(),
                words(&["-l", "-i", "-c", "exec $argv", "claude", "-c"])
            )
        );
    }

    #[test]
    fn a_command_is_written_as_a_shell_would_split_it_and_read_back() {
        let line = words(&["/Applications/My Shell.app/bin/sh", "-l", "it's"]);
        let text = format_command(&line);
        assert_eq!(parse_command(&text), Some(line));
        assert_eq!(
            parse_command("  claude   --model opus "),
            Some(words(&["claude", "--model", "opus"]))
        );
        assert_eq!(parse_command(""), Some(vec![]));
        // An open quote says nothing yet.
        assert_eq!(parse_command("claude \"--model"), None);
    }

    #[test]
    fn variables_are_written_one_a_line_and_read_back() {
        let env = parse_env("FOO=bar\n\n  BAZ = a=b \n# a note\nbroken\n");
        assert_eq!(
            env,
            BTreeMap::from([
                ("BAZ".to_string(), "a=b".to_string()),
                ("FOO".to_string(), "bar".to_string()),
            ])
        );
        assert_eq!(format_env(&env), "BAZ=a=b\nFOO=bar");
        assert_eq!(parse_env(&format_env(&env)), env);
    }
}
