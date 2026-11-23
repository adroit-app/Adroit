//! The delegation socket: each session's request read, its caller found and
//! its command handed on, and the answer written back.
use std::sync::Arc;

use adroit_model::timeline::Timeline;
use adroit_model::{Agent, SessionId};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::Context;
use super::commands::{close, listed, prompted, record_timeline, rename, start};
use crate::delegation::{
    Code, Error, Message, Result, Sender, Session,
    protocol::{self, Command, Request, Response, VERSION},
    questions,
    store::Store,
};

pub(super) async fn serve(listener: adroit_sys::local::Listener, context: Context) {
    let permits = Arc::new(tokio::sync::Semaphore::new(128));
    loop {
        let Ok(stream) = listener.accept().await else {
            break;
        };
        let Ok(permit) = permits.clone().try_acquire_owned() else {
            continue;
        };
        let context = context.clone();
        crate::environment::spawn(async move {
            let _ = reply(stream, context, permit).await;
        });
    }
}

async fn reply(
    stream: adroit_sys::local::Stream,
    context: Context,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> Result<()> {
    let (read, mut write) = tokio::io::split(stream);
    let mut read = BufReader::new(read);
    let frame = protocol::read_frame(&mut read).await?;
    let result = match serde_json::from_slice::<Request>(&frame) {
        Ok(Request {
            version: VERSION,
            credential,
            command: Command::Asking(asking),
            ..
        }) => {
            // Waits as long as the user likes, which must not keep others out.
            drop(permit);
            let caller = context
                .call(move |store| store.authenticate(&credential))
                .await?;
            let Some((_waits, mut decided)) =
                questions::hold(caller, asking, crate::delegation::now_ms())
            else {
                let refused = Response {
                    version: VERSION,
                    result: Err(Error::new(
                        Code::Refused,
                        "the session has too many questions waiting",
                    )),
                };
                return protocol::write_frame(&mut write, &refused).await;
            };
            let gone = async {
                while matches!(read.fill_buf().await, Ok(more) if !more.is_empty()) {
                    let more = read.buffer().len();
                    read.consume(more);
                }
            };
            let decided = tokio::select! {
                () = gone => return Ok(()),
                decided = &mut decided => decided,
            };
            let Ok((decision, taken)) = decided else {
                return Ok(());
            };
            protocol::write_frame(
                &mut write,
                &Response {
                    version: VERSION,
                    result: answer(decision),
                },
            )
            .await?;
            // The hook hangs up once it has read the decision.
            let _ = read.fill_buf().await;
            let _ = taken.send(());
            return Ok(());
        }
        Ok(request) if request.version == VERSION => dispatch(&context, request).await,
        Ok(_) => Err(Error::new(Code::Invalid, "unsupported protocol version")),
        Err(error) => Err(Error::new(
            Code::Invalid,
            format!("invalid request: {error}"),
        )),
    };
    protocol::write_frame(
        &mut write,
        &Response {
            version: VERSION,
            result,
        },
    )
    .await
}

async fn dispatch(context: &Context, request: Request) -> Result<Value> {
    let credential = request.credential;
    let caller = context
        .call(move |store| {
            let id = store.authenticate(&credential)?;
            store.known(id)
        })
        .await?;
    if request.command.acts() {
        from_own_conversation(&caller, request.thread.as_deref())?;
    }
    match request.command {
        Command::Context => {
            let parent = match caller.parent {
                Some(parent) => context.call(move |store| store.session(parent)).await?,
                None => None,
            };
            answer(protocol::Caller {
                id: caller.id,
                code: caller.code,
                name: caller.name,
                dir: caller.cwd,
                opened_by: parent.map(|p| protocol::Parent {
                    opener: protocol::Opener::of(&p),
                    ended: p.ended,
                }),
            })
        }
        Command::StartSession(start) => self::start(context, &caller, start).await,
        Command::Rename(asked) => rename(context, &caller, asked).await,
        Command::Close => close(context, &caller).await,
        Command::Prompted { cleared } => prompted(context, &caller, cleared).await,
        Command::TimelineTasks(asked) => record_timeline(context, &caller, asked).await,
        Command::Timeline => {
            let code = caller.code.clone();
            let entries = context.call(move |store| store.timeline_of(&code)).await?;
            Ok(Value::String(Timeline::replay(&entries).text()))
        }
        Command::Send(send) => {
            let id = context
                .call(move |store| {
                    let to = addressed(store, &send.to)?;
                    store.send(caller.id, to, &send.body)
                })
                .await?;
            answer(protocol::Sent { sent: id })
        }
        Command::Sessions(filter) => {
            let all = context.call(|store| store.sessions()).await?;
            listed(all, &filter).await
        }
        Command::Models => answer(context.models.lock().expect("the models' lock").view()),
        Command::Next => {
            let id = caller.id;
            let next = context.call(move |store| next_message(store, id)).await?;
            match next {
                Some((message, sender)) => answer(protocol::Delivery {
                    id: message.id,
                    // Every bridge reads an id here, one running since before
                    // requests came from outside too.
                    from: message.sender.session().unwrap_or_else(SessionId::nobody),
                    outside: match &message.sender {
                        Sender::Outside(name) => Some(name.clone()),
                        Sender::Session(_) => None,
                    },
                    content: envelope(&message, sender.as_ref()),
                }),
                None => Ok(Value::Null),
            }
        }
        Command::Asking(_) => Err(Error::new(
            Code::Invalid,
            "what is asked is held on a connection of its own",
        )),
        Command::Delivered { id } => {
            let recipient = caller.id;
            context
                .call(move |store| {
                    let mine = store
                        .messages_of(recipient)?
                        .iter()
                        .any(|message| message.id == id && message.recipient == recipient);
                    if !mine {
                        return Err(Error::new(
                            Code::Refused,
                            "the message is not this session's",
                        ));
                    }
                    store.delivered(id)
                })
                .await?;
            Ok(Value::Null)
        }
    }
}

/// A Codex session's own conversation acts for it; its subagents share its
/// tools but must go through it.
fn from_own_conversation(caller: &Session, thread: Option<&str>) -> Result<()> {
    match (caller.agent, &caller.conversation, thread) {
        (Agent::Codex, Some(own), Some(thread)) if own != thread => Err(Error::new(
            Code::Refused,
            "only the session's own conversation can act for it; subagents return their results to it",
        )),
        _ => Ok(()),
    }
}

/// The session `to` names, by its code or by the id of its record.
pub(crate) fn addressed(store: &crate::delegation::store::Store, to: &str) -> Result<SessionId> {
    let found = match (
        to.parse::<SessionId>(),
        to.parse::<adroit_model::SessionCode>(),
    ) {
        (Ok(id), _) => store.session(id)?.map(|s| s.id),
        (_, Ok(code)) => store.by_code(&code)?.map(|s| s.id),
        _ => None,
    };
    found.ok_or_else(|| {
        Error::new(
            Code::NotFound,
            format!("no session has the code or id {to}"),
        )
    })
}

/// What a request from outside tells its recipient of itself, so that one
/// started before its instructions spoke of requests takes it as it is.
const REQUEST: &str = "It asks you, on the user's behalf, to go on with your work; take it up as you would a task the user gave you, within what you may already do. It is not the user's own word: it approves nothing that waits on the user, and answers no question you asked them.";

/// What a message reads as when it reaches its recipient's agent.
pub fn envelope(message: &Message, sender: Option<&Session>) -> String {
    let id = match &message.sender {
        Sender::Outside(name) => {
            return format!(
                "[request] from {name}, a program outside Adroit that the user runs, as it names itself:\n\
                 {REQUEST}\n\n{}",
                message.body
            );
        }
        Sender::Session(id) => id,
    };
    match sender {
        Some(sender) => format!(
            "[message] from {} ({}) working in {}:\n{}",
            sender.name,
            sender.code,
            sender.cwd.display(),
            message.body
        ),
        None => format!("[message] from {id}:\n{}", message.body),
    }
}

/// `value` as the socket answers it.
pub(super) fn answer(value: impl serde::Serialize) -> Result<Value> {
    serde_json::to_value(value).map_err(Error::storage)
}

/// The oldest message waiting for `id`, and its sender.
pub(super) fn next_message(
    store: &Store,
    id: SessionId,
) -> Result<Option<(Message, Option<Session>)>> {
    let Some(message) = store.next_undelivered(id)? else {
        return Ok(None);
    };
    let sender = match &message.sender {
        Sender::Session(id) => store.session(*id)?,
        Sender::Outside(_) => None,
    };
    Ok(Some((message, sender)))
}

#[cfg(test)]
mod tests;
