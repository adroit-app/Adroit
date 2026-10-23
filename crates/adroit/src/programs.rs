//! How the programs adroit runs outside an agent's adapter are started: the
//! account's login shell, and Claude Code asked something outside a console.
//! What the settings say of each is `adroit_model::config`.

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
