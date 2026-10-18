fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") => {
            println!("adroit {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Some("shell") => std::process::exit(adroit::shell_cli(&args[1..])),
        Some("editor") => std::process::exit(adroit::editor::cli(&args[1..])),
        Some("daemon") => std::process::exit(adroit::daemon_cli()),
        _ => {}
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
