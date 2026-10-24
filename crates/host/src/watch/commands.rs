//! The commands `/` offers in the prompt: the agent's own and those it was
//! found to offer where the prompt's session runs, asked of the agent when
//! that comes into sight, or a `/` typed asks, and what was found is old.
//! What was found before is offered meanwhile.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use adroit_agents::claude::commands::Cache;
use adroit_model::profile::ConnectionId;
use adroit_model::{Agent, Command};
use adroit_protocol::{CommandsRead, CommandsSight, PromptChange};

use super::{Watched, distinct};

/// What was found, by the agent and the connection it was asked in.
type Caches = Arc<Mutex<HashMap<(Agent, ConnectionId), Cache>>>;

pub(super) async fn watch(mut watched: Watched) {
    let caches = Caches::default();
    loop {
        for sight in distinct(&watched.sights(), |s| s.commands.clone()) {
            watched.record([offered(&caches, &sight)]);
            ask(&watched, &caches, sight);
        }
        watched
            .wait_commands(|seen| {
                seen.iter()
                    .map(|s| s.sight.commands.clone())
                    .collect::<Vec<_>>()
            })
            .await;
    }
}

/// What `/` offers for `sight` now: nothing for an agent that cannot be
/// asked.
fn offered(caches: &Caches, sight: &CommandsSight) -> PromptChange {
    let adapter = adroit_agents::adapter(sight.agent);
    let commands = if adapter.capabilities().command_probe {
        let found = lock(caches)
            .get(&(sight.agent, sight.connection.clone()))
            .map(|cache| cache.get(&sight.dir).to_vec())
            .unwrap_or_default();
        offer(adapter.builtin_commands(), found)
    } else {
        Vec::new()
    };
    PromptChange::Commands {
        read: CommandsRead {
            agent: sight.agent,
            connection: sight.connection.clone(),
            dir: sight.dir.clone(),
            commands,
        },
    }
}

/// The agent's own commands, then those found where it runs.
fn offer(own: Vec<Command>, found: Vec<Command>) -> Vec<Command> {
    own.into_iter().chain(found).collect()
}

/// Ask the agent in the background, if what was found for `sight` is old
/// and it is not being asked already; what it answers is offered.
fn ask(watched: &Watched, caches: &Caches, sight: CommandsSight) {
    let adapter = adroit_agents::adapter(sight.agent);
    if !adapter.capabilities().command_probe {
        return;
    }
    let key = (sight.agent, sight.connection.clone());
    let wanted = lock(caches)
        .entry(key.clone())
        .or_default()
        .want(&sight.dir, Instant::now());
    if !wanted {
        return;
    }
    let (watched, caches) = (watched.clone(), caches.clone());
    tokio::spawn(async move {
        let got = match super::asked_in(&watched.seen(), sight.agent, &sight.connection).await {
            Some(context) => adapter.commands(context, sight.dir.clone()).await,
            None => None,
        };
        lock(&caches)
            .entry(key)
            .or_default()
            .settle(&sight.dir, Instant::now(), got);
        watched.record([offered(&caches, &sight)]);
    });
}

fn lock(caches: &Caches) -> MutexGuard<'_, HashMap<(Agent, ConnectionId), Cache>> {
    caches
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(name: &str) -> Command {
        Command {
            name: name.into(),
            description: String::new(),
            argument_hint: String::new(),
        }
    }

    #[test]
    fn the_agent_s_own_commands_come_before_those_found_where_it_runs() {
        let got = offer(vec![command("model")], vec![command("deploy")]);
        assert_eq!(got, [command("model"), command("deploy")]);
    }

    #[test]
    fn before_anything_is_found_the_agent_s_own_commands_are_offered() {
        let sight = CommandsSight {
            agent: Agent::Codex,
            connection: "codex".parse().unwrap(),
            dir: "/w".into(),
        };
        let PromptChange::Commands { read } = offered(&Caches::default(), &sight) else {
            panic!("commands offered");
        };
        assert!(read.is_for(&sight));
        assert_eq!(
            read.commands,
            adroit_agents::adapter(Agent::Codex).builtin_commands()
        );
    }
}
