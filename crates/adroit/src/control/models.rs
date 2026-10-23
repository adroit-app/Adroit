//! Keeps the store's model lists and default agent following the providers
//! and the settings.
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use adroit_model::{Agent, ProviderModel};
use tokio::time::Instant;

use super::actor::Actor;
use crate::settings::Settings;

pub type ModelProbe = Pin<Box<dyn Future<Output = Option<Vec<ProviderModel>>> + Send>>;

/// How often the watch runs, and how long a provider's answer, or its
/// failure to answer, stands before it is asked again.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub tick: Duration,
    /// Models change with releases and accounts, rarely within a session.
    pub answered: Duration,
    /// Shorter, so a provider that was still starting up is soon checked.
    pub failed: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            tick: Duration::from_secs(1),
            answered: Duration::from_secs(600),
            failed: Duration::from_secs(60),
        }
    }
}

/// Ask every agent's provider through `probe`, in the home directory, and
/// record what it answers and the default agent of `settings` in the store.
pub async fn watch(
    actor: Actor,
    settings: impl Fn() -> Result<Settings, String>,
    probe: impl Fn(Agent, Settings, PathBuf) -> ModelProbe,
    timing: Timing,
) {
    let mut interval = tokio::time::interval(timing.tick);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut next: HashMap<Agent, Instant> = HashMap::new();
    let mut asking = tokio::task::JoinSet::<(Agent, Option<Vec<ProviderModel>>)>::new();
    let mut default = None;
    loop {
        tokio::select! {
            _ = interval.tick() => {}
            Some(Ok((agent, got))) = asking.join_next(), if !asking.is_empty() => {
                let wait = if got.is_some() { timing.answered } else { timing.failed };
                next.insert(agent, Instant::now() + wait);
                if let Some(models) = got {
                    let _ = actor
                        .call(move |store| {
                            store.learn_models(agent, models);
                            Ok(())
                        })
                        .await;
                }
                continue;
            }
        }
        let Ok(settings) = settings() else {
            continue;
        };
        let settings = settings.sane();
        if default != Some(settings.agent) {
            let agent = settings.agent;
            if actor
                .call(move |store| {
                    store.set_default_agent(agent);
                    Ok(())
                })
                .await
                .is_ok()
            {
                default = Some(agent);
            }
        }
        let now = Instant::now();
        for agent in Agent::ALL {
            if next.get(&agent).is_none_or(|at| now >= *at) {
                // Far ahead while asking, so a slow answer is not asked twice.
                next.insert(agent, now + timing.answered);
                let asked = probe(agent, settings.clone(), crate::paths::home());
                asking.spawn(async move { (agent, asked.await) });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn model(id: &str) -> ProviderModel {
        ProviderModel {
            id: id.into(),
            resolved: None,
            name: id.into(),
            description: String::new(),
        }
    }

    fn timing() -> Timing {
        Timing {
            tick: Duration::from_millis(10),
            answered: Duration::from_secs(600),
            failed: Duration::from_millis(50),
        }
    }

    async fn actor() -> (tempfile::TempDir, Actor) {
        let dir = tempfile::tempdir().unwrap();
        let actor = Actor::open(dir.path().join("control/state.sqlite3"))
            .await
            .unwrap();
        (dir, actor)
    }

    async fn until(actor: &Actor, done: impl Fn(&[adroit_model::AgentModels]) -> bool) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let view = actor.call(|store| Ok(store.models())).await.unwrap();
                if done(&view) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the watch to record the models");
    }

    #[tokio::test]
    async fn every_provider_s_answer_and_the_default_agent_reach_the_store() {
        let (_dir, actor) = actor().await;
        let watched = tokio::spawn(watch(
            actor.clone(),
            || {
                Ok(Settings {
                    agent: Agent::Codex,
                    ..Settings::default()
                })
            },
            |agent, _, _| Box::pin(async move { Some(vec![model(agent.key())]) }) as ModelProbe,
            timing(),
        ));

        until(&actor, |view| view.iter().all(|a| a.models.is_some())).await;
        let view = actor.call(|store| Ok(store.models())).await.unwrap();
        watched.abort();

        assert_eq!(view[0].models, Some(vec![model("claude")]));
        assert_eq!(view[1].models, Some(vec![model("codex")]));
        assert!(!view[0].default && view[1].default);
    }

    #[tokio::test]
    async fn a_provider_that_fails_is_asked_again_and_one_that_answered_is_not() {
        let (_dir, actor) = actor().await;
        let asked = Arc::new(Mutex::new(HashMap::<Agent, usize>::new()));
        let watched = tokio::spawn(watch(
            actor.clone(),
            || Ok(Settings::default()),
            {
                let asked = asked.clone();
                move |agent, _, _| {
                    let n = {
                        let mut asked = asked.lock().unwrap();
                        let n = asked.entry(agent).or_default();
                        *n += 1;
                        *n
                    };
                    Box::pin(async move {
                        (agent == Agent::Claude || n > 2).then(|| vec![model(agent.key())])
                    }) as ModelProbe
                }
            },
            timing(),
        ));

        until(&actor, |view| view[1].models.is_some()).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        watched.abort();

        let asked = asked.lock().unwrap();
        assert_eq!(asked[&Agent::Claude], 1);
        assert_eq!(asked[&Agent::Codex], 3);
    }

    #[tokio::test]
    async fn unreadable_settings_ask_nothing() {
        let (_dir, actor) = actor().await;
        let asked = Arc::new(Mutex::new(0));
        let watched = tokio::spawn(watch(
            actor.clone(),
            || Err("Invalid configuration".into()),
            {
                let asked = asked.clone();
                move |_, _, _| {
                    *asked.lock().unwrap() += 1;
                    Box::pin(async { None }) as ModelProbe
                }
            },
            timing(),
        ));

        tokio::time::sleep(Duration::from_millis(100)).await;
        watched.abort();

        assert_eq!(*asked.lock().unwrap(), 0);
    }
}
