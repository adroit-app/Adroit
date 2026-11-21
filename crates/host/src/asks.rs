//! What the work host answers when a window asks and waits: starting
//! sessions and terminals held until the window has told them the theme,
//! the terminal host's standing and its restart, keeping a pasted image,
//! saving the settings.

use std::path::PathBuf;
use std::sync::Arc;

use adroit_model::TerminalProgram;
use adroit_model::time::now_ms;
use adroit_protocol::{Answer, Ask, ConfigFile, FileText, HostPath, SessionStart, TerminalStart};
use tokio::sync::watch;

use crate::delegation::app as delegation;
use crate::execution::{COLS, ROWS, StartRequest, TerminalStartRequest, spawn_may_have_happened};
use crate::scratch;
use crate::serve::Asks;
use crate::terminal_host::{self, Judged};
use crate::watch::Watched;
use crate::work_host::adroit_program;
use crate::{daemon, held, pasted, programs, sessions, settings};

mod clone;
mod logins;

use clone::{clone, cloned};
use logins::{delete_login, login_standing, open_login};

/// What answers each ask, through `watched` and the terminal host `judged`
/// stands as.
pub(crate) fn asks(watched: Watched, judged: Arc<watch::Sender<Judged>>) -> Asks {
    Arc::new(move |ask| {
        let (watched, judged) = (watched.clone(), judged.clone());
        Box::pin(async move { answer(&watched, &judged, ask).await })
    })
}

async fn answer(watched: &Watched, judged: &watch::Sender<Judged>, ask: Ask) -> Answer {
    if matches!(
        &ask,
        Ask::StartSession(_)
            | Ask::StartTerminal(_)
            | Ask::OpenLogin { .. }
            | Ask::Clone { .. }
            | Ask::BringBack { .. }
            | Ask::Run { .. }
    ) {
        let prepared = terminal_host::judged(judged.subscribe()).await;
        let starts = match &ask {
            Ask::Run { args, .. } => args.get(1).is_some_and(|op| op == "start"),
            _ => true,
        };
        // Existing terminals can be used even when preparing terminfo failed.
        if starts && let Err(text) = prepared {
            return Answer::Failed { text };
        }
    }
    let answered = match ask {
        Ask::PathEntries { root, typed } => crate::completion::read(root, typed)
            .await
            .map(|entries| Answer::PathEntries { entries }),
        Ask::Apply { .. } | Ask::MutationStatus { .. } | Ask::MutationEpoch => {
            Err("mutation admission belongs to the work connection".into())
        }
        Ask::SubmitPrompt {
            session,
            agent,
            text,
            images,
        } => submit_prompt(&session, agent, &text, &images).await,
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
        Ask::KeepImageFile { path } => blocking(move || keep_image_file(&path)).await,
        Ask::MakeEntry { path, dir } => {
            let made = blocking(move || crate::making::make(&path, dir)).await;
            // The listing shows what was made without waiting on the disk's watch.
            watched.read_again();
            made.map(|()| Answer::Done)
        }
        Ask::PasteEntries {
            sources,
            dir,
            moving,
        } => {
            let pasted = blocking(move || crate::shifting::paste(&sources, &dir, moving)).await;
            // What was pasted before one failed is listed all the same.
            watched.read_again();
            pasted.map(|()| Answer::Done)
        }
        Ask::TrashEntries { paths } => {
            let trashed = blocking(move || crate::shifting::trash(&paths)).await;
            watched.read_again();
            trashed.map(|()| Answer::Done)
        }
        Ask::PullWorktree {
            repository,
            number,
            title,
        } => adroit_vcs::github::pull_worktree(&repository, number, &title)
            .await
            .map(|path| Answer::Worktree { path }),
        Ask::GitHubThread {
            repository,
            listing,
            number,
        } => adroit_vcs::github::thread(&repository, listing, number)
            .await
            .map(|thread| Answer::Thread {
                thread: Box::new(thread),
            }),
        Ask::SaveSettings { text } => save_settings(text),
        Ask::SaveSettingsChecked { text, expected } => save_settings_checked(text, expected),
        Ask::SaveKeys { text } => save_keys(&text),
        Ask::Drafts => {
            let dir = crate::paths::instance_state();
            blocking(move || crate::drafts::load(&dir))
                .await
                .map(|drafts| Answer::Drafts(Box::new(drafts)))
        }
        Ask::KeepDrafts(changes) => {
            let dir = crate::paths::instance_state();
            blocking(move || crate::drafts::change(&dir, *changes))
                .await
                .map(|()| Answer::Done)
        }
        Ask::ConsumeDraft { key, text, images } => {
            let dir = crate::paths::instance_state();
            blocking(move || crate::drafts::consume(&dir, &key, &text, &images))
                .await
                .map(|cleared| Answer::DraftConsumed { cleared })
        }
        Ask::Picture { path, whole } => Ok(picture(path, whole).await),
        Ask::Run { args, cwd, input } => Ok(run(&args, &cwd, input).await),
        Ask::LoginStanding { profile, after } => {
            if let Some(terminal) = after {
                ended(terminal).await;
            }
            login_standing(&profile).await
        }
        Ask::OpenLogin {
            profile,
            cols,
            rows,
        } => open_login(watched, &profile, cols, rows).await,
        Ask::DeleteLogin { profile } => delete_login(&profile).await,
        Ask::Clone { typed, cols, rows } => clone(watched, &typed, cols, rows).await,
        Ask::Cloned { path, after } => cloned(&path, after).await,
        Ask::BringBack { ids } => {
            let outcomes = crate::restore::bring_back(ids).await;
            watched.relist();
            outcomes.map(|outcomes| Answer::BroughtBack { outcomes })
        }
        Ask::LetGo { ids } => {
            let gone = crate::restore::let_go(ids).await;
            watched.relist();
            gone.map(|()| Answer::Done)
        }
    };
    answered.unwrap_or_else(|text| Answer::Failed { text })
}

/// What `work` gives, run on a blocking thread; a thread that failed to give
/// it fails with its text.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    crate::environment::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())
        .and_then(|done| done)
}

async fn submit_prompt(
    session: &str,
    agent: adroit_model::Agent,
    text: &str,
    images: &[PathBuf],
) -> Result<Answer, String> {
    let id = sessions::id_of(session).ok_or("invalid session ID")?;
    let listed = daemon::daemon()
        .sessions()
        .await
        .map_err(|e| e.to_string())?;
    let actual = sessions::agents(&listed)
        .find(|s| s.id == session)
        .ok_or("the prompt's session no longer exists")?;
    if actual.agent != agent {
        return Err("the prompt's agent does not match the session".into());
    }
    let composed = crate::compose::compose(0, agent, text, images);
    if composed.image_missing {
        return Err("a prompt attachment is missing on the work machine".into());
    }
    match type_prompt(id, &composed.typing).await {
        Ok(()) => Ok(Answer::Done),
        Err(text) => Ok(Answer::Unconfirmed { text }),
    }
}

fn failed_spawn(error: adroit_term::Error, context: &str) -> Result<Answer, String> {
    let text = format!("{context}: {error}");
    if crate::remote::active() && spawn_may_have_happened(&error) {
        Ok(Answer::Unconfirmed { text })
    } else {
        Err(text)
    }
}

pub(crate) async fn type_prompt(id: u128, steps: &[adroit_protocol::Typing]) -> Result<(), String> {
    use adroit_protocol::Typing;
    use adroit_term::Input;
    let daemon = daemon::daemon();
    let input: Vec<_> = steps
        .iter()
        .map(|step| match step {
            Typing::Text { text } => Input::Text(text.clone()),
            Typing::Paste { text } => Input::Paste(text.clone()),
            Typing::Key { name } => Input::Key(adroit_model::Key::new(name)),
            Typing::Pause { ms } => Input::Pause(std::time::Duration::from_millis((*ms).min(5000))),
        })
        .collect();
    daemon.input(id, &input).await.map_err(|e| e.to_string())
}

/// The command `adroit <args>` run as in `cwd` with `input` on its standard
/// input: what it wrote, and its exit status.
async fn run(args: &[String], cwd: &std::path::Path, input: Option<String>) -> Answer {
    let output = match args.split_first() {
        Some((command, rest)) if command == "session" => crate::start::run(rest, cwd, input).await,
        Some((command, rest)) if command == "terminal" => crate::terminals::run(rest, cwd).await,
        _ => crate::command::Output {
            wrote: Err(format!("the work host runs no command {args:?}")),
            code: 2,
            effects_possible: false,
        },
    };
    output.answer(crate::remote::active())
}

/// The path of `place`, which has to be on this host's own machine: no
/// other host can be reached yet.
fn here(place: HostPath) -> Result<PathBuf, String> {
    match place.on_local() {
        Some(path) => Ok(path.to_path_buf()),
        None => Err(format!(
            "{} is on {}, which cannot be reached",
            place.path.display(),
            place.host.0
        )),
    }
}

/// Start the session `start` asks for, held, as the delegation store records
/// it: a draft's in the scratch folder, made if need be, when it is there.
async fn start_session(watched: &Watched, start: SessionStart) -> Result<Answer, String> {
    let settings = settings::current();
    let verb = if start.resume.is_some() {
        "resume"
    } else {
        "start"
    };
    let name = start.name;
    let at = here(start.cwd).map_err(|e| format!("cannot {verb} {name}: {e}"))?;
    let cwd = if start.draft {
        let root = settings::scratch_folder(&settings, &crate::paths::home());
        scratch::start_dir(&at, &root)
            .map_err(|e| format!("cannot make the scratch folder for {name}: {e}"))?
    } else {
        at
    };
    let request = StartRequest {
        agent: start.agent,
        profile: start.profile,
        name: name.clone(),
        name_given: start.name_given,
        cwd,
        model: start.model,
        permissions: start.permissions,
        prompt: start.prompt,
        images: start.images,
        resume: start.resume,
        home: start.home,
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
            if crate::remote::active() && spawn_may_have_happened(&e) {
                return failed_spawn(e, &format!("cannot {verb} {name}"));
            }
            recorded.failed().await;
            return Err(format!("cannot {verb} {name}: {e}"));
        }
    };
    if let Err(e) = recorded.attach(id).await {
        let _ = terminal_host.end(id).await;
        if crate::remote::active() {
            return Ok(Answer::Unconfirmed {
                text: format!("cannot confirm starting {name}: {e}"),
            });
        }
        return Err(format!("cannot {verb} {name}: {e}"));
    }
    held::hold(id).await;
    watched.relist();
    Ok(Answer::Session {
        id: sessions::hex(id),
    })
}

/// Start the shell, Neovim or command `start` asks for, held, and list it at
/// once.
async fn start_terminal(watched: &Watched, start: TerminalStart) -> Result<Answer, String> {
    let cwd = here(start.cwd).map_err(|e| format!("cannot open {}: {e}", start.name))?;
    let settings = settings::current();
    let mut resolved = crate::execution::resolve_terminal(
        &settings,
        TerminalStartRequest {
            name: start.name,
            cwd,
            program: start.program,
            // Not `agent_env`: a shell or Neovim keeps the user's own editor.
            env: daemon::terminal_env(),
            file: start.file,
            command: Vec::new(),
            opener: None,
            show: false,
            restored_from: None,
        },
        &programs::login_shell(),
    );
    (resolved.spawn.cols, resolved.spawn.rows) = size(start.cols, start.rows);
    let id = match daemon::daemon().spawn_held(resolved.spawn).await {
        Ok(id) => id,
        Err(error) => return failed_spawn(error, "cannot start the terminal"),
    };
    held::hold(id).await;
    resolved.terminal.id = id;
    // A command is not run again unasked, so only a shell or a Neovim is
    // kept to be opened again after the terminal host goes.
    if resolved.terminal.program != TerminalProgram::Command {
        let terminal = resolved.terminal.clone();
        let generation = daemon::daemon().generation();
        let kept = delegation::with_store(&delegation::root(), move |store| {
            store.open_terminal(&terminal, generation, now_ms())
        })
        .await;
        if let Err(e) = kept {
            tracing::warn!(
                terminal = %resolved.terminal.name,
                error = %e,
                "cannot keep a terminal to open again"
            );
        }
    }
    watched.terminal_opened(resolved.terminal.clone());
    Ok(Answer::Terminal {
        terminal: resolved.terminal,
    })
}

/// Once the terminal `id` has ended: its program exited, or it is gone; or
/// once a login left open has been waited for long enough. The terminal
/// host tells no one when a program exits, so it is looked at until then.
async fn ended(id: u128) {
    const LONGEST: std::time::Duration = std::time::Duration::from_secs(30 * 60);
    let until = tokio::time::Instant::now() + LONGEST;
    while tokio::time::Instant::now() < until {
        let gone = match daemon::daemon().sessions().await {
            Ok(sessions) => !sessions.iter().any(|s| s.id == id && !s.exited),
            Err(_) => true,
        };
        if gone {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
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

/// Keep a copy of a dropped image file where the prompt's images are kept,
/// dropping those kept too long.
fn keep_image_file(from: &std::path::Path) -> Result<Answer, String> {
    let dir = pasted::dir();
    let now = std::time::SystemTime::now();
    pasted::prune(&dir, now);
    pasted::copy(&dir, from, now)
        .map(|path| Answer::Kept { path })
        .map_err(|e| e.to_string())
}

/// Make the settings `text` writes the settings, kept for the next run.
fn save_settings(text: String) -> Result<Answer, String> {
    let next = settings::read(&ConfigFile {
        path: settings::path(),
        text: FileText::Read { text },
    })?;
    settings::set(next)?;
    Ok(Answer::Done)
}

/// The image file at `path`, whole or as its small copy.
async fn picture(path: std::path::PathBuf, whole: bool) -> Answer {
    let url = crate::environment::spawn_blocking(move || {
        if whole {
            crate::pictures::file_shown(&path)
        } else {
            crate::pictures::file_thumb(&path)
        }
    })
    .await
    .ok()
    .flatten();
    Answer::Picture {
        url: url.map(|u| u.to_string()),
    }
}

/// What a keys file made without a binding in it starts with.
const KEYS_STARTER: &str = "# Key bindings over the defaults, area by area.\n";

/// Make `text` the key bindings, read again at once.
fn save_keys(text: &str) -> Result<Answer, String> {
    let path = crate::paths::keys_path();
    let text = if text.trim().is_empty() && !path.exists() {
        KEYS_STARTER
    } else {
        text
    };
    crate::config::write(&path, text)
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    let _ = settings::CHANGED.send(());
    Ok(Answer::Done)
}

fn save_settings_checked(text: String, expected: FileText) -> Result<Answer, String> {
    let path = settings::path();
    std::fs::create_dir_all(path.parent().ok_or("Settings path has no parent")?)
        .map_err(|e| e.to_string())?;
    use adroit_sys::fs::OpenOptionsExt;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path.with_extension("lock"))
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    if crate::config::file(&path).text != expected {
        return Err("Settings changed in another window; reload before saving".into());
    }
    save_settings(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_or_terminal_starts_on_this_host_alone() {
        assert_eq!(here(HostPath::local("/w")), Ok(PathBuf::from("/w")));
        let away = HostPath {
            host: adroit_protocol::HostName("build-box".into()),
            path: "/w".into(),
        };
        assert_eq!(
            here(away),
            Err("/w is on build-box, which cannot be reached".into())
        );
    }

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
