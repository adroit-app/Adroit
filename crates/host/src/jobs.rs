//! What the host carries out for a window that takes a while: ending a
//! session or a terminal, killing one of its processes, naming a session
//! anew, letting a held program go. Each is
//! carried out on its own, and the window that asked is told how it went.

use std::time::Duration;

use adroit_model::{Agent, Process, Session};
use adroit_protocol::{Event, Request, Told, Typing};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::sessions::{self, id_of};
use crate::watch::Watched;
use crate::work::WindowId;

/// Carry out each request `jobs` brings, for as long as the host lives.
pub(crate) async fn run(watched: Watched, mut jobs: UnboundedReceiver<(WindowId, Request)>) {
    while let Some((window, job)) = jobs.recv().await {
        let watched = watched.clone();
        tokio::spawn(async move {
            match job {
                Request::Stop { session } => stop(&watched, window, *session).await,
                Request::Kill { session, process } => {
                    kill(&watched, window, *session, process).await
                }
                Request::Rename {
                    id,
                    name,
                    was,
                    agent,
                } => rename(&watched, window, id, name, was, agent).await,
                Request::Release { id } => crate::held::release(id).await,
                Request::EndTerminal { id } => end_terminal(&watched, id).await,
                _ => {}
            }
        });
    }
}

/// Ask each Neovim `quits` brings to quit, for as long as the host lives.
/// One that refuses, holding an unsaved change, runs on; one that quits is
/// listed no more once the sessions are next listed.
pub(crate) async fn quit(mut quits: UnboundedReceiver<u128>) {
    while let Some(id) = quits.recv().await {
        tokio::spawn(async move {
            // One that ended already has nothing to lose.
            let _ = crate::terminals::quit_neovim(&crate::daemon::daemon(), id).await;
        });
    }
}

/// End `asked`; its agent keeps its conversation.
async fn stop(watched: &Watched, window: WindowId, asked: Session) {
    let Some(id) = id_of(&asked.id) else {
        return;
    };
    match crate::daemon::daemon().end(id).await {
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
        }
        Err(e) => watched.tell(window, failure(format!("cannot end {}: {e}", asked.name))),
    }
    watched.relist();
}

/// End the terminal `id`, and list it no more.
async fn end_terminal(watched: &Watched, id: u128) {
    // One ended already is gone all the same.
    let _ = crate::daemon::daemon().end(id).await;
    watched.terminal_closed(id);
}

/// Kill `process`, which runs in `session`.
async fn kill(watched: &Watched, window: WindowId, session: Session, process: Process) {
    let pid = process.pid;
    let killed =
        tokio::task::spawn_blocking(move || crate::processes::kill(&session, &process)).await;
    watched.tell(
        window,
        notice(match killed {
            Ok(Ok(())) => format!("Killed process {pid}"),
            Ok(Err(e)) => e,
            Err(e) => format!("Cannot kill process: {e}"),
        }),
    );
    watched.read_again();
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
) {
    let Some(terminal) = id_of(&id) else {
        return;
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
        return;
    }
    watched.relist();
    if let Some(command) = adroit_agents::adapter(agent).rename_command(&name) {
        watched.tell(
            window,
            Event::Type {
                session: id,
                typing: typing_of(command),
            },
        );
    }
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
