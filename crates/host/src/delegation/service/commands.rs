//! The commands a session's agent sends that change or read what Adroit
//! keeps: its timeline, its name, its end, and the sessions it opens or lists.
use std::time::Duration;

use adroit_model::timeline::{Entry, Timeline};
use adroit_term::tags;
use serde_json::Value;

use super::Context;
use super::serve::answer;
use crate::delegation::{Code, Error, Result, Session, protocol};
use crate::starting::Stage;

/// Make the changes the caller's agent asks of its timeline and journal
/// them; the trees as they now stand, with each change that could not be
/// made. A call none of whose changes could be made is refused and left out
/// of the journal.
pub(super) async fn record_timeline(
    context: &Context,
    caller: &Session,
    asked: protocol::TimelineTasks,
) -> Result<Value> {
    if asked.ops.is_empty() {
        return Err(Error::new(Code::Invalid, "ops names no change to make"));
    }
    let code = caller.code.clone();
    context
        .call(move |store| {
            let mut timeline = Timeline::replay(&store.timeline_of(&code)?);
            let entry = Entry {
                at_ms: crate::delegation::now_ms(),
                ops: asked.ops,
            };
            let refused = timeline.apply(&entry);
            if refused.len() == entry.ops.len() {
                return Err(Error::new(
                    Code::Invalid,
                    format!(
                        "no change could be made:
{}",
                        refused.join(
                            "
"
                        )
                    ),
                ));
            }
            store.record_timeline(&code, &entry)?;
            let mut answer = timeline.text();
            if !refused.is_empty() {
                answer.push_str(
                    "
Not made:
",
                );
                answer.push_str(&refused.join(
                    "
",
                ));
            }
            Ok(Value::String(answer))
        })
        .await
}

/// Name the caller's session as its agent asks, in its tags, which the tree
/// shows, and in its record. A name a person gave is kept, and so is every
/// name while the user has switched renaming off. Its agent is not told
/// with its own command, which would have to be typed while it works.
pub(super) async fn rename(
    context: &Context,
    caller: &Session,
    asked: protocol::Rename,
) -> Result<Value> {
    if !context.settings()?.rename_sessions {
        return Err(Error::new(
            Code::Refused,
            format!(
                "the user has switched off sessions naming themselves; keep the name {}",
                caller.name
            ),
        ));
    }
    if caller.name_given {
        return Err(Error::new(
            Code::Refused,
            format!("the user named this session {}; keep the name", caller.name),
        ));
    }
    let label = crate::content::label_of(&asked.name)
        .ok_or_else(|| Error::new(Code::Invalid, "the name holds no letters or digits"))?;
    let terminal = caller
        .terminal
        .ok_or_else(|| Error::new(Code::Refused, "the session has no terminal yet"))?;
    let listed = context.daemon.sessions().await.map_err(Error::storage)?;
    let name = crate::starting::fresh_name(&label, &listed, Some(terminal));
    let held = listed
        .iter()
        .find(|s| s.id == terminal)
        .map(|s| tags::holding(&s.tags, crate::sessions::NAME))
        .unwrap_or_default();
    context
        .daemon
        .tag(terminal, tags::tagged(crate::sessions::NAME, &name), held)
        .await
        .map_err(Error::storage)?;
    let (id, kept) = (caller.id, name.clone());
    context
        .call(move |store| store.rename(id, &kept, false))
        .await?;
    answer(protocol::Renamed { name })
}

/// How long a session closing itself runs on once it is answered, for its
/// agent to take the answer in.
const CLOSING: Duration = Duration::from_secs(1);

/// End the caller's session as the user ends one, a moment after it is
/// answered: ending it at once would cut its agent off before the answer
/// reached it, and the agent would show the call as failed.
pub(super) async fn close(context: &Context, caller: &Session) -> Result<Value> {
    let terminal = caller
        .terminal
        .ok_or_else(|| Error::new(Code::Refused, "the session has no terminal yet"))?;
    let (context, id) = (context.clone(), caller.id);
    crate::environment::spawn(async move {
        tokio::time::sleep(CLOSING).await;
        if let Err(error) = context.daemon.end(terminal).await {
            tracing::warn!(%id, %error, "cannot end a session that closed itself");
            return;
        }
        if let Err(error) = context.call(move |store| store.end(id)).await {
            tracing::warn!(
                %id,
                %error,
                "cannot record a session that closed itself as ended"
            );
        }
    });
    answer(protocol::Closing { closing: true })
}

/// What the caller's agent is told as a prompt arrives or once its
/// conversation was cleared: of its session's name, and, while nothing is on
/// its timeline, to record the plan of its work there. Nothing when neither
/// applies.
pub(super) async fn prompted(context: &Context, caller: &Session, cleared: bool) -> Result<Value> {
    let naming = naming(context, caller, cleared).await?;
    let code = caller.code.clone();
    let empty = context
        .call(move |store| store.timeline_of(&code))
        .await?
        .is_empty();
    let timeline = empty.then(|| crate::delegation::launch::TIMELINE_AT_PROMPT.to_owned());
    let note = match (naming, timeline) {
        (Some(naming), Some(timeline)) => Some(format!("{naming}\n\n{timeline}")),
        (naming, timeline) => naming.or(timeline),
    };
    answer(protocol::Note { note })
}

/// What the caller's agent is told of its session's name: once its
/// conversation was cleared, that the name is of the work before, even one
/// a person gave, which a clear hands back to the agent; as each prompt
/// arrives, to weigh the name against the work. Nothing while renaming is
/// switched off, or at a prompt while a person's name stands.
async fn naming(context: &Context, caller: &Session, cleared: bool) -> Result<Option<String>> {
    if !context.settings()?.rename_sessions {
        return Ok(None);
    }
    if cleared {
        if caller.name_given {
            let id = caller.id;
            context.call(move |store| store.hand_back_name(id)).await?;
        }
        return Ok(Some(crate::delegation::launch::naming_after_clear(
            &caller.name,
        )));
    }
    Ok((!caller.name_given).then(|| crate::delegation::launch::naming_at_prompt(&caller.name)))
}

pub(super) async fn start(
    context: &Context,
    caller: &Session,
    start: protocol::Start,
) -> Result<Value> {
    let prompt = start.prompt.trim().to_owned();
    if prompt.is_empty() {
        return Err(Error::new(Code::Invalid, "the prompt is empty"));
    }
    let settings = context.settings()?;
    if !settings.delegation.enabled {
        return Err(Error::new(
            Code::Refused,
            "the user has switched off sessions opening sessions; carry on yourself, or ask the user to open the session",
        ));
    }
    // The caller's own profile is not passed on: a session starts from the
    // one it names, else from the default.
    let chosen = crate::programs::starting(&settings, start.agent, start.profile.as_ref())
        .map_err(|error| Error::new(Code::Invalid, error))?;
    let (agent, profile) = (chosen.agent(), chosen.profile);
    context
        .models
        .lock()
        .expect("the models' lock")
        .check(&profile, start.model.as_deref())
        .map_err(|error| Error::new(Code::Invalid, error))?;
    let dir = start.dir.clone().unwrap_or_else(|| caller.cwd.clone());
    let dir = adroit_sys::fs::canonicalize(&dir)
        .ok()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| Error::new(Code::Invalid, format!("{} is no directory", dir.display())))?;
    if start.base.is_some() && !start.worktree {
        return Err(Error::new(
            Code::Invalid,
            "a base revision needs a new worktree (worktree: true)",
        ));
    }
    let listed = context.daemon.sessions().await.map_err(Error::storage)?;
    let request = crate::execution::StartRequest {
        agent,
        profile: Some(profile),
        model: start.model,
        name: crate::starting::name_for(start.name.as_deref(), &prompt, &listed),
        prompt,
        permissions: start.permissions.unwrap_or_default(),
        env: crate::daemon::agent_env(),
        started_ms: crate::delegation::now_ms(),
        show: !start.background,
        ..Default::default()
    };
    let place = crate::starting::Place {
        dir,
        worktree: start.worktree,
        base: start.base,
    };
    let host = crate::starting::Host {
        daemon: &context.daemon,
        root: &context.root,
        program: &context.program,
    };
    let started = crate::starting::start(host, &settings, Some(caller.id), place, request)
        .await
        .map_err(|failure| {
            let code = match failure.stage {
                Stage::Refused => Code::Invalid,
                Stage::NotStarted | Stage::MayHaveRun => Code::Storage,
            };
            Error::new(code, failure.message)
        })?;
    answer(protocol::Started {
        id: started.recorded.id,
        code: started.recorded.code,
        name: started.name,
        dir: started.dir,
    })
}

/// The sessions that have not ended, narrowed as `filter` asks.
pub(super) async fn listed(all: Vec<Session>, filter: &protocol::Sessions) -> Result<Value> {
    let wanted_repo = match &filter.repo {
        Some(dir) => Some(adroit_vcs::common_dir(dir).await.ok_or_else(|| {
            Error::new(
                Code::Invalid,
                format!("{} is in no repository", dir.display()),
            )
        })?),
        None => None,
    };
    let wanted_dir = filter
        .dir
        .as_ref()
        .map(|dir| adroit_sys::fs::canonicalize(dir).unwrap_or(dir.clone()));
    let mut out = Vec::new();
    for session in all.iter().filter(|session| !session.ended) {
        let repository = adroit_vcs::common_dir(&session.cwd).await;
        if wanted_repo.is_some() && repository != wanted_repo {
            continue;
        }
        if let Some(dir) = &wanted_dir
            && !session.cwd.starts_with(dir)
        {
            continue;
        }
        let opened_by = session
            .parent
            .and_then(|parent| all.iter().find(|s| s.id == parent))
            .map(protocol::Opener::of);
        out.push(protocol::Listed {
            id: session.id,
            code: session.code.clone(),
            name: session.name.clone(),
            agent: session.agent,
            dir: session.cwd.clone(),
            repository,
            readiness: session.readiness,
            opened_by,
        });
    }
    answer(out)
}

#[cfg(test)]
mod tests;
