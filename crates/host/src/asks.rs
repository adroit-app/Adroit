//! What the work host answers when a window asks and waits: starting
//! sessions and terminals held until the window has told them the theme,
//! the terminal host's standing and its restart, keeping a pasted image,
//! saving the settings.

use std::sync::Arc;

use adroit_model::time::now_ms;
use adroit_protocol::{Answer, Ask, ConfigFile, FileText, Request, SessionStart, TerminalStart};
use tokio::sync::watch;

use crate::delegation::app as delegation;
use crate::execution::{StartRequest, TerminalStartRequest};
use crate::scratch::{self, Moment};
use crate::serve::Asks;
use crate::terminal_host::{self, Judged};
use crate::watch::Watched;
use crate::work_host::adroit_program;
use crate::{daemon, held, pasted, programs, sessions, settings};

/// The size a program starts at when the window names none.
const COLS: u16 = 120;
const ROWS: u16 = 40;

/// What answers each ask, through `watched` and the terminal host `judged`
/// stands as.
pub(crate) fn asks(watched: Watched, judged: Arc<watch::Sender<Judged>>) -> Asks {
    Arc::new(move |ask| {
        let (watched, judged) = (watched.clone(), judged.clone());
        Box::pin(async move { answer(&watched, &judged, ask).await })
    })
}

async fn answer(watched: &Watched, judged: &watch::Sender<Judged>, ask: Ask) -> Answer {
    let answered = match ask {
        Ask::StartSession(start) => start_session(watched, *start).await,
        Ask::StartTerminal(start) => start_terminal(watched, *start).await,
        Ask::TerminalHost => terminal_host::judged(judged.subscribe())
            .await
            .map(|standing| Answer::TerminalHost { standing }),
        Ask::RestartTerminalHost => {
            let restarted = terminal_host::restart(judged).await;
            watched.relist();
            restarted.map(|standing| Answer::TerminalHost { standing })
        }
        Ask::KeepImage { mime, data } => keep_image(&mime, &data),
        Ask::SaveSettings { text } => save_settings(text),
        Ask::SaveKeys { text } => save_keys(&text),
        Ask::Run { args, cwd, input } => Ok(run(&args, &cwd, input).await),
    };
    answered.unwrap_or_else(|text| Answer::Failed { text })
}

/// The command `adroit <args>` run as in `cwd` with `input` on its standard
/// input: what it wrote, and its exit status.
async fn run(args: &[String], cwd: &std::path::Path, input: Option<String>) -> Answer {
    let (wrote, code) = match args.split_first() {
        Some((command, rest)) if command == "session" => crate::start::run(rest, cwd, input).await,
        Some((command, rest)) if command == "terminal" => crate::terminals::run(rest, cwd).await,
        _ => (Err(format!("the work host runs no command {args:?}")), 2),
    };
    let (stdout, stderr) = match wrote {
        Ok(out) => (out, String::new()),
        Err(error) => (String::new(), error),
    };
    Answer::Ran {
        stdout,
        stderr,
        code,
    }
}

/// Start the session `start` asks for, held, as the delegation store records
/// it: a draft's in a new folder of its own when it is in the scratch folder.
async fn start_session(watched: &Watched, start: SessionStart) -> Result<Answer, String> {
    let settings = settings::current();
    let verb = if start.resume.is_some() {
        "resume"
    } else {
        "start"
    };
    let name = start.name;
    let cwd = if start.draft {
        let root = settings::scratch_folder(&settings, &crate::paths::home());
        scratch::start_dir(&start.cwd, &root, Moment::now())
            .map_err(|e| format!("cannot make a directory for {name}: {e}"))?
    } else {
        start.cwd
    };
    let request = StartRequest {
        agent: start.agent,
        name: name.clone(),
        cwd,
        model: start.model,
        permissions: start.permissions,
        prompt: start.prompt,
        images: start.images,
        resume: start.resume,
        env: daemon::agent_env(),
        started_ms: now_ms(),
        ..Default::default()
    };
    let resumed = verb == "resume";
    let (resolved, recorded) = delegation::prepare(
        &delegation::root(),
        &settings,
        request,
        None,
        &adroit_program(),
    )
    .await
    .map_err(|e| {
        if resumed {
            format!("cannot resume {name}: {e}")
        } else {
            e
        }
    })?;
    let mut spawn = resolved.spawn;
    (spawn.cols, spawn.rows) = size(start.cols, start.rows);
    let terminal_host = daemon::daemon();
    let id = match terminal_host.spawn_held(spawn).await {
        Ok(id) => id,
        Err(e) => {
            recorded.failed().await;
            return Err(format!("cannot {verb} {name}: {e}"));
        }
    };
    if let Err(e) = recorded.attach(id).await {
        let _ = terminal_host.end(id).await;
        return Err(format!("cannot {verb} {name}: {e}"));
    }
    held::hold(id);
    watched.relist();
    Ok(Answer::Session {
        id: sessions::hex(id),
    })
}

/// Start the shell, Neovim or command `start` asks for, held, and list it at
/// once.
async fn start_terminal(watched: &Watched, start: TerminalStart) -> Result<Answer, String> {
    let settings = settings::current();
    let mut resolved = crate::execution::resolve_terminal(
        &settings,
        TerminalStartRequest {
            name: start.name,
            cwd: start.cwd,
            program: start.program,
            // Not `agent_env`: a shell or Neovim keeps the user's own editor.
            env: daemon::terminal_env(),
            file: start.file,
            command: Vec::new(),
        },
        &programs::login_shell(),
    );
    (resolved.spawn.cols, resolved.spawn.rows) = size(start.cols, start.rows);
    let id = daemon::daemon()
        .spawn_held(resolved.spawn)
        .await
        .map_err(|e| e.to_string())?;
    held::hold(id);
    resolved.terminal.id = id;
    watched.apply(Request::TerminalOpened {
        terminal: resolved.terminal.clone(),
    });
    Ok(Answer::Terminal {
        terminal: resolved.terminal,
    })
}

fn size(cols: u16, rows: u16) -> (u16, u16) {
    (
        if cols == 0 { COLS } else { cols },
        if rows == 0 { ROWS } else { rows },
    )
}

/// Keep a pasted image where the prompt's images are kept, dropping those
/// kept too long.
fn keep_image(mime: &str, data: &str) -> Result<Answer, String> {
    let dir = pasted::dir();
    let now = std::time::SystemTime::now();
    pasted::prune(&dir, now);
    pasted::save(&dir, mime, data, now)
        .map(|path| Answer::Kept { path })
        .map_err(|e| e.to_string())
}

/// Make the settings `text` writes the settings, kept for the next run.
fn save_settings(text: String) -> Result<Answer, String> {
    let next = settings::Settings::read(&ConfigFile {
        path: settings::path(),
        text: FileText::Read { text },
    })?;
    settings::set(next)?;
    Ok(Answer::Done)
}

/// What a keys file made without a binding in it starts with.
const KEYS_STARTER: &str = "# Key bindings over the defaults, area by area.\n";

/// Make `text` the key bindings, read again at once.
fn save_keys(text: &str) -> Result<Answer, String> {
    let path = crate::paths::keys_path();
    let failed = |e: std::io::Error| format!("cannot write {}: {e}", path.display());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(failed)?;
    }
    let text = if text.trim().is_empty() && !path.exists() {
        KEYS_STARTER
    } else {
        text
    };
    std::fs::write(&path, text).map_err(failed)?;
    let _ = settings::CHANGED.send(());
    Ok(Answer::Done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_command_is_run_by_its_name_and_one_unknown_or_misspoken_is_refused_as_misused() {
        let cwd = std::path::Path::new("/");
        for args in [
            vec!["daemon".to_string()],
            vec![],
            vec!["terminal".into(), "nonsense".into()],
            vec!["session".into(), "stop".into()],
        ] {
            let Answer::Ran {
                stdout,
                stderr,
                code,
            } = run(&args, cwd, None).await
            else {
                panic!("{args:?} ran nothing");
            };
            assert_eq!((stdout.as_str(), code), ("", 2), "{args:?}");
            assert!(!stderr.is_empty(), "{args:?}");
        }
    }
}
