fn main() {
    use adroit_host::cli::{Command, USAGE, command, settle_instance};
    let args: Vec<String> = std::env::args().skip(1).collect();
    let asked = command(&args);
    let given = match &asked {
        Some(Command::Window { instance, .. }) => instance.clone(),
        _ => None,
    };
    let new = matches!(asked, Some(Command::Window { new: true, .. }));
    // Every process started from here belongs to the instance it settles on.
    if let Err(e) = settle_instance(given) {
        eprintln!("adroit: {e}");
        std::process::exit(2);
    }
    if matches!(asked, Some(Command::Daemon(_) | Command::Host(_))) {
        let names = std::env::vars_os().map(|(key, _)| key.to_string_lossy().into_owned());
        for key in adroit_host::env::inherited(names) {
            // SAFETY: the host runtime has not created any threads yet.
            unsafe { std::env::remove_var(key) };
        }
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
        Some(Command::Session(rest)) => std::process::exit(adroit_host::session_cli(&rest)),
        Some(Command::Mcp(rest)) => std::process::exit(adroit_host::delegation::bridge::cli(&rest)),
        Some(Command::Terminal(rest)) => std::process::exit(adroit_host::terminal_cli(&rest)),
        Some(Command::Editor(rest)) => std::process::exit(adroit_window::editor::cli(&rest)),
        Some(Command::Daemon(asked)) => std::process::exit(adroit_host::daemon_cli(asked)),
        Some(Command::Host(asked)) => std::process::exit(adroit_host::host_cli(asked)),
        Some(Command::Window { .. }) => {}
    }
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
