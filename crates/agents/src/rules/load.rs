//! The rules in force for an agent: the ones Adroit carries, or the user's
//! own file when it can be used, read again when it changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::SystemTime;

use adroit_model::Agent;

use super::Rules;

/// The rules Adroit carries, one set an agent.
pub(crate) const BUNDLED: &[(Agent, &str)] = &[
    (Agent::Claude, include_str!("../../rules/claude.toml")),
    (Agent::Codex, include_str!("../../rules/codex.toml")),
    (Agent::Pi, include_str!("../../rules/pi.toml")),
    (Agent::OpenCode, include_str!("../../rules/opencode.toml")),
];

impl Rules {
    /// The rules Adroit carries for `agent`, if it reads that agent's screen.
    pub fn bundled(agent: Agent) -> Option<Self> {
        let (_, text) = BUNDLED.iter().find(|(a, _)| *a == agent)?;
        Some(
            Self::parse(agent.key(), text)
                .unwrap_or_else(|e| panic!("the rules bundled for {}: {e}", agent.key())),
        )
    }
}

/// Where the user keeps rules of their own for `agent`, in place of the ones
/// Adroit carries.
pub fn user_file(agent: Agent) -> PathBuf {
    adroit_model::paths::config_dir()
        .join("screen-rules")
        .join(format!("{}.toml", agent.key()))
}

/// The rules that judge an agent's screen, and where they came from.
#[derive(Debug)]
pub struct InForce {
    pub rules: Rules,
    /// The user's file the rules were read from; none for the bundled ones.
    pub from: Option<PathBuf>,
    /// Why the user's file was set aside for the bundled rules.
    pub warning: Option<String>,
}

impl InForce {
    /// The rules for `agent`: those the user wrote at `path` if they can be
    /// used, else the bundled ones, saying why the user's were not.
    pub fn load(agent: Agent, path: &Path) -> Self {
        let bundled = |warning| Self {
            rules: Rules::bundled(agent).expect("every agent's rules are bundled"),
            from: None,
            warning,
        };
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return bundled(None),
            Err(e) => {
                return bundled(Some(format!(
                    "{} was not read, so the bundled rules judge instead: {e}",
                    path.display()
                )));
            }
        };
        match Rules::parse(agent.key(), &text) {
            Ok(rules) => Self {
                rules,
                from: Some(path.to_path_buf()),
                warning: None,
            },
            Err(e) => bundled(Some(format!(
                "{} cannot be used, so the bundled rules judge instead: {e}",
                path.display()
            ))),
        }
    }
}

/// The rules read for an agent from a file, and when the file was last
/// changed as they were read; none while there was no file.
type Read = (Option<SystemTime>, Arc<InForce>);

/// The rules in force for each agent, read again whenever the user's file
/// is changed, made or removed.
#[derive(Default)]
pub struct Kept(Mutex<HashMap<(Agent, PathBuf), Read>>);

impl Kept {
    pub fn get(&self, agent: Agent, path: &Path) -> Arc<InForce> {
        let stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        let mut kept = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let key = (agent, path.to_path_buf());
        if let Some((seen, in_force)) = kept.get(&key)
            && *seen == stamp
        {
            return in_force.clone();
        }
        let in_force = Arc::new(InForce::load(agent, path));
        kept.insert(key, (stamp, in_force.clone()));
        in_force
    }
}

/// The rules in force for `agent`, from the user's file if it can be used.
pub fn in_force(agent: Agent) -> Arc<InForce> {
    static KEPT: LazyLock<Kept> = LazyLock::new(Kept::default);
    if cfg!(test) {
        // This crate's tests hold the bundled rules to the recorded screens,
        // whatever rules the person running them keeps.
        static BUNDLED_ONLY: LazyLock<Kept> = LazyLock::new(Kept::default);
        return BUNDLED_ONLY.get(agent, Path::new("/nonexistent/adroit/screen-rules.toml"));
    }
    KEPT.get(agent, &user_file(agent))
}
