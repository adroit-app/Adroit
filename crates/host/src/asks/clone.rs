//! The asks about cloning a repository with ghq: the terminal that clones
//! it, and where it was kept once that terminal ended.
use adroit_model::{CloneTarget, TerminalProgram, TerminalResourceId, TerminalSession};
use adroit_protocol::Answer;
use adroit_term::Spawn;

use super::{ended, failed_spawn, size};
use crate::watch::Watched;
use crate::{daemon, held, sessions};

/// Start a terminal in the home directory that clones the repository `typed`
/// names with ghq, for the user to answer what the clone asks there.
pub(super) async fn clone(
    watched: &Watched,
    typed: &str,
    cols: u16,
    rows: u16,
) -> Result<Answer, String> {
    let target = CloneTarget::parse(typed).ok_or("not a repository to clone")?;
    let (program, args) = adroit_vcs::ghq::get(&target);
    let cwd = crate::paths::home();
    let terminal = TerminalSession {
        resource_id: TerminalResourceId::new(),
        id: 0,
        name: format!("clone · {}", target.path),
        cwd: cwd.clone(),
        program: TerminalProgram::Command,
        file: None,
        command: std::iter::once(program.clone())
            .chain(args.clone())
            .collect(),
        ..TerminalSession::default()
    };
    let (cols, rows) = size(cols, rows);
    let spawn = Spawn {
        tags: sessions::terminal_tags(&terminal)
            .into_iter()
            .chain(crate::environment::tags())
            .collect(),
        program,
        args,
        cwd,
        env: daemon::terminal_env(),
        cols,
        rows,
    };
    let id = match daemon::daemon().spawn_held(spawn).await {
        Ok(id) => id,
        Err(error) => return failed_spawn(error, "cannot start the clone"),
    };
    held::hold(id).await;
    let terminal = TerminalSession { id, ..terminal };
    watched.terminal_opened(terminal.clone());
    Ok(Answer::Terminal { terminal })
}

/// Where ghq keeps the repository at `path`, once the terminal `after` that
/// cloned it has ended.
pub(super) async fn cloned(path: &str, after: u128) -> Result<Answer, String> {
    let target = CloneTarget::parse(path).ok_or("not a repository to clone")?;
    ended(after).await;
    Ok(Answer::Cloned {
        dir: adroit_vcs::ghq::kept(&target).await,
    })
}
