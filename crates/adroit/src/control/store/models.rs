//! The providers' model lists, kept in memory: they are asked for again
//! whenever the daemon starts, so nothing is gained by recording them.
use super::*;

impl Store {
    pub fn learn_models(&mut self, agent: Agent, models: Vec<ProviderModel>) {
        self.models.learn(agent, models);
    }

    pub fn set_default_agent(&mut self, agent: Agent) {
        self.models.set_default_agent(agent);
    }

    pub fn models(&self) -> Vec<AgentModels> {
        self.models.view()
    }

    pub fn check_model(&self, agent: Option<Agent>, model: Option<&str>) -> Result<()> {
        self.models.check(agent, model).map_err(invalid)
    }
}
