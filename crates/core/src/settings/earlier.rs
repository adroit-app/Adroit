//! What `settings.toml` holds beside the settings as adroit writes them: the
//! profiles in the shape earlier files may write them, the agent tables files
//! written before profiles hold in their place, and settings retired since.

use serde::Deserialize;

use adroit_model::Agent;
use adroit_model::config::{CodexOptions, Launch, SessionOptions};
use adroit_model::profile::earlier::{self, Connection, Written};
use adroit_model::profile::{AgentOptions, AgentProfile, ProfileId};

/// Settings earlier versions wrote that adroit reads no more, for projects
/// it no longer keeps and the orchestration delegation replaced: dropped as
/// they are read, so a file they are in still reads.
const RETIRED: &[&str] = &["naming_model", "projects_dir", "orchestration"];

/// The same within `[delegation]`, where earlier files capped how many
/// sessions opened by sessions could run.
const RETIRED_DELEGATION: &[&str] = &["max_started"];

/// `settings.toml` as it is read: the profiles, what files written before
/// profiles hold in their place, and every other setting as written.
#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct File {
    profile: Option<ProfileId>,
    profiles: Option<Vec<Written>>,
    connections: Vec<Connection>,
    agent: Option<Agent>,
    claude: Option<Launch>,
    sessions: Option<SessionOptions>,
    codex: Option<Launch>,
    codex_sessions: Option<CodexOptions>,
    #[serde(flatten)]
    rest: toml::Table,
}

/// The profiles a file holds, as the settings keep them.
pub(super) struct Profiles {
    pub(super) profile: ProfileId,
    pub(super) profiles: Vec<AgentProfile>,
    pub(super) unshared: Vec<ProfileId>,
}

impl File {
    /// The profiles the file holds, checked, and every other setting it
    /// holds without those retired.
    pub(super) fn split(self) -> Result<(Profiles, toml::Table), String> {
        let mut rest = self.rest;
        for key in RETIRED {
            rest.remove(*key);
        }
        if let Some(toml::Value::Table(delegation)) = rest.get_mut("delegation") {
            for key in RETIRED_DELEGATION {
                delegation.remove(*key);
            }
        }
        let earlier: Vec<&str> = [
            ("agent", self.agent.is_some()),
            ("claude", self.claude.is_some()),
            ("sessions", self.sessions.is_some()),
            ("codex", self.codex.is_some()),
            ("codex_sessions", self.codex_sessions.is_some()),
        ]
        .into_iter()
        .filter_map(|(key, named)| named.then_some(key))
        .collect();
        let mixed = |profile_key: &str| {
            let keys: Vec<String> = earlier.iter().map(|k| format!("`{k}`")).collect();
            format!(
                "`{profile_key}` cannot be named beside {}, which only settings written before profiles hold; move what they say into a profile",
                keys.join(", ")
            )
        };
        let (profile, (profiles, unshared)) = match self.profiles {
            Some(_) if !earlier.is_empty() => return Err(mixed("profiles")),
            Some(written) => {
                let profiles = earlier::adopt(written, &self.connections)?;
                let profile = match self.profile {
                    Some(profile) => profile,
                    None => profiles.0.first().map(|p| p.id.clone()).ok_or(
                        "the settings hold no profile; add one, or leave `profiles` out for the ones adroit comes with",
                    )?,
                };
                (profile, profiles)
            }
            None if self.profile.is_some() && !earlier.is_empty() => {
                return Err(mixed("profile"));
            }
            None if !self.connections.is_empty() && !earlier.is_empty() => {
                return Err(mixed("connections"));
            }
            None => {
                let claude = AgentProfile {
                    launch: self.claude.unwrap_or_default(),
                    options: AgentOptions::Claude(self.sessions.unwrap_or_default()),
                    ..AgentProfile::builtin(Agent::Claude)
                };
                let codex = AgentProfile {
                    launch: self.codex.unwrap_or_default(),
                    options: AgentOptions::Codex(self.codex_sessions.unwrap_or_default()),
                    ..AgentProfile::builtin(Agent::Codex)
                };
                let profile = self
                    .profile
                    .unwrap_or_else(|| ProfileId::builtin(self.agent.unwrap_or_default()));
                (
                    profile,
                    (
                        vec![
                            claude,
                            codex,
                            AgentProfile::builtin(Agent::Pi),
                            AgentProfile::builtin(Agent::OpenCode),
                        ],
                        Vec::new(),
                    ),
                )
            }
        };
        adroit_model::profile::check(&profile, &profiles)?;
        Ok((
            Profiles {
                profile,
                profiles,
                unshared,
            },
            rest,
        ))
    }
}
