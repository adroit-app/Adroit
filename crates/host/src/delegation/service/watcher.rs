//! The watch kept on every recorded session: how it starts, whether it still
//! runs, the conversation it holds, and Codex's messages handed over.
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use adroit_model::{Agent, SessionId, StartupReadiness};
use adroit_term::tags;

use super::serve::{envelope, next_message};
use super::{Context, STALL};
use crate::delegation::{
    Error, Message, Readiness, Result, Session, launch::CLAUDE_CHANNEL, store::Store,
};
use crate::restore::Fate;
use crate::settings::RestoreSessions;

/// The tag a terminal carries once its warning about Adroit's channel was
/// confirmed, so that one launch is never confirmed twice.
const CHANNEL_CONFIRMED: &str = "adroit.channel-confirmed";

/// `told`, unless the terminal host is taking down what it tells ended.
/// The listing is read before it is asked, so whatever its stop ended is
/// already ended by the time it says it drains.
async fn judged(context: &Context, told: Fate) -> Fate {
    if told != Fate::Ended {
        return told;
    }
    crate::restore::settled(told, context.daemon.draining().await.ok())
}

pub(super) async fn supervise(context: Context) {
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut watcher = Watcher::default();
    let mut last_error = None;
    loop {
        interval.tick().await;
        let result = watcher.tick(&context).await;
        let error = result.err().map(|error| error.to_string());
        if let Some(error) = error.as_deref().filter(|_| error != last_error) {
            tracing::warn!(%error, "cannot watch sessions");
        }
        last_error = error;
    }
}

#[derive(Default)]
struct Watcher {
    /// Codex sessions a message is being handed to, one at a time each.
    delivering: HashSet<SessionId>,
    deliveries: tokio::task::JoinSet<SessionId>,
    /// Codex conversations being looked up from a shortened title.
    lookups: tokio::task::JoinSet<(SessionId, String, Option<String>)>,
    looking: HashSet<SessionId>,
    resolved: HashMap<SessionId, (String, String)>,
}

impl Watcher {
    async fn tick(&mut self, context: &Context) -> Result<()> {
        while let Some(done) = self.deliveries.try_join_next() {
            if let Ok(id) = done {
                self.delivering.remove(&id);
            }
        }
        while let Some(found) = self.lookups.try_join_next() {
            if let Ok((id, title, conversation)) = found {
                self.looking.remove(&id);
                if let Some(conversation) = conversation {
                    self.resolved.insert(id, (title, conversation));
                }
            }
        }
        let native = tokio::time::timeout(Duration::from_secs(3), context.daemon.sessions())
            .await
            .map_err(|_| Error::storage("the terminal daemon did not answer"))?
            .map_err(Error::storage)?;
        let now = context.daemon.generation();
        let mut interrupted = self.terminals(context, &native, now.as_deref()).await?;
        let records = context.call(|store| store.sessions()).await?;
        let all = records.clone();
        for mut record in records.into_iter().filter(|record| !record.ended) {
            let Some(terminal) = record.terminal else {
                continue;
            };
            let listed = native.iter().find(|native| native.id == terminal);
            if listed.is_none_or(|native| native.exited) {
                let id = record.id;
                let told = crate::restore::fate(
                    listed.map(|native| native.exited),
                    record.generation.as_deref(),
                    now.as_deref(),
                );
                match judged(context, told).await {
                    Fate::Interrupted => {
                        context.call(move |store| store.interrupt(id)).await?;
                        interrupted = true;
                    }
                    Fate::Ended => context.call(move |store| store.end(id)).await?,
                    Fate::Runs | Fate::Unknown => {}
                }
                continue;
            }
            let native = listed.expect("a listed terminal that runs");
            crate::outside::began(&record, &all, native)
                .await
                .map_err(Error::storage)?;
            if record.readiness != Readiness::Ready {
                let screen = context
                    .daemon
                    .visible(terminal)
                    .await
                    .map_err(Error::storage)?;
                self.starting(context, &record, native, &screen).await?;
            }
            if record.agent == Agent::Codex {
                let current = self
                    .follow_conversation(context, &mut record, native)
                    .await?;
                if current && record.readiness == Readiness::Ready {
                    self.deliver(context, &record).await?;
                }
            } else {
                Self::learn_tagged_conversation(context, &mut record, native).await?;
            }
        }
        let restore = context
            .settings()
            .map_or(RestoreSessions::Ask, |settings| settings.restore_sessions);
        if interrupted && restore != RestoreSessions::Ask {
            let waiting = context
                .call(|store| {
                    Ok(crate::restore::listed(
                        &store.interrupted()?,
                        &store.interrupted_terminals()?,
                    ))
                })
                .await?;
            let chosen: Vec<String> = waiting.into_iter().map(|w| w.id).collect();
            tokio::spawn(async move {
                let done = match restore {
                    RestoreSessions::Always => crate::restore::bring_back(chosen).await.map(drop),
                    _ => crate::restore::let_go(chosen).await,
                };
                if let Err(error) = done {
                    tracing::warn!(%error, "cannot settle what the terminal host took with it");
                }
            });
        }
        Ok(())
    }

    /// Records the conversation the session's daemon session is tagged as
    /// holding, which the work host keeps there as it follows the agent.
    /// Codex's is followed from its title here instead, as delivering to it
    /// needs the conversation the moment its title names one.
    async fn learn_tagged_conversation(
        context: &Context,
        record: &mut Session,
        native: &adroit_term::Session,
    ) -> Result<()> {
        let Some(conversation) = crate::sessions::conversation_of(native) else {
            return Ok(());
        };
        if record.conversation.as_deref() == Some(conversation.as_str()) {
            return Ok(());
        }
        let (id, learned) = (record.id, conversation.clone());
        context
            .call(move |store| store.learn_conversation(id, &learned))
            .await?;
        record.conversation = Some(conversation);
        Ok(())
    }

    /// Records each terminal the terminal host no longer holds as ended, or
    /// as interrupted when the terminal host it ran in went; whether any was.
    async fn terminals(
        &self,
        context: &Context,
        native: &[adroit_term::Session],
        now: Option<&str>,
    ) -> Result<bool> {
        let kept = context.call(|store| store.terminals()).await?;
        let mut interrupted = false;
        for kept in kept {
            let id = kept.terminal.id;
            let listed = native.iter().find(|n| n.id == id).map(|n| n.exited);
            let told = crate::restore::fate(listed, kept.generation.as_deref(), now);
            match judged(context, told).await {
                Fate::Interrupted => {
                    context
                        .call(move |store| store.interrupt_terminal(id))
                        .await?;
                    interrupted = true;
                }
                Fate::Ended => context.call(move |store| store.end_terminal(id)).await?,
                Fate::Runs | Fate::Unknown => {}
            }
        }
        Ok(interrupted)
    }

    /// Confirms the warning about Adroit's channel when the session's launch
    /// allows it, and records how the session stands while it starts.
    async fn starting(
        &mut self,
        context: &Context,
        record: &Session,
        native: &adroit_term::Session,
        screen: &[String],
    ) -> Result<()> {
        let answered = tags::value(&native.tags, CHANNEL_CONFIRMED).is_some();
        let (id, profile, home) = (record.id, record.profile.clone(), record.home.clone());
        let allowed = context
            .call(move |store| store.launch(id))
            .await?
            .and_then(|launch| crate::programs::recorded(&launch, &profile, home.as_deref()).ok())
            .is_some_and(|started| started.claude().confirm_channel_warning);
        if let Some(keys) = channel_warning_keys(record.agent, allowed, answered, screen) {
            // Tagged before pressing: a lost reply must not lead to a second press.
            context
                .daemon
                .tag(native.id, tags::tagged(CHANNEL_CONFIRMED, "1"), Vec::new())
                .await
                .map_err(Error::storage)?;
            context
                .daemon
                .press(native.id, keys)
                .await
                .map_err(Error::storage)?;
            return Ok(());
        }
        let seen = adroit_agents::adapter(record.agent).startup_readiness(screen);
        let elapsed =
            Duration::from_millis(crate::delegation::now_ms().saturating_sub(record.started_ms));
        if let Some(readiness) = readiness(seen, elapsed) {
            context
                .call(move |store| store.set_readiness(id, readiness))
                .await?;
        }
        Ok(())
    }

    async fn follow_conversation(
        &mut self,
        context: &Context,
        record: &mut Session,
        native: &adroit_term::Session,
    ) -> Result<bool> {
        let adapter = adroit_agents::adapter(record.agent);
        let Some(title) = native.title.as_deref() else {
            return Ok(false);
        };
        if record
            .conversation
            .as_deref()
            .is_some_and(|known| adapter.conversation_matches_title(title, known))
        {
            self.resolved.remove(&record.id);
            return Ok(true);
        }
        let id = record.id;
        let resolved = self
            .resolved
            .remove(&id)
            .filter(|(asked, _)| asked == title)
            .map(|(_, conversation)| conversation);
        if let Some(conversation) = adapter.conversation_title(title).or(resolved) {
            let learned = conversation.clone();
            context
                .call(move |store| store.learn_conversation(id, &learned))
                .await?;
            record.conversation = Some(conversation);
            return Ok(true);
        }
        if adapter.conversation_title_needs_lookup(title) && self.looking.insert(id) {
            let (context, title, cwd) = (context.clone(), title.to_owned(), record.cwd.clone());
            let (profile, home) = (record.profile.clone(), record.home.clone());
            self.lookups.spawn(async move {
                let Ok(Some(launch)) = context.call(move |store| store.launch(id)).await else {
                    return (id, title, None);
                };
                let Ok(started) = crate::programs::recorded(&launch, &profile, home.as_deref())
                else {
                    return (id, title, None);
                };
                let conversation = adapter
                    .resolve_conversation(started, cwd, title.clone())
                    .await;
                (id, title, conversation)
            });
        }
        Ok(false)
    }

    /// Hands a Codex session its oldest waiting message through Codex's own queue.
    async fn deliver(&mut self, context: &Context, record: &Session) -> Result<()> {
        let Some(conversation) = record.conversation.clone() else {
            return Ok(());
        };
        if self.delivering.contains(&record.id) {
            return Ok(());
        }
        let id = record.id;
        let on = context.takes_requests();
        let next = context
            .call(move |store| next_delivery(store, id, on))
            .await?;
        let Some((message, sender, started)) = next else {
            return Ok(());
        };
        self.delivering.insert(id);
        let text = envelope(&message, sender.as_ref());
        let (context, cwd) = (context.clone(), record.cwd.clone());
        self.deliveries.spawn(async move {
            let handed = crate::execution::deliver(started, cwd, conversation, text).await;
            if handed.is_ok() {
                let message = message.id;
                let _ = context.call(move |store| store.delivered(message)).await;
            }
            id
        });
        Ok(())
    }
}

/// The oldest message waiting for `id`, its sender, and the context it is
/// handed over in: that of the profile `id` was recorded as started from,
/// in the login it was recorded as running in, as the settings it was
/// started with say, whatever they say now, as another login's queue does
/// not know the session.
fn next_delivery(
    store: &mut Store,
    id: SessionId,
    requests: bool,
) -> Result<Option<(Message, Option<Session>, adroit_agents::AgentContext)>> {
    let Some((message, sender)) = next_message(store, id, requests)? else {
        return Ok(None);
    };
    let (Some(launch), Some(recipient)) = (store.launch(id)?, store.session(id)?) else {
        return Ok(None);
    };
    let started = crate::programs::recorded(&launch, &recipient.profile, recipient.home.as_deref())
        .map_err(Error::storage)?;
    Ok(Some((message, sender, started)))
}

/// How a starting session stands, from what its adapter read on its screen
/// and how long it has been starting; `None` while that tells nothing yet.
fn readiness(seen: Option<StartupReadiness>, elapsed: Duration) -> Option<Readiness> {
    match seen {
        Some(StartupReadiness::Ready) => Some(Readiness::Ready),
        Some(
            StartupReadiness::AwaitingTrust
            | StartupReadiness::AwaitingPermission
            | StartupReadiness::AwaitingInput,
        ) => Some(Readiness::Waiting),
        Some(StartupReadiness::Stalled) => Some(Readiness::Stalled),
        Some(StartupReadiness::Spawned) | None => (elapsed >= STALL).then_some(Readiness::Stalled),
    }
}

/// The keys that confirm Claude Code's warning about loading development
/// channels on `screen`, when `allowed`, not yet `answered` for this launch,
/// and the warning names Adroit's own channel and nothing else.
fn channel_warning_keys(
    agent: Agent,
    allowed: bool,
    answered: bool,
    screen: &[String],
) -> Option<&'static [u8]> {
    let ours = || {
        adroit_agents::claude::dialog::development_channels(screen)
            .is_some_and(|channels| channels == [CLAUDE_CHANNEL])
    };
    (agent == Agent::Claude && allowed && !answered && ours()).then_some(b"\r")
}

#[cfg(test)]
mod tests;
