//! What an environment's details say of the agents its profiles start.

use std::collections::BTreeMap;

use adroit_model::Agent;
use adroit_model::profile::ProfileId;
use adroit_model::usage::Limit;
use adroit_protocol::{Account, AgentsWork};

use super::{Gauge, gauges};
use crate::settings::Settings;

/// One profile of an environment, as its details list it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileRow {
    pub id: ProfileId,
    pub name: String,
    pub agent: Agent,
    /// The profile a new session starts from when it names none.
    pub default: bool,
    /// Whether its agent's account has usage limits to tell; one without
    /// says only how its login stands.
    pub reports_usage: bool,
    /// What is left of its login's limits, as last told.
    pub gauges: Vec<Gauge>,
    /// The page where its agent's account shows its usage.
    pub usage_url: Option<String>,
}

/// Each of `settings`' profiles, in the order they list them, with what
/// `agents` tell of their agents and the limits `usage` holds for their
/// logins, at `now_ms` since the Unix epoch.
pub fn profile_rows(
    settings: &Settings,
    agents: &AgentsWork,
    usage: &BTreeMap<Account, Vec<Limit>>,
    now_ms: u64,
) -> Vec<ProfileRow> {
    settings
        .profiles
        .iter()
        .map(|p| {
            let agent = p.agent();
            let facts = agents.of(agent);
            let reports_usage = facts.is_some_and(|f| f.usage_limits);
            let account = Account {
                agent,
                home: p.home.clone(),
            };
            ProfileRow {
                id: p.id.clone(),
                name: p.label().to_string(),
                agent,
                default: p.id == settings.profile,
                reports_usage,
                gauges: usage
                    .get(&account)
                    .filter(|_| reports_usage)
                    .map(|limits| gauges(limits, now_ms))
                    .unwrap_or_default(),
                usage_url: facts.and_then(|f| f.usage_url.clone()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use adroit_model::profile::AgentProfile;
    use adroit_model::usage::{Severity, Window};
    use adroit_protocol::AgentFacts;

    use super::*;

    fn profile(id: &str, name: &str, agent: Agent, home: Option<&str>) -> AgentProfile {
        AgentProfile {
            id: id.parse().unwrap(),
            name: name.into(),
            home: home.map(PathBuf::from),
            ..AgentProfile::builtin(agent)
        }
    }

    fn settings() -> Settings {
        Settings {
            profile: "claude-work".parse().unwrap(),
            profiles: vec![
                profile("claude", "Claude Personal", Agent::Claude, None),
                profile("claude-work", "Claude Work", Agent::Claude, Some("/p/w")),
                profile("pi", "", Agent::Pi, None),
            ],
            ..Settings::default()
        }
    }

    fn agents() -> AgentsWork {
        let facts = |agent, usage_limits, usage_url: Option<&str>| AgentFacts {
            agent,
            program: agent.key().into(),
            usage_url: usage_url.map(String::from),
            usage_limits,
        };
        AgentsWork {
            agents: vec![
                facts(
                    Agent::Claude,
                    true,
                    Some("https://claude.ai/settings/usage"),
                ),
                facts(Agent::Pi, false, None),
            ],
        }
    }

    fn week(percent: f64) -> Limit {
        Limit {
            window: Window::Week,
            percent,
            resets_ms: None,
            severity: Severity::Normal,
        }
    }

    #[test]
    fn every_profile_is_listed_in_order_with_the_default_marked() {
        let rows = profile_rows(&settings(), &agents(), &BTreeMap::new(), 0);
        let listed: Vec<_> = rows
            .iter()
            .map(|r| (r.id.to_string(), r.name.as_str(), r.agent, r.default))
            .collect();
        assert_eq!(
            listed,
            [
                ("claude".into(), "Claude Personal", Agent::Claude, false),
                ("claude-work".into(), "Claude Work", Agent::Claude, true),
                ("pi".into(), "pi", Agent::Pi, false),
            ]
        );
    }

    #[test]
    fn each_profile_shows_the_limits_of_its_own_login() {
        let work = Account {
            agent: Agent::Claude,
            home: Some("/p/w".into()),
        };
        let usage = BTreeMap::from([(work, vec![week(30.0)])]);
        let rows = profile_rows(&settings(), &agents(), &usage, 0);
        assert!(rows[0].gauges.is_empty());
        assert_eq!(rows[1].gauges, gauges(&[week(30.0)], 0));
        assert_eq!(rows[1].gauges[0].left, "70% left");
    }

    #[test]
    fn a_profile_whose_agent_has_no_limits_reports_no_usage_and_each_links_its_agent_s_page() {
        let pi = Account {
            agent: Agent::Pi,
            home: None,
        };
        let usage = BTreeMap::from([(pi, vec![week(10.0)])]);
        let rows = profile_rows(&settings(), &agents(), &usage, 0);
        let pi = &rows[2];
        assert!(!pi.reports_usage);
        assert!(pi.gauges.is_empty());
        assert_eq!(pi.usage_url, None);
        assert!(rows[0].reports_usage);
        assert_eq!(
            rows[0].usage_url.as_deref(),
            Some("https://claude.ai/settings/usage")
        );
    }

    #[test]
    fn nothing_is_known_to_report_usage_before_the_host_tells_of_its_agents() {
        let rows = profile_rows(&settings(), &AgentsWork::default(), &BTreeMap::new(), 0);
        assert!(
            rows.iter()
                .all(|r| !r.reports_usage && r.usage_url.is_none())
        );
    }
}
