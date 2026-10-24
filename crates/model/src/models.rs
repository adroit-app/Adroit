//! The models each profile's provider says it accepts, and whether a model
//! chosen for a managed session is one of them.

use serde::{Deserialize, Serialize};

use crate::Agent;
use crate::profile::{ConnectionId, ProfileId};

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

/// One profile's entry in [`ModelCatalog::view`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProfileModels {
    pub profile: ProfileId,
    pub agent: Agent,
    /// The connection it runs in, whose login the list is of.
    pub connection: ConnectionId,
    /// Whether a task that names neither a profile nor an agent starts from it.
    pub default: bool,
    /// `None` until the provider has answered.
    pub models: Option<Vec<ProviderModel>>,
}

/// A profile as the catalog asks of it: what it is, and what its provider
/// is asked through, which, when it changes, makes the list asked before
/// another's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub profile: ProfileId,
    pub agent: Agent,
    pub connection: ConnectionId,
    /// Everything that decides what the provider answers: the connection's
    /// state directory and how the program is started.
    pub through: String,
}

/// One profile's place in the catalog: what it was last configured as, the
/// generation that configuration is, and the list its provider gave for it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    asked: Asked,
    generation: u64,
    models: Option<Vec<ProviderModel>>,
}

/// The last list each profile's provider gave, and the profile a task that
/// names none starts from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelCatalog {
    default: Option<ProfileId>,
    entries: Vec<Entry>,
    /// The last generation given out, so no two configurations share one.
    generations: u64,
}

impl ModelCatalog {
    /// Takes in the profiles the settings hold now, `default` being the one a
    /// task that names none starts from. A profile asked through anything
    /// other than before is a new generation with no list yet; the others
    /// keep theirs; one no longer held is forgotten.
    pub fn configure(&mut self, profiles: Vec<Asked>, default: ProfileId) {
        let mut entries = Vec::with_capacity(profiles.len());
        for asked in profiles {
            let kept = self
                .entries
                .iter()
                .position(|e| e.asked == asked)
                .map(|at| self.entries.swap_remove(at));
            entries.push(match kept {
                Some(entry) => entry,
                None => {
                    self.generations += 1;
                    Entry {
                        asked,
                        generation: self.generations,
                        models: None,
                    }
                }
            });
        }
        self.entries = entries;
        self.default = Some(default);
    }

    /// Each profile to ask, with the generation its answer is to be learned
    /// as.
    pub fn to_ask(&self) -> Vec<(Asked, u64)> {
        self.entries
            .iter()
            .map(|e| (e.asked.clone(), e.generation))
            .collect()
    }

    /// Replace the list of the profile asked as `generation`. An answer for
    /// a generation since replaced came from an earlier configuration and
    /// is dropped; an empty answer tells nothing about what the provider
    /// accepts, so it leaves the list there was.
    pub fn learn(&mut self, generation: u64, models: Vec<ProviderModel>) {
        if models.is_empty() {
            return;
        }
        if let Some(entry) = self.entries.iter_mut().find(|e| e.generation == generation) {
            entry.models = Some(models);
        }
    }

    pub fn view(&self) -> Vec<ProfileModels> {
        self.entries
            .iter()
            .map(|e| ProfileModels {
                profile: e.asked.profile.clone(),
                agent: e.asked.agent,
                connection: e.asked.connection.clone(),
                default: self.default.as_ref() == Some(&e.asked.profile),
                models: e.models.clone(),
            })
            .collect()
    }

    /// Whether `model`, chosen for a session of `profile`, is one its
    /// provider lists. A bracketed suffix, as in `opus[1m]`, selects a
    /// variant of the listed model before it.
    pub fn check(&self, profile: &ProfileId, model: Option<&str>) -> Result<(), String> {
        let entry = self.entries.iter().find(|e| &e.asked.profile == profile);
        let (Some(model), Some(list)) = (model, entry.and_then(|e| e.models.as_ref())) else {
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
            "profile `{profile}` does not list the model `{model}`; choose one of: {listed}"
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

    fn id(text: &str) -> ProfileId {
        text.parse().unwrap()
    }

    fn asked(profile: &str, agent: Agent, connection: &str, through: &str) -> Asked {
        Asked {
            profile: id(profile),
            agent,
            connection: connection.parse().unwrap(),
            through: through.into(),
        }
    }

    /// The generation `profile` is asked as now.
    fn generation(catalog: &ModelCatalog, profile: &str) -> u64 {
        catalog
            .to_ask()
            .into_iter()
            .find(|(a, _)| a.profile == id(profile))
            .unwrap()
            .1
    }

    fn profiles() -> Vec<Asked> {
        vec![
            asked("claude", Agent::Claude, "claude", "own"),
            asked("claude-work", Agent::Claude, "work", "/w"),
            asked("codex", Agent::Codex, "codex", "own"),
        ]
    }

    fn catalog() -> ModelCatalog {
        let mut catalog = ModelCatalog::default();
        catalog.configure(profiles(), id("claude"));
        let claude = generation(&catalog, "claude");
        catalog.learn(
            claude,
            vec![
                model("opus", Some("claude-opus-5-5")),
                model("claude-sonnet-4-6", None),
            ],
        );
        let codex = generation(&catalog, "codex");
        catalog.learn(codex, vec![model("gpt-6-sol", None)]);
        catalog
    }

    #[test]
    fn a_listed_model_its_resolved_name_or_no_model_at_all_is_accepted() {
        let catalog = catalog();
        assert_eq!(catalog.check(&id("claude"), Some("opus")), Ok(()));
        assert_eq!(
            catalog.check(&id("claude"), Some("claude-opus-5-5")),
            Ok(())
        );
        assert_eq!(catalog.check(&id("claude"), None), Ok(()));
    }

    #[test]
    fn a_model_outside_the_list_is_refused_naming_the_listed_ones() {
        let error = catalog().check(&id("claude"), Some("opsu")).unwrap_err();
        assert!(error.contains("profile `claude`"), "{error}");
        assert!(error.contains("`opsu`"), "{error}");
        assert!(error.contains("opus, claude-sonnet-4-6"), "{error}");
    }

    #[test]
    fn each_profile_is_checked_against_its_own_list() {
        let catalog = catalog();
        assert!(catalog.check(&id("codex"), Some("opus")).is_err());
        assert_eq!(catalog.check(&id("codex"), Some("gpt-6-sol")), Ok(()));
    }

    #[test]
    fn two_profiles_of_one_agent_keep_lists_of_their_own() {
        let mut catalog = catalog();
        let work = generation(&catalog, "claude-work");
        catalog.learn(work, vec![model("haiku", None)]);
        assert_eq!(catalog.check(&id("claude-work"), Some("haiku")), Ok(()));
        assert!(catalog.check(&id("claude-work"), Some("opus")).is_err());
        assert!(catalog.check(&id("claude"), Some("haiku")).is_err());
    }

    #[test]
    fn an_answer_for_a_profile_configured_since_is_dropped() {
        let mut catalog = catalog();
        let before = generation(&catalog, "claude-work");
        let mut moved = profiles();
        moved[1].through = "/elsewhere".into();
        catalog.configure(moved, id("claude"));
        catalog.learn(before, vec![model("stale", None)]);
        let view = catalog.view();
        assert_eq!(
            view[1].models, None,
            "a late answer of the old configuration"
        );
        let now = generation(&catalog, "claude-work");
        assert_ne!(now, before);
        catalog.learn(now, vec![model("fresh", None)]);
        assert_eq!(catalog.check(&id("claude-work"), Some("fresh")), Ok(()));
    }

    #[test]
    fn changing_one_profile_leaves_the_others_lists_alone() {
        let mut catalog = catalog();
        let mut changed = profiles();
        changed[1].through = "/elsewhere".into();
        catalog.configure(changed, id("claude"));
        assert!(catalog.check(&id("claude"), Some("opsu")).is_err());
        assert_eq!(catalog.check(&id("codex"), Some("gpt-6-sol")), Ok(()));
        // A profile no longer held is forgotten, and comes back with no list.
        catalog.configure(profiles()[..1].to_vec(), id("claude"));
        catalog.configure(profiles(), id("claude"));
        assert_eq!(catalog.view()[2].models, None);
    }

    #[test]
    fn a_profile_whose_provider_has_not_answered_is_not_checked() {
        let mut catalog = ModelCatalog::default();
        assert_eq!(catalog.check(&id("claude"), Some("anything")), Ok(()));
        catalog.configure(profiles(), id("claude"));
        let claude = generation(&catalog, "claude");
        catalog.learn(claude, Vec::new());
        assert_eq!(catalog.check(&id("claude"), Some("anything")), Ok(()));
    }

    #[test]
    fn an_empty_answer_keeps_the_list_there_was() {
        let mut catalog = catalog();
        let claude = generation(&catalog, "claude");
        catalog.learn(claude, Vec::new());
        assert!(catalog.check(&id("claude"), Some("opsu")).is_err());
    }

    #[test]
    fn a_bracketed_variant_of_a_listed_model_is_accepted() {
        let catalog = catalog();
        assert_eq!(catalog.check(&id("claude"), Some("opus[1m]")), Ok(()));
        assert_eq!(
            catalog.check(&id("claude"), Some("claude-sonnet-4-6[1m]")),
            Ok(())
        );
        assert!(catalog.check(&id("claude"), Some("opsu[1m]")).is_err());
        assert!(catalog.check(&id("claude"), Some("[1m]")).is_err());
    }

    #[test]
    fn surrounding_whitespace_is_not_part_of_the_model() {
        assert_eq!(catalog().check(&id("claude"), Some(" opus ")), Ok(()));
    }

    #[test]
    fn the_view_shows_every_profile_with_its_connection_list_and_the_default() {
        let mut catalog = catalog();
        catalog.configure(profiles(), id("codex"));
        let view = catalog.view();
        let shown: Vec<(&str, &str, bool, bool)> = view
            .iter()
            .map(|p| {
                (
                    p.profile.as_str(),
                    p.connection.as_str(),
                    p.default,
                    p.models.is_some(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("claude", "claude", false, true),
                ("claude-work", "work", false, false),
                ("codex", "codex", true, true),
            ]
        );
    }
}
