//! What the `adroit` command was asked to do.

use crate::paths::{DEFAULT_INSTANCE, INSTANCE_ENV, is_instance};

/// What `adroit` does, by its arguments.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Open a window of the instance named, or of the one the environment
    /// names: a new one beside those open when `new`, or else the one open
    /// brought to the front.
    Window {
        instance: Option<String>,
        new: bool,
    },
    Version,
    Connect,
    /// Pair this client with the work machine at `address`, shown to its
    /// operator as `name`.
    Pair {
        address: String,
        name: Option<String>,
    },
    /// List the work machines announcing themselves on the local network.
    Discover,
    /// A subcommand, and the arguments that follow it.
    Terminal(Vec<String>),
    /// Serve a session's `adroit` MCP server on standard input and output.
    Mcp(Vec<String>),
    Editor(Vec<String>),
    Session(Vec<String>),
    /// Start an agent's program in a login of its own, as the last step
    /// of its launch; never typed by hand, so left out of the usage.
    InOwnLogin(Vec<String>),
    Daemon(DaemonCommand),
    Host(HostCommand),
    Remote {
        operation: RemoteCommand,
        instance: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteCommand {
    Host,
    Daemon,
    Info,
    /// Let paired clients reach the hosts over the network.
    Gateway {
        listen: Option<String>,
        /// Whether it announces itself on the local network.
        advertise: bool,
    },
    /// Approve a client's pairing request with the code it shows.
    Pair,
    Devices,
    /// Forget the paired client whose fingerprint starts with this.
    Revoke(String),
    /// Run the hosts and the gateway as the user's services.
    Enable {
        listen: Option<String>,
        advertise: bool,
    },
    Disable,
}

fn remote(args: &[String]) -> Option<Command> {
    let (name, mut rest) = args.split_first()?;
    let mut operation = match name.as_str() {
        "host" => RemoteCommand::Host,
        "daemon" => RemoteCommand::Daemon,
        "info" => RemoteCommand::Info,
        "gateway" => RemoteCommand::Gateway {
            listen: None,
            advertise: true,
        },
        "pair" => RemoteCommand::Pair,
        "devices" => RemoteCommand::Devices,
        "enable" => RemoteCommand::Enable {
            listen: None,
            advertise: true,
        },
        "disable" => RemoteCommand::Disable,
        "revoke" => {
            let (prefix, after) = rest.split_first()?;
            rest = after;
            RemoteCommand::Revoke(prefix.clone())
        }
        _ => return None,
    };
    let mut instance = None;
    while let [option, after @ ..] = rest {
        rest = after;
        match (option.as_str(), &mut operation) {
            (
                "--no-advertise",
                RemoteCommand::Gateway { advertise, .. } | RemoteCommand::Enable { advertise, .. },
            ) if *advertise => *advertise = false,
            ("--instance", _) if instance.is_none() => {
                let (value, after) = rest.split_first().filter(|(v, _)| is_instance(v))?;
                instance = Some(value.clone());
                rest = after;
            }
            (
                "--listen",
                RemoteCommand::Gateway { listen, .. } | RemoteCommand::Enable { listen, .. },
            ) if listen.is_none() => {
                let (value, after) = rest.split_first()?;
                value.parse::<std::net::SocketAddr>().ok()?;
                *listen = Some(value.clone());
                rest = after;
            }
            _ => return None,
        }
    }
    Some(Command::Remote {
        operation,
        instance,
    })
}

/// What `adroit daemon` is asked to do with the daemon of the instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonCommand {
    Serve,
    Stop,
    Restart,
}

/// A window opened as `args`, its options in any order, each once.
fn window(args: &[String]) -> Option<Command> {
    let (mut instance, mut new) = (None, false);
    let mut words = args.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--instance" if instance.is_none() => {
                let name = words.next().filter(|n| is_instance(n))?;
                instance = Some(name.clone());
            }
            "--new-window" if !new => new = true,
            _ => return None,
        }
    }
    Some(Command::Window { instance, new })
}

/// What `adroit host` is asked to do with the work host of the instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostCommand {
    Serve,
    Stop,
    Restart,
}

/// The variables through which a command run inside a session reaches the
/// hosts of its instance's environment.
pub const ROUTING: [&str; 4] = [
    "ADROIT_HOST",
    "ADROIT_DAEMON",
    "ADROIT_ENVIRONMENT",
    "ADROIT_WORK_INSTANCE",
];

impl Command {
    /// Whether the command reaches the hosts of its instance's environment
    /// through [`ROUTING`], as one run inside a session does. A window
    /// reaches each environment it shows by a route of its own, and the
    /// hosts are given theirs by whoever starts them.
    pub fn routed(&self) -> bool {
        matches!(
            self,
            Command::Session(_)
                | Command::Terminal(_)
                | Command::Mcp(_)
                | Command::Editor(_)
                | Command::Connect
        )
    }
}

pub const USAGE: &str = "usage: adroit [[--instance <name>] [--new-window] | --version | connect [--check | discover | pair <address> [--name <name>]] | terminal … | mcp … | editor … | session … | daemon [stop | restart] | host [stop | restart] | remote <daemon|host|info|gateway [--listen <address>] [--no-advertise]|pair|devices|revoke <fingerprint>|enable [--listen <address>] [--no-advertise]|disable> [--instance <name>]]";

/// The command `args` ask for; `None` for one adroit does not know, which
/// must not open a window: a program that ran an older adroit's subcommand
/// would open one each time.
pub fn command(args: &[String]) -> Option<Command> {
    let rest = || args[1..].to_vec();
    match args.first().map(String::as_str) {
        None => Some(Command::Window {
            instance: None,
            new: false,
        }),
        // What macOS once added when it launched an app from the Finder.
        Some(a) if a.starts_with("-psn_") && args.len() == 1 => Some(Command::Window {
            instance: None,
            new: false,
        }),
        Some("--instance" | "--new-window") => window(args),
        Some("--version") if args.len() == 1 => Some(Command::Version),
        Some("connect") if args.len() == 1 || args == ["connect", "--check"] => {
            Some(Command::Connect)
        }
        Some("connect") => match &args[1..] {
            [verb] if verb == "discover" => Some(Command::Discover),
            [verb, address] if verb == "pair" => Some(Command::Pair {
                address: address.clone(),
                name: None,
            }),
            [verb, address, option, name] if verb == "pair" && option == "--name" => {
                Some(Command::Pair {
                    address: address.clone(),
                    name: Some(name.clone()),
                })
            }
            _ => None,
        },
        Some("mcp") => Some(Command::Mcp(rest())),
        Some("terminal") => Some(Command::Terminal(rest())),
        Some("editor") => Some(Command::Editor(rest())),
        Some("session") => Some(Command::Session(rest())),
        Some("remote") => remote(&args[1..]),
        Some(adroit_agents::own_login::SUBCOMMAND) => Some(Command::InOwnLogin(rest())),
        Some("daemon") => match &args[1..] {
            [] => Some(Command::Daemon(DaemonCommand::Serve)),
            [verb] if verb == "stop" => Some(Command::Daemon(DaemonCommand::Stop)),
            [verb] if verb == "restart" => Some(Command::Daemon(DaemonCommand::Restart)),
            _ => None,
        },
        Some("host") => match &args[1..] {
            [] => Some(Command::Host(HostCommand::Serve)),
            [verb] if verb == "stop" => Some(Command::Host(HostCommand::Stop)),
            [verb] if verb == "restart" => Some(Command::Host(HostCommand::Restart)),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(words: &[&str]) -> Option<Command> {
        let args: Vec<String> = words.iter().map(|w| w.to_string()).collect();
        command(&args)
    }

    fn window(instance: Option<&str>, new: bool) -> Option<Command> {
        Some(Command::Window {
            instance: instance.map(String::from),
            new,
        })
    }

    #[test]
    fn no_arguments_open_the_window() {
        assert_eq!(asked(&[]), window(None, false));
    }

    #[test]
    fn the_process_serial_number_macos_once_passed_opens_the_window() {
        assert_eq!(asked(&["-psn_0_12345"]), window(None, false));
    }

    #[test]
    fn a_new_window_is_asked_for_of_the_instance_named_or_the_one_in_use() {
        assert_eq!(asked(&["--new-window"]), window(None, true));
        assert_eq!(
            asked(&["--instance", "work", "--new-window"]),
            window(Some("work"), true)
        );
        assert_eq!(
            asked(&["--new-window", "--instance", "work"]),
            window(Some("work"), true)
        );
        assert_eq!(asked(&["--new-window", "--new-window"]), None);
        assert_eq!(asked(&["--new-window", "extra"]), None);
    }

    #[test]
    fn a_command_run_inside_a_session_is_routed_by_the_environment_and_a_window_is_not() {
        for words in [
            &["session", "list"][..],
            &["terminal", "screen", "dev"],
            &["mcp"],
            &["editor", "/tmp/p.md"],
            &["connect"],
        ] {
            assert!(asked(words).unwrap().routed(), "{words:?}");
        }
        for words in [
            &[][..],
            &["--new-window"],
            &["host"],
            &["daemon"],
            &["--version"],
        ] {
            assert!(!asked(words).unwrap().routed(), "{words:?}");
        }
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
        assert_eq!(asked(&["host"]), Some(Command::Host(HostCommand::Serve)));
        assert_eq!(
            asked(&["host", "stop"]),
            Some(Command::Host(HostCommand::Stop))
        );
        assert_eq!(
            asked(&["host", "restart"]),
            Some(Command::Host(HostCommand::Restart))
        );
        assert_eq!(asked(&["--version"]), Some(Command::Version));
        assert_eq!(
            asked(&["in-own-login", "claude", "/w", "claude"]),
            Some(Command::InOwnLogin(vec![
                "claude".into(),
                "/w".into(),
                "claude".into()
            ]))
        );
    }

    #[test]
    fn the_window_of_a_named_instance_is_opened_by_its_name() {
        assert_eq!(asked(&["--instance", "work"]), window(Some("work"), false));
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

    #[test]
    fn remote_services_and_discovery_are_selected_without_opening_a_window() {
        for (name, operation) in [
            ("host", RemoteCommand::Host),
            ("daemon", RemoteCommand::Daemon),
            ("info", RemoteCommand::Info),
        ] {
            assert_eq!(
                asked(&["remote", name]),
                Some(Command::Remote {
                    operation: operation.clone(),
                    instance: None
                })
            );
            assert_eq!(
                asked(&["remote", name, "--instance", "ai"]),
                Some(Command::Remote {
                    operation,
                    instance: Some("ai".into())
                })
            );
        }
        assert_eq!(asked(&["remote", "info", "--instance", "../other"]), None);
        assert_eq!(asked(&["remote", "stop"]), None);
    }

    #[test]
    fn the_gateway_and_its_clients_are_managed_from_remote_subcommands() {
        let remote = |operation| {
            Some(Command::Remote {
                operation,
                instance: None,
            })
        };
        assert_eq!(
            asked(&["remote", "gateway"]),
            remote(RemoteCommand::Gateway {
                listen: None,
                advertise: true
            })
        );
        assert_eq!(
            asked(&["remote", "gateway", "--listen", "192.168.1.5:47130"]),
            remote(RemoteCommand::Gateway {
                listen: Some("192.168.1.5:47130".into()),
                advertise: true,
            })
        );
        assert_eq!(
            asked(&[
                "remote",
                "gateway",
                "--instance",
                "ai",
                "--listen",
                "[::]:1"
            ]),
            Some(Command::Remote {
                operation: RemoteCommand::Gateway {
                    listen: Some("[::]:1".into()),
                    advertise: true,
                },
                instance: Some("ai".into())
            })
        );
        assert_eq!(asked(&["remote", "pair"]), remote(RemoteCommand::Pair));
        assert_eq!(
            asked(&["remote", "devices"]),
            remote(RemoteCommand::Devices)
        );
        assert_eq!(
            asked(&["remote", "revoke", "0123abcd"]),
            remote(RemoteCommand::Revoke("0123abcd".into()))
        );
        assert_eq!(asked(&["remote", "revoke"]), None);
        assert_eq!(
            asked(&["remote", "pair", "123456"]),
            None,
            "no code in argv"
        );
        assert_eq!(asked(&["remote", "gateway", "--listen", "nowhere"]), None);
        assert_eq!(
            asked(&["remote", "enable"]),
            remote(RemoteCommand::Enable {
                listen: None,
                advertise: true
            })
        );
        assert_eq!(
            asked(&["remote", "enable", "--listen", "10.0.0.2:47130"]),
            remote(RemoteCommand::Enable {
                listen: Some("10.0.0.2:47130".into()),
                advertise: true,
            })
        );
        assert_eq!(
            asked(&["remote", "disable"]),
            remote(RemoteCommand::Disable)
        );
        assert_eq!(
            asked(&["remote", "disable", "--listen", "10.0.0.2:1"]),
            None
        );
        assert_eq!(
            asked(&[
                "remote",
                "gateway",
                "--no-advertise",
                "--listen",
                "10.0.0.2:1"
            ]),
            remote(RemoteCommand::Gateway {
                listen: Some("10.0.0.2:1".into()),
                advertise: false
            })
        );
        assert_eq!(
            asked(&["remote", "enable", "--no-advertise"]),
            remote(RemoteCommand::Enable {
                listen: None,
                advertise: false
            })
        );
        assert_eq!(asked(&["remote", "pair", "--no-advertise"]), None);
        assert_eq!(
            asked(&["remote", "gateway", "--no-advertise", "--no-advertise"]),
            None
        );
    }

    #[test]
    fn a_client_pairs_with_a_work_machine_by_its_address() {
        assert_eq!(
            asked(&["connect", "pair", "work-box.local:47130"]),
            Some(Command::Pair {
                address: "work-box.local:47130".into(),
                name: None
            })
        );
        assert_eq!(
            asked(&["connect", "pair", "10.0.0.2:47130", "--name", "desk"]),
            Some(Command::Pair {
                address: "10.0.0.2:47130".into(),
                name: Some("desk".into())
            })
        );
        assert_eq!(asked(&["connect", "pair"]), None);
        assert_eq!(asked(&["connect", "pair", "a:1", "--name"]), None);
        assert_eq!(asked(&["connect"]), Some(Command::Connect));
        assert_eq!(asked(&["connect", "discover"]), Some(Command::Discover));
        assert_eq!(asked(&["connect", "discover", "x"]), None);
    }
}
