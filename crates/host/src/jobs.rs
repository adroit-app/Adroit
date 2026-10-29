//! What the host carries out for a window that takes a while: ending a
//! session or a terminal, killing one of its processes, naming a session
//! anew, letting a held program go. Each is
//! carried out on its own, and the window that asked is told how it went.

use std::time::Duration;

use adroit_model::{Agent, Process, Session};
use adroit_protocol::{Answer, Event, Request, Told, Typing};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::sessions::{self, id_of};
use crate::watch::Watched;
use crate::work::WindowId;

/// Carry out each request `jobs` brings, for as long as the host lives.
pub(crate) async fn run(watched: Watched, mut jobs: UnboundedReceiver<(WindowId, Request)>) {
    while let Some((window, job)) = jobs.recv().await {
        let watched = watched.clone();
        crate::environment::spawn(async move {
            let _ = apply(&watched, window, job).await;
        });
    }
}

pub(crate) async fn apply(watched: &Watched, window: WindowId, mut job: Request) -> Answer {
    if let Err(text) = authorize(&mut job).await {
        return Answer::Failed { text };
    }
    let uncertain_error = crate::remote::active()
        && matches!(
            &job,
            Request::Stop { .. } | Request::Rename { .. } | Request::EndTerminal { .. }
        );
    let result = match job {
        Request::Stop { session } => stop(watched, window, *session).await,
        Request::Kill { session, process } => kill(watched, window, *session, process).await,
        Request::Rename {
            id,
            name,
            was,
            agent,
        } => rename(watched, window, id, name, was, agent).await,
        Request::Release { id } => {
            crate::held::release(id).await;
            Ok(())
        }
        Request::EndTerminal { id } => end_terminal(watched, id).await,
        _ => Err("unsupported mutation request".into()),
    };
    match result {
        Ok(()) => Answer::Done,
        Err(text) if uncertain_error => Answer::Unconfirmed { text },
        Err(text) => Answer::Failed { text },
    }
}

async fn authorize(job: &mut Request) -> Result<(), String> {
    let id = match job {
        Request::Stop { session } | Request::Kill { session, .. } => {
            id_of(&session.id).ok_or("invalid session ID")?
        }
        Request::Rename { id, .. } => id_of(id).ok_or("invalid session ID")?,
        Request::Release { id } | Request::EndTerminal { id } => *id,
        _ => return Ok(()),
    };
    let listed = crate::daemon::daemon()
        .sessions()
        .await
        .map_err(|e| e.to_string())?;
    let Some(actual) = listed.iter().find(|session| session.id == id) else {
        return if matches!(job, Request::Release { .. } | Request::EndTerminal { .. }) {
            Ok(())
        } else {
            Err("the session no longer exists".into())
        };
    };
    if !crate::environment::owns(&actual.tags) {
        return Err("the session belongs to another working profile".into());
    }
    if let Request::Stop { session } | Request::Kill { session, .. } = job {
        let Some(sessions::Kept::Agent(actual)) = sessions::read(actual) else {
            return Err("the target is not an agent session".into());
        };
        **session = actual;
    }
    Ok(())
}

/// Ask each Neovim `quits` brings to quit, for as long as the host lives.
/// One that refuses, holding an unsaved change, runs on; one that quits is
/// listed no more once the sessions are next listed.
pub(crate) async fn quit(mut quits: UnboundedReceiver<u128>) {
    while let Some(id) = quits.recv().await {
        crate::environment::spawn(async move {
            // One that ended already has nothing to lose.
            let _ = crate::terminals::quit_neovim(&crate::daemon::daemon(), id).await;
        });
    }
}

/// End `asked`; its agent keeps its conversation.
async fn stop(watched: &Watched, window: WindowId, asked: Session) -> Result<(), String> {
    let Some(id) = id_of(&asked.id) else {
        return Err("invalid session ID".into());
    };
    let result = match crate::daemon::daemon().end(id).await {
        Ok(()) => {
            watched.tell(window, notice(format!("ended {}", asked.name)));
            if let Some(record) = asked.delegation {
                let root = crate::delegation::app::root();
                if let Err(error) = crate::delegation::app::ended(&root, record).await {
                    watched.tell(
                        window,
                        failure(format!(
                            "ended {}, but cannot record its exit: {error}",
                            asked.name
                        )),
                    );
                }
            }
            Ok(())
        }
        Err(e) => {
            watched.tell(window, failure(format!("cannot end {}: {e}", asked.name)));
            Err(e.to_string())
        }
    };
    watched.relist();
    result
}

/// End the terminal `id`, and list it no more.
async fn end_terminal(watched: &Watched, id: u128) -> Result<(), String> {
    let daemon = crate::daemon::daemon();
    if let Err(error) = daemon.end(id).await {
        // A terminal confirmed absent is ended already. A lost RPC answer
        // alone cannot confirm that the program stopped.
        let absent = daemon
            .sessions()
            .await
            .is_ok_and(|sessions| sessions.iter().all(|session| session.id != id));
        if !absent {
            return Err(error.to_string());
        }
    }
    watched.terminal_closed(id);
    Ok(())
}

/// Kill `process`, which runs in `session`.
async fn kill(
    watched: &Watched,
    window: WindowId,
    session: Session,
    process: Process,
) -> Result<(), String> {
    let pid = process.pid;
    let killed =
        crate::environment::spawn_blocking(move || crate::processes::kill(&session, &process))
            .await;
    let result = killed.map_err(|e| e.to_string()).and_then(|result| result);
    watched.tell(
        window,
        notice(match &result {
            Ok(()) => format!("Killed process {pid}"),
            Err(e) => e.clone(),
        }),
    );
    watched.read_again();
    result
}

/// Name the session `id` anew: in its tags, which the tree shows, and to
/// its agent with its own command, so its interface says the same.
async fn rename(
    watched: &Watched,
    window: WindowId,
    id: String,
    name: String,
    was: String,
    agent: Agent,
) -> Result<(), String> {
    let Some(terminal) = id_of(&id) else {
        return Err("invalid session ID".into());
    };
    let daemon = crate::daemon::daemon();
    let held = daemon
        .sessions()
        .await
        .ok()
        .and_then(|all| all.into_iter().find(|s| s.id == terminal))
        .map(|s| adroit_term::tags::holding(&s.tags, sessions::NAME))
        .unwrap_or_default();
    let named = adroit_term::tags::tagged(sessions::NAME, &name);
    if let Err(e) = daemon.tag(terminal, named, held).await {
        watched.tell(window, failure(format!("cannot rename {was}: {e}")));
        return Err(e.to_string());
    }
    watched.relist();
    if let Some(command) = adroit_agents::adapter(agent).rename_command(&name) {
        if crate::remote::active() {
            return crate::asks::type_prompt(terminal, &typing_of(command)).await;
        }
        watched.tell(
            window,
            Event::Type {
                session: id,
                typing: typing_of(command),
            },
        );
    }
    Ok(())
}

/// How long a program is left after the command before Enter.
const SETTLE: Duration = Duration::from_millis(40);

/// The steps that run `command` in a session's input.
fn typing_of(command: String) -> Vec<Typing> {
    vec![
        Typing::Paste { text: command },
        Typing::Pause {
            ms: SETTLE.as_millis() as u64,
        },
        Typing::Key {
            name: "Enter".into(),
        },
    ]
}

fn notice(text: String) -> Event {
    Event::Told(Told::Notice { text })
}

fn failure(text: String) -> Event {
    Event::Told(Told::Failure { text })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_pasted_and_entered_after_a_moment() {
        assert_eq!(
            typing_of("/rename fixer".into()),
            [
                Typing::Paste {
                    text: "/rename fixer".into()
                },
                Typing::Pause { ms: 40 },
                Typing::Key {
                    name: "Enter".into()
                },
            ]
        );
    }
}
