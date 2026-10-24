//! What the work host answers when a window asks and waits: starting
//! sessions and terminals held until the window has told them the theme,
//! the terminal host's standing and its restart, keeping a pasted image,
//! saving the settings.

use std::path::PathBuf;
use std::sync::Arc;

use adroit_model::profile::ProfileId;
use adroit_model::time::now_ms;
use adroit_model::{TerminalProgram, TerminalResourceId, TerminalSession};
use adroit_protocol::{Answer, Ask, ConfigFile, FileText, HostPath, SessionStart, TerminalStart};
use adroit_term::Spawn;
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
        Ask::KeepImageFile { path } => tokio::task::spawn_blocking(move || keep_image_file(&path))
            .await
            .map_err(|e| e.to_string())
            .and_then(|kept| kept),
        Ask::SaveSettings { text } => save_settings(text),
        Ask::SaveKeys { text } => save_keys(&text),
        Ask::Drafts => {
            let dir = crate::paths::instance_state();
            tokio::task::spawn_blocking(move || crate::drafts::load(&dir))
                .await
                .map_err(|e| e.to_string())
                .and_then(|kept| kept)
                .map(|drafts| Answer::Drafts(Box::new(drafts)))
        }
        Ask::KeepDrafts(changes) => {
            let dir = crate::paths::instance_state();
            tokio::task::spawn_blocking(move || crate::drafts::change(&dir, *changes))
                .await
                .map_err(|e| e.to_string())
                .and_then(|kept| kept)
                .map(|()| Answer::Done)
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
/// it: a draft's in a new folder of its own when it is in the scratch folder.
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
        scratch::start_dir(&at, &root, Moment::now())
            .map_err(|e| format!("cannot make a directory for {name}: {e}"))?
    } else {
        at
    };
    let request = StartRequest {
        agent: start.agent,
        profile: start.profile,
        name: name.clone(),
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
            recorded.failed().await;
            return Err(format!("cannot {verb} {name}: {e}"));
        }
    };
    if let Err(e) = recorded.attach(id).await {
        let _ = terminal_host.end(id).await;
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
        },
        &programs::login_shell(),
    );
    (resolved.spawn.cols, resolved.spawn.rows) = size(start.cols, start.rows);
    let id = daemon::daemon()
        .spawn_held(resolved.spawn)
        .await
        .map_err(|e| e.to_string())?;
    held::hold(id).await;
    resolved.terminal.id = id;
    watched.terminal_opened(resolved.terminal.clone());
    Ok(Answer::Terminal {
        terminal: resolved.terminal,
    })
}

/// Whether the profile `profile` is logged in, as the CLI it starts says
/// where it runs it, asked in the user's home.
async fn login_standing(profile: &ProfileId) -> Result<Answer, String> {
    let context = programs::for_profile(&settings::current(), profile)?;
    let standing = adroit_agents::login::standing(&context, &crate::paths::home()).await;
    Ok(Answer::LoginStanding { standing })
}

/// Delete the folder the profile `profile` keeps its login in, as the
/// settings say now: one adroit made, never the agent's own environment
/// nor a folder named by hand, and none a session still runs in.
async fn delete_login(profile: &ProfileId) -> Result<Answer, String> {
    let settings = settings::current();
    let found = settings
        .profiles
        .iter()
        .find(|p| &p.id == profile)
        .ok_or_else(|| format!("profile `{profile}` is not defined"))?;
    let Some(home) = found.home.clone() else {
        return Err(format!(
            "{} runs in your terminal's login, of which adroit deletes nothing",
            found.label()
        ));
    };
    let data = adroit_model::paths::data_dir();
    if !adroit_core::profiles::made_by_adroit(&home, &data, |p| std::fs::canonicalize(p).ok()) {
        return Err(format!(
            "{} is not a folder adroit made, so it is left as it is; delete it yourself if it is to go",
            home.display()
        ));
    }
    let kept = daemon::daemon()
        .sessions()
        .await
        .map_err(|e| e.to_string())?;
    let running: Vec<adroit_model::Session> = kept
        .iter()
        .filter_map(sessions::read)
        .filter_map(|k| match k {
            sessions::Kept::Agent(s) => Some(s),
            sessions::Kept::Terminal(_) => None,
        })
        .collect();
    unused(found.label(), &home, &running)?;
    match tokio::fs::remove_dir_all(&home).await {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("{} could not be deleted: {e}", home.display()))
        }
        _ => Ok(Answer::Done),
    }
}

/// Whether no session of `running` runs in `home`, the folder of the
/// profile called `name`; why it cannot be deleted otherwise, naming them.
fn unused(
    name: &str,
    home: &std::path::Path,
    running: &[adroit_model::Session],
) -> Result<(), String> {
    let using: Vec<&str> = running
        .iter()
        .filter(|s| s.home.as_deref() == Some(home))
        .map(|s| match s.name.trim() {
            "" => s.id.as_str(),
            named => named,
        })
        .collect();
    if using.is_empty() {
        return Ok(());
    }
    let (runs, end) = match using.len() {
        1 => ("runs", "it"),
        _ => ("run", "them"),
    };
    Err(format!(
        "{} {runs} in {}; end {end}, or delete {name} without its folder",
        using.join(", "),
        home.display()
    ))
}

/// Start a terminal, listed in the tree as any other, that runs the CLI's
/// own login where the profile `profile` runs it, in the user's home.
async fn open_login(
    watched: &Watched,
    profile: &ProfileId,
    cols: u16,
    rows: u16,
) -> Result<Answer, String> {
    let settings = settings::current();
    let context = programs::for_profile(&settings, profile)?;
    let (program, args, env) = adroit_agents::login::login(&context);
    let cwd = crate::paths::home();
    let terminal = TerminalSession {
        resource_id: TerminalResourceId::new(),
        id: 0,
        name: format!(
            "log in · {}",
            settings
                .find_profile(profile)
                .map_or_else(|| profile.to_string(), |p| p.label().to_string())
        ),
        cwd: cwd.clone(),
        program: TerminalProgram::Command,
        file: None,
        command: std::iter::once(program.clone())
            .chain(args.clone())
            .collect(),
    };
    let (cols, rows) = size(cols, rows);
    let spawn = Spawn {
        tags: sessions::terminal_tags(&terminal),
        program,
        args,
        cwd,
        env: daemon::terminal_env().into_iter().chain(env).collect(),
        cols,
        rows,
    };
    let id = daemon::daemon()
        .spawn_held(spawn)
        .await
        .map_err(|e| e.to_string())?;
    held::hold(id).await;
    // Settings, which opens it too, shows no terminal to wait for; the
    // host's own presence has told the program the theme already.
    held::release(id).await;
    let terminal = TerminalSession { id, ..terminal };
    watched.terminal_opened(terminal.clone());
    Ok(Answer::Terminal { terminal })
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
    let url = tokio::task::spawn_blocking(move || {
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

    #[test]
    fn a_login_s_folder_is_not_deleted_while_sessions_run_in_it_and_says_which() {
        use adroit_model::testing::session;
        let home = std::path::Path::new("/d/adroit/profiles/work");
        let elsewhere = [
            session("a")
                .name("fix")
                .profile("work", Some("/d/x"))
                .build(),
            session("b").name("fix").profile("claude", None).build(),
        ];
        assert_eq!(unused("Work", home, &elsewhere), Ok(()));
        let one = [session("a")
            .name("fix")
            .profile("work", Some("/d/adroit/profiles/work"))
            .build()];
        assert_eq!(
            unused("Work", home, &one),
            Err(
                "fix runs in /d/adroit/profiles/work; end it, or delete Work without its folder"
                    .into()
            )
        );
        let two = [
            one[0].clone(),
            session("0c")
                .name(" ")
                .profile("old", Some("/d/adroit/profiles/work"))
                .build(),
        ];
        assert_eq!(
            unused("Work", home, &two),
            Err("fix, 0c run in /d/adroit/profiles/work; end them, or delete Work without its folder".into())
        );
    }

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
