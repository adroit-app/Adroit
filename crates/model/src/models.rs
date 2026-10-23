//! The models each provider says it accepts, and whether a model chosen for a
//! managed session is one of them.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::Agent;

/// One model a provider lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProviderModel {
    /// What the provider takes as its model option.
    pub id: String,
    /// The full model `id` stands for, when the provider names one apart from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// One agent's entry in [`ModelCatalog::view`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentModels {
    pub agent: Agent,
    /// Whether a task that names no agent runs this one.
    pub default: bool,
    /// `None` until the provider has answered.
    pub models: Option<Vec<ProviderModel>>,
}

/// The last list each provider gave, and the agent a task without one runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelCatalog {
    default_agent: Agent,
    lists: HashMap<Agent, Vec<ProviderModel>>,
}

impl ModelCatalog {
    pub fn set_default_agent(&mut self, agent: Agent) {
        self.default_agent = agent;
    }

    pub fn default_agent(&self) -> Agent {
        self.default_agent
    }

    /// Replace `agent`'s list. An empty answer tells nothing about what the
    /// provider accepts, so it leaves the list there was.
    pub fn learn(&mut self, agent: Agent, models: Vec<ProviderModel>) {
        if !models.is_empty() {
            self.lists.insert(agent, models);
        }
    }

    pub fn view(&self) -> Vec<AgentModels> {
        Agent::ALL
            .into_iter()
            .map(|agent| AgentModels {
                agent,
                default: agent == self.default_agent,
                models: self.lists.get(&agent).cloned(),
            })
            .collect()
    }

    /// Whether `model`, chosen for `agent` (the default agent when `None`), is
    /// one its provider lists. A bracketed suffix, as in `opus[1m]`, selects a
    /// variant of the listed model before it.
    pub fn check(&self, agent: Option<Agent>, model: Option<&str>) -> Result<(), String> {
        let agent = agent.unwrap_or(self.default_agent);
        let (Some(model), Some(list)) = (model, self.lists.get(&agent)) else {
            return Ok(());
        };
        let model = model.trim();
        let base = match model.strip_suffix(']').and_then(|m| m.rsplit_once('[')) {
            Some((base, _)) if !base.is_empty() => base,
            _ => model,
        };
        if list
            .iter()
            .any(|m| m.id == base || m.resolved.as_deref() == Some(base))
        {
            return Ok(());
        }
        let listed = list
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Err(format!(
            "{} does not list the model `{model}`; choose one of: {listed}",
            agent.key()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str, resolved: Option<&str>) -> ProviderModel {
        ProviderModel {
            id: id.into(),
            resolved: resolved.map(Into::into),
            name: id.into(),
            description: String::new(),
        }
    }

    fn catalog() -> ModelCatalog {
        let mut catalog = ModelCatalog::default();
        catalog.learn(
            Agent::Claude,
            vec![
                model("opus", Some("claude-opus-5-5")),
                model("claude-sonnet-4-6", None),
            ],
        );
        catalog.learn(Agent::Codex, vec![model("gpt-6-sol", None)]);
        catalog
    }

    #[test]
    fn a_listed_model_its_resolved_name_or_no_model_at_all_is_accepted() {
        let catalog = catalog();
        assert_eq!(catalog.check(Some(Agent::Claude), Some("opus")), Ok(()));
        assert_eq!(
            catalog.check(Some(Agent::Claude), Some("claude-opus-5-5")),
            Ok(())
        );
        assert_eq!(catalog.check(Some(Agent::Claude), None), Ok(()));
    }

    #[test]
    fn a_model_outside_the_list_is_refused_naming_the_listed_ones() {
        let error = catalog()
            .check(Some(Agent::Claude), Some("opsu"))
            .unwrap_err();
        assert!(error.contains("`opsu`"), "{error}");
        assert!(error.contains("opus, claude-sonnet-4-6"), "{error}");
    }

    #[test]
    fn each_agent_is_checked_against_its_own_list() {
        let catalog = catalog();
        assert!(catalog.check(Some(Agent::Codex), Some("opus")).is_err());
        assert_eq!(catalog.check(Some(Agent::Codex), Some("gpt-6-sol")), Ok(()));
    }

    #[test]
    fn a_task_without_an_agent_is_checked_against_the_default_agent() {
        let mut catalog = catalog();
        assert_eq!(catalog.check(None, Some("opus")), Ok(()));
        catalog.set_default_agent(Agent::Codex);
        assert!(catalog.check(None, Some("opus")).is_err());
        assert_eq!(catalog.check(None, Some("gpt-6-sol")), Ok(()));
    }

    #[test]
    fn a_bracketed_variant_of_a_listed_model_is_accepted() {
        let catalog = catalog();
        assert_eq!(catalog.check(Some(Agent::Claude), Some("opus[1m]")), Ok(()));
        assert_eq!(
            catalog.check(Some(Agent::Claude), Some("claude-sonnet-4-6[1m]")),
            Ok(())
        );
        assert!(
            catalog
                .check(Some(Agent::Claude), Some("opsu[1m]"))
                .is_err()
        );
        assert!(catalog.check(Some(Agent::Claude), Some("[1m]")).is_err());
    }

    #[test]
    fn surrounding_whitespace_is_not_part_of_the_model() {
        assert_eq!(catalog().check(Some(Agent::Claude), Some(" opus ")), Ok(()));
    }

    #[test]
    fn an_agent_whose_provider_has_not_answered_is_not_checked() {
        let mut catalog = ModelCatalog::default();
        assert_eq!(catalog.check(Some(Agent::Claude), Some("anything")), Ok(()));
        catalog.learn(Agent::Claude, Vec::new());
        assert_eq!(catalog.check(Some(Agent::Claude), Some("anything")), Ok(()));
    }

    #[test]
    fn an_empty_answer_keeps_the_list_there_was() {
        let mut catalog = catalog();
        catalog.learn(Agent::Claude, Vec::new());
        assert!(catalog.check(Some(Agent::Claude), Some("opsu")).is_err());
    }

    #[test]
    fn the_view_shows_every_agent_with_its_list_and_the_default() {
        let mut catalog = ModelCatalog::default();
        catalog.learn(Agent::Codex, vec![model("gpt-6-sol", None)]);
        catalog.set_default_agent(Agent::Codex);
        assert_eq!(
            catalog.view(),
            [
                AgentModels {
                    agent: Agent::Claude,
                    default: false,
                    models: None
                },
                AgentModels {
                    agent: Agent::Codex,
                    default: true,
                    models: Some(vec![model("gpt-6-sol", None)])
                },
            ]
        );
    }
}
