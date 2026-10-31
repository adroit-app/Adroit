fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "__startup_lease") {
        std::process::exit(adroit_host::startup_lease(&args[1..]));
    }
    use adroit_host::cli::{Command, RemoteCommand, USAGE, command, settle_instance};
    // Run from inside an agent's account, such as by its MCP server or a
    // command its session runs, adroit finds its instance where it is
    // outside the account, not where the account moved the agent's folders.
    for (name, value) in adroit_agents::own_login::outside(std::env::vars_os()) {
        // SAFETY: no other thread exists yet.
        unsafe {
            match value {
                Some(value) => std::env::set_var(&name, value),
                None => std::env::remove_var(&name),
            }
        }
    }
    let asked = command(&args);
    // Ahead of settling the instance: the agent's program is handed the
    // environment exactly as its login leaves it.
    if let Some(Command::InOwnLogin(rest)) = &asked {
        std::process::exit(adroit_agents::own_login::run(rest));
    }
    let given = match &asked {
        Some(Command::Window { instance, .. } | Command::Remote { instance, .. }) => {
            instance.clone()
        }
        Some(Command::Host(_) | Command::Daemon(_)) => std::env::var("ADROIT_WORK_INSTANCE").ok(),
        _ => None,
    };
    let new = matches!(asked, Some(Command::Window { new: true, .. }));
    let work_route = std::env::var_os("ADROIT_WORK_INSTANCE").is_some();
    // Every process started from here belongs to the instance it settles on.
    if let Err(e) = settle_instance(given) {
        eprintln!("adroit: {e}");
        std::process::exit(2);
    }
    if (cfg!(feature = "desktop") && matches!(asked, Some(Command::Window { .. })))
        || (!work_route
            && std::env::var_os("ADROIT_ENVIRONMENT").is_none()
            && matches!(
                asked,
                Some(
                    Command::Session(_)
                        | Command::Terminal(_)
                        | Command::Mcp(_)
                        | Command::Editor(_)
                        | Command::Connect
                )
            ))
    {
        let state = adroit_host::paths::instance_state();
        let binding = if matches!(asked, Some(Command::Window { .. })) {
            adroit_connect::environments::settle(&adroit_host::paths::instance(), &state).map(Some)
        } else {
            adroit_connect::environments::Binding::load(&state)
        };
        let binding = match binding {
            Ok(binding) => binding,
            Err(error) => {
                eprintln!("adroit: {error}");
                std::process::exit(2);
            }
        };
        if let Some(binding) = binding {
            // SAFETY: environment routing is settled before any runtime creates threads.
            unsafe {
                if binding.target == adroit_connect::Target::Local {
                    let work = binding.work_instance.as_deref().unwrap_or("default");
                    if binding.scope.is_some() || work != adroit_host::paths::instance() {
                        let service = adroit_host::paths::instance_dir(
                            &adroit_host::paths::state_dir(),
                            work,
                        );
                        std::env::set_var("ADROIT_WORK_INSTANCE", work);
                        std::env::set_var("ADROIT_HOST", service.join("host/host.sock"));
                        std::env::set_var("ADROIT_DAEMON", service.join("felis/daemon.sock"));
                    }
                }
                match binding.scope {
                    Some(scope) => {
                        std::env::set_var("ADROIT_ENVIRONMENT", scope);
                    }
                    None => {
                        std::env::remove_var("ADROIT_ENVIRONMENT");
                    }
                }
            }
        }
    }
    if !adroit_host::remote::active()
        && std::env::var_os("ADROIT_ENVIRONMENT").is_none()
        && !work_route
        && matches!(
            &asked,
            Some(
                Command::Session(_)
                    | Command::Terminal(_)
                    | Command::Mcp(_)
                    | Command::Editor(_)
                    | Command::Daemon(_)
                    | Command::Host(_)
            )
        )
    {
        let path = adroit_host::paths::instance_state().join("execution.toml");
        let target =
            adroit_connect::environments::Binding::load(&adroit_host::paths::instance_state())
                .and_then(|binding| {
                    binding.map_or_else(
                        || adroit_connect::Target::load(&path),
                        |binding| Ok(binding.target),
                    )
                });
        let refused = match target {
            Ok(adroit_connect::Target::Local) => None,
            Ok(adroit_connect::Target::Native { .. }) => Some("Remote execution is selected; run this command on the work machine, or use adroit connect to check its connection".to_string()),
            Err(text) => Some(text),
        };
        if let Some(text) = refused {
            eprintln!("adroit: {text}");
            std::process::exit(2);
        }
    }
    if matches!(
        asked,
        Some(
            Command::Daemon(_)
                | Command::Host(_)
                | Command::Remote {
                    operation: RemoteCommand::Host | RemoteCommand::Daemon,
                    ..
                }
        )
    ) {
        // SAFETY: the host runtime has not started any threads.
        unsafe {
            std::env::remove_var("ADROIT_ENVIRONMENT");
        }
        let names = std::env::vars_os().map(|(key, _)| key.to_string_lossy().into_owned());
        for key in adroit_host::env::inherited(names) {
            // SAFETY: the host runtime has not created any threads yet.
            unsafe { std::env::remove_var(key) };
        }
    }
    if !matches!(asked, Some(Command::Remote { .. } | Command::Version))
        && let Err(e) = adroit_host::remote::refresh_client()
    {
        eprintln!("adroit: cannot discover remote hosts: {e}");
        std::process::exit(1);
    }
    match asked {
        None => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
        Some(Command::Version) => {
            println!("adroit {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Some(Command::Connect) => {
            let path = adroit_host::paths::instance_state().join("execution.toml");
            let result = adroit_connect::diagnose(&path);
            match result {
                Ok(text) => println!("{text}"),
                Err(text) => {
                    eprintln!("adroit: {text}");
                    std::process::exit(1);
                }
            }
            return;
        }
        Some(Command::Discover) => {
            match adroit_connect::discover_cli() {
                Ok(lines) if lines.is_empty() => {
                    println!("No work machine announced itself on this network")
                }
                Ok(lines) => lines.iter().for_each(|line| println!("{line}")),
                Err(text) => {
                    eprintln!("adroit: {text}");
                    std::process::exit(1);
                }
            }
            return;
        }
        Some(Command::Pair { address, name }) => {
            match adroit_connect::pair_cli(&address, name.as_deref(), |text| println!("{text}")) {
                Ok(text) => println!("{text}"),
                Err(text) => {
                    eprintln!("adroit: {text}");
                    std::process::exit(1);
                }
            }
            return;
        }
        Some(Command::Session(rest)) => std::process::exit(adroit_host::session_cli(&rest)),
        Some(Command::Mcp(rest)) => std::process::exit(adroit_host::delegation::bridge::cli(&rest)),
        Some(Command::Terminal(rest)) => std::process::exit(adroit_host::terminal_cli(&rest)),
        Some(Command::Editor(rest)) => std::process::exit(editor(&rest)),
        Some(Command::Daemon(asked)) => std::process::exit(adroit_host::daemon_cli(asked)),
        Some(Command::Host(asked)) => std::process::exit(adroit_host::host_cli(asked)),
        Some(Command::Remote { operation, .. }) => {
            std::process::exit(adroit_host::remote::cli(operation))
        }
        Some(Command::Window { .. }) => {}
        Some(Command::InOwnLogin(_)) => unreachable!("run before the instance is settled"),
    }
    window(new);
}

#[cfg(feature = "desktop")]
fn editor(args: &[String]) -> i32 {
    adroit_window::editor::cli(args)
}

#[cfg(not(feature = "desktop"))]
fn editor(_: &[String]) -> i32 {
    eprintln!("adroit: this build has no window to take editor input");
    2
}

#[cfg(not(feature = "desktop"))]
fn window(_: bool) {
    eprintln!("adroit: this build has no window; use `adroit host` to serve the work");
    std::process::exit(2);
}

#[cfg(feature = "desktop")]
fn window(new: bool) {
    match adroit_window::open_window(new) {
        // An open window came to the front.
        Ok(false) => return,
        Ok(true) => {}
        Err(e) => {
            eprintln!("adroit: cannot open a window: {e}");
            std::process::exit(1);
        }
    }
    // Opened from the Finder, there is no terminal and no PATH of the user's
    // own; the login shell knows where their tools are.
    if std::env::var_os("TERM").is_none()
        && let Some(path) = adroit_host::env::login_path(
            &adroit_host::env::login_shell().unwrap_or_else(|| "/bin/zsh".into()),
        )
    {
        // SAFETY: no other thread exists yet.
        unsafe { std::env::set_var("PATH", path) };
    }
    let names = std::env::vars_os().map(|(k, _)| k.to_string_lossy().into_owned());
    for k in adroit_host::env::inherited(names) {
        // SAFETY: no other thread exists yet.
        unsafe { std::env::remove_var(k) };
    }
    adroit_window::launch(adroit_window::Build {
        build: adroit_host::standing::build(),
        described: adroit_host::standing::DESCRIBED,
    });
}
