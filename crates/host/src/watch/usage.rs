//! How much of the account's usage limits is left, for the foot of the
//! details panel and the agents an environment's details list: asked of
//! each account shown while the panel is open, again once the answer has
//! aged.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use adroit_agents::claude::usage::Fresh;
use adroit_model::Agent;
use adroit_model::profile::AgentProfile;
use adroit_protocol::{Account, DetailsChange, Sight};

use super::{Watched, distinct};

pub(super) async fn watch(mut watched: Watched) {
    // By the login, which is the account the limits are of, so a login both
    // a session and the environment's agents show is asked once.
    let mut fresh = HashMap::<Account, Fresh>::new();
    loop {
        let shown = wanted(
            &watched.sights(),
            &crate::settings::current().profiles,
            |agent| adroit_agents::adapter(agent).capabilities().usage_limits,
        );
        for account in shown {
            let held = fresh.entry(account.clone()).or_default();
            if held.want(Instant::now()) {
                let got =
                    match super::asked_in(&watched.seen(), account.agent, account.home.as_deref())
                        .await
                    {
                        Some(context) => {
                            adroit_agents::adapter(account.agent)
                                .usage(context, crate::paths::home())
                                .await
                        }
                        None => None,
                    };
                held.settle(Instant::now(), got);
            }
            if let Some(limits) = held.get() {
                watched.record([DetailsChange::Usage {
                    account,
                    limits: limits.to_vec(),
                }]);
            }
        }
        watched
            .wait(Duration::from_secs(30), |seen| {
                seen.iter()
                    .map(|s| (s.sight.details, s.sight.agents, s.sight.usage.clone()))
                    .collect::<Vec<_>>()
            })
            .await;
    }
}

/// The accounts whose usage `sights` want read, each once: the one an open
/// details panel shows, and, while a window lists the environment's agents,
/// the login of each of `profiles` whose agent is `limited`, having usage
/// limits to tell.
fn wanted(
    sights: &[Sight],
    profiles: &[AgentProfile],
    limited: impl Fn(Agent) -> bool,
) -> Vec<Account> {
    let mut found = distinct(sights, |s| s.usage.clone().filter(|_| s.details));
    if sights.iter().any(|s| s.agents) {
        let logins = profiles
            .iter()
            .filter(|p| limited(p.agent()))
            .map(|p| Account {
                agent: p.agent(),
                home: p.home.clone(),
            });
        for account in logins {
            if !found.contains(&account) {
                found.push(account);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn profile(id: &str, agent: Agent, home: Option<&str>) -> AgentProfile {
        AgentProfile {
            id: id.parse().unwrap(),
            home: home.map(PathBuf::from),
            ..AgentProfile::builtin(agent)
        }
    }

    fn account(agent: Agent, home: Option<&str>) -> Account {
        Account {
            agent,
            home: home.map(PathBuf::from),
        }
    }

    fn profiles() -> Vec<AgentProfile> {
        vec![
            profile("claude", Agent::Claude, None),
            profile("claude-work", Agent::Claude, Some("/p/claude-work")),
            profile("claude-same", Agent::Claude, Some("/p/claude-work")),
            profile("pi", Agent::Pi, None),
        ]
    }

    fn limited(agent: Agent) -> bool {
        agent == Agent::Claude
    }

    #[test]
    fn only_the_account_an_open_panel_shows_is_read_while_no_window_lists_the_agents() {
        let shown = Sight {
            details: true,
            usage: Some(account(Agent::Codex, None)),
            ..Sight::default()
        };
        let closed = Sight {
            details: false,
            usage: Some(account(Agent::Claude, None)),
            ..Sight::default()
        };
        assert_eq!(
            wanted(&[shown, closed], &profiles(), limited),
            [account(Agent::Codex, None)]
        );
    }

    #[test]
    fn listing_the_agents_reads_each_login_of_a_profile_whose_agent_has_limits_once() {
        let listing = Sight {
            details: true,
            agents: true,
            usage: Some(account(Agent::Claude, None)),
            ..Sight::default()
        };
        assert_eq!(
            wanted(&[listing], &profiles(), limited),
            [
                account(Agent::Claude, None),
                account(Agent::Claude, Some("/p/claude-work")),
            ]
        );
    }

    #[test]
    fn nothing_is_read_for_a_window_that_shows_no_usage_and_lists_no_agents() {
        assert!(wanted(&[Sight::default()], &profiles(), limited).is_empty());
    }
}
