fn main() {
    use adroit::cli::{Command, USAGE, command};
    let args: Vec<String> = std::env::args().skip(1).collect();
    match command(&args) {
        None => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
        Some(Command::Version) => {
            println!("adroit {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Some(Command::Session(rest)) => std::process::exit(adroit::session_cli(&rest)),
        Some(Command::Shell(rest)) => std::process::exit(adroit::shell_cli(&rest)),
        Some(Command::Editor(rest)) => std::process::exit(adroit::editor::cli(&rest)),
        Some(Command::Daemon) => std::process::exit(adroit::daemon_cli()),
        Some(Command::Window) => {}
    }
    // Opened from the Finder, there is no terminal and no PATH of the user's
    // own; the login shell knows where their tools are.
    if std::env::var_os("TERM").is_none()
        && let Some(path) = adroit::env::login_path(
            &adroit::env::login_shell().unwrap_or_else(|| "/bin/zsh".into()),
        )
    {
        // SAFETY: no other thread exists yet.
        unsafe { std::env::set_var("PATH", path) };
    }
    let names = std::env::vars_os().map(|(k, _)| k.to_string_lossy().into_owned());
    for k in adroit::env::inherited(names) {
        // SAFETY: no other thread exists yet.
        unsafe { std::env::remove_var(k) };
    }
    adroit::launch();
}
