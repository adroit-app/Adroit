//! What the host tells of the agents it runs, which the window shows them
//! by: the same for as long as the host runs.

use adroit_model::Agent;
use serde::{Deserialize, Serialize};

/// What the host knows of one agent beyond its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFacts {
    pub agent: Agent,
    /// The program a session of the agent runs, unless the settings name
    /// another.
    pub program: String,
    /// The page where the agent's account shows its usage.
    pub usage_url: Option<String>,
    /// The agent's account has usage limits, which the details panel shows.
    pub usage_limits: bool,
}

/// What the host tells of every agent it runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentsWork {
    pub agents: Vec<AgentFacts>,
}

impl AgentsWork {
    /// What the host told of `agent`; none before it told anything.
    pub fn of(&self, agent: Agent) -> Option<&AgentFacts> {
        self.agents.iter().find(|facts| facts.agent == agent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agent_is_found_by_its_facts_and_none_is_told_of_before_the_host_says() {
        let codex = AgentFacts {
            agent: Agent::Codex,
            program: "codex".into(),
            usage_url: None,
            usage_limits: true,
        };
        let work = AgentsWork {
            agents: vec![codex.clone()],
        };
        assert_eq!(work.of(Agent::Codex), Some(&codex));
        assert_eq!(work.of(Agent::Claude), None);
        assert_eq!(AgentsWork::default().of(Agent::Codex), None);
    }
}
