//! What a session starts from, as settings.toml names it: a profile, which
//! pins an agent, the connection it runs in and how it is launched, and the
//! connections, each a state directory of one agent's CLI.

use std::collections::HashSet;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::Agent;
use crate::config::{CodexOptions, Launch, SessionOptions};

/// Why a string cannot be an id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidId(String);

impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` is not a valid id: use lowercase letters, digits and `-`, starting with a letter or a digit, up to 64 characters",
            self.0
        )
    }
}

impl std::error::Error for InvalidId {}

/// Whether `id` keeps to the characters an id may hold: safe in a file name,
/// a terminal tag and a database column without quoting.
fn valid_id(id: &str) -> bool {
    id.len() <= 64
        && id
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

macro_rules! string_ids {
    ($($(#[$meta:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = InvalidId;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                if valid_id(value) {
                    Ok(Self(value.to_string()))
                } else {
                    Err(InvalidId(value.to_string()))
                }
            }
        }

        impl TryFrom<String> for $name {
            type Error = InvalidId;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                value.parse()
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> Self {
                id.0
            }
        }
    )+};
}

string_ids!(
    /// A profile, kept when the profile is renamed.
    ProfileId,
    /// A connection, kept when the connection is renamed.
    ConnectionId,
);

impl ProfileId {
    /// The profile adroit comes with for `agent`.
    pub fn builtin(agent: Agent) -> Self {
        Self(agent.key().to_string())
    }
}

/// The profile adroit comes with for the agent a session is when nothing says.
impl Default for ProfileId {
    fn default() -> Self {
        Self::builtin(Agent::default())
    }
}

/// The own environment of the agent a session is when nothing says.
impl Default for ConnectionId {
    fn default() -> Self {
        Self::implicit(Agent::default())
    }
}

impl ConnectionId {
    /// The connection every agent has without one being defined: the CLI's
    /// own environment, as it runs outside adroit.
    pub fn implicit(agent: Agent) -> Self {
        Self(agent.key().to_string())
    }

    /// The agent whose own environment this id stands for, if it is one.
    pub fn implicit_agent(&self) -> Option<Agent> {
        Agent::parse(&self.0)
    }
}

/// How sessions of a profile run, in the terms of its agent alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentOptions {
    Claude(SessionOptions),
    Codex(CodexOptions),
}

impl AgentOptions {
    /// `agent`'s options with nothing chosen.
    pub fn of(agent: Agent) -> Self {
        match agent {
            Agent::Claude => Self::Claude(SessionOptions::default()),
            Agent::Codex => Self::Codex(CodexOptions::default()),
        }
    }

    pub fn agent(&self) -> Agent {
        match self {
            Self::Claude(_) => Agent::Claude,
            Self::Codex(_) => Agent::Codex,
        }
    }
}

/// What a session is started from: an agent, the connection it runs in, how
/// it is launched and the agent's own options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "ProfileEntry", into = "ProfileEntry")]
pub struct AgentProfile {
    pub id: ProfileId,
    pub name: String,
    pub connection: ConnectionId,
    pub launch: Launch,
    pub options: AgentOptions,
    pub archived: bool,
}

impl AgentProfile {
    /// The profile adroit comes with for `agent`: the CLI's own environment,
    /// started as its own settings say.
    pub fn builtin(agent: Agent) -> Self {
        Self {
            id: ProfileId::builtin(agent),
            name: agent.label().to_string(),
            connection: ConnectionId::implicit(agent),
            launch: Launch::default(),
            options: AgentOptions::of(agent),
            archived: false,
        }
    }

    pub fn agent(&self) -> Agent {
        self.options.agent()
    }

    /// What the user calls the profile: its name, or its id when it has none.
    pub fn label(&self) -> &str {
        match self.name.trim() {
            "" => self.id.as_str(),
            name => name,
        }
    }

    /// Claude Code's options, for a Claude Code profile.
    pub fn claude(&self) -> Option<&SessionOptions> {
        match &self.options {
            AgentOptions::Claude(options) => Some(options),
            AgentOptions::Codex(_) => None,
        }
    }

    pub fn claude_mut(&mut self) -> Option<&mut SessionOptions> {
        match &mut self.options {
            AgentOptions::Claude(options) => Some(options),
            AgentOptions::Codex(_) => None,
        }
    }

    /// Codex's options, for a Codex profile.
    pub fn codex(&self) -> Option<&CodexOptions> {
        match &self.options {
            AgentOptions::Codex(options) => Some(options),
            AgentOptions::Claude(_) => None,
        }
    }

    pub fn codex_mut(&mut self) -> Option<&mut CodexOptions> {
        match &mut self.options {
            AgentOptions::Codex(options) => Some(options),
            AgentOptions::Claude(_) => None,
        }
    }
}

/// A profile as settings.toml writes it.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ProfileEntry {
    /// The profile's id, kept when it is renamed: lowercase letters, digits
    /// and `-`.
    id: ProfileId,
    /// What the profile is called; empty for its id.
    #[serde(default)]
    name: String,
    /// The agent it starts, `claude` or `codex`.
    agent: Agent,
    /// The connection it runs in, by id; left out for the agent's own
    /// environment, which the agent's key also names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    connection: Option<ConnectionId>,
    /// How its agent is started; an empty command runs the agent's own program.
    #[serde(default)]
    launch: Launch,
    /// Claude Code's model, permission mode and tools denied, for a `claude`
    /// profile; what is left empty is Claude Code's own choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    claude: Option<SessionOptions>,
    /// Codex's model and reasoning effort, approval policy and sandbox, for a
    /// `codex` profile; empty values leave Codex's own settings in effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    codex: Option<CodexOptions>,
    /// Whether it is kept only for the sessions that ran from it, and offered
    /// for no new one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    archived: bool,
}

impl TryFrom<ProfileEntry> for AgentProfile {
    type Error = String;

    fn try_from(entry: ProfileEntry) -> Result<Self, Self::Error> {
        let options = match (entry.agent, entry.claude, entry.codex) {
            (Agent::Claude, claude, None) => AgentOptions::Claude(claude.unwrap_or_default()),
            (Agent::Codex, None, codex) => AgentOptions::Codex(codex.unwrap_or_default()),
            (agent, _, _) => {
                let other = Agent::ALL
                    .into_iter()
                    .find(|a| *a != agent)
                    .unwrap_or(agent);
                return Err(format!(
                    "profile `{}` starts {} and cannot hold options for {}",
                    entry.id,
                    agent.label(),
                    other.label()
                ));
            }
        };
        Ok(Self {
            connection: entry
                .connection
                .unwrap_or_else(|| ConnectionId::implicit(entry.agent)),
            id: entry.id,
            name: entry.name,
            launch: entry.launch,
            options,
            archived: entry.archived,
        })
    }
}

impl From<AgentProfile> for ProfileEntry {
    fn from(profile: AgentProfile) -> Self {
        let agent = profile.agent();
        let (claude, codex) = match profile.options {
            AgentOptions::Claude(options) => (Some(options), None),
            AgentOptions::Codex(options) => (None, Some(options)),
        };
        Self {
            id: profile.id,
            name: profile.name,
            agent,
            connection: (profile.connection != ConnectionId::implicit(agent))
                .then_some(profile.connection),
            launch: profile.launch,
            claude,
            codex,
            archived: profile.archived,
        }
    }
}

/// A state directory of one agent's CLI, which holds its login, history and
/// settings apart from every other; the CLI itself logs in and keeps the
/// credentials there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentConnection {
    /// The connection's id, kept when it is renamed: lowercase letters, digits
    /// and `-`, and none of the agents' keys, which name their own
    /// environments.
    pub id: ConnectionId,
    /// What the connection is called; empty for its id.
    #[serde(default)]
    pub name: String,
    /// The agent whose CLI keeps its state there, `claude` or `codex`.
    pub agent: Agent,
    /// The state directory, as an absolute path without `~`, `.` or `..` and
    /// with no `/` doubled or at its end; it is fixed when the connection is
    /// made, since the CLIs tell logins apart by it as it is written.
    pub home: PathBuf,
    /// Whether it is kept only for the profiles and sessions that used it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub archived: bool,
}

/// Whether a connection's CLI says it is logged in, as it answered when
/// asked; adroit never takes a login for granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginStanding {
    LoggedIn,
    LoggedOut,
    /// The CLI answered in a way that tells neither.
    Unknown,
    /// The CLI is not installed where it is started from.
    Missing,
}

impl LoginStanding {
    pub fn label(self) -> &'static str {
        match self {
            Self::LoggedIn => "Logged in",
            Self::LoggedOut => "Not logged in",
            Self::Unknown => "Unknown",
            Self::Missing => "CLI not found",
        }
    }
}

/// Whether `profiles` and `connections` hold together, with `default`
/// naming the profile a session starts from when nothing else is named; why
/// not otherwise.
pub fn check(
    default: &ProfileId,
    profiles: &[AgentProfile],
    connections: &[AgentConnection],
) -> Result<(), String> {
    let mut seen = HashSet::new();
    for connection in connections {
        if !seen.insert(&connection.id) {
            return Err(format!("connection `{}` is defined twice", connection.id));
        }
        if let Some(agent) = connection.id.implicit_agent() {
            return Err(format!(
                "connection id `{}` is reserved for {}'s own environment",
                connection.id,
                agent.label()
            ));
        }
        if !plain_absolute(&connection.home) {
            return Err(format!(
                "connection `{}` needs an absolute path as its home, written without `~`, `.` or `..` and with no `/` doubled or at its end",
                connection.id
            ));
        }
    }
    let mut seen = HashSet::new();
    for profile in profiles {
        if !seen.insert(&profile.id) {
            return Err(format!("profile `{}` is defined twice", profile.id));
        }
        let agent = profile.agent();
        let owner = match profile.connection.implicit_agent() {
            Some(owner) => owner,
            None => {
                connections
                    .iter()
                    .find(|c| c.id == profile.connection)
                    .ok_or_else(|| {
                        format!(
                            "profile `{}` names connection `{}`, which is not defined",
                            profile.id, profile.connection
                        )
                    })?
                    .agent
            }
        };
        if owner != agent {
            return Err(format!(
                "profile `{}` starts {} but connection `{}` belongs to {}",
                profile.id,
                agent.label(),
                profile.connection,
                owner.label()
            ));
        }
    }
    match profiles.iter().find(|p| &p.id == default) {
        None => Err(format!("profile `{default}` is not defined")),
        Some(p) if p.archived => Err(format!(
            "profile `{default}` is archived and cannot be the default"
        )),
        Some(_) => Ok(()),
    }
}

/// Whether `home` is an absolute path written the one way: the agents' programs
/// tell logins apart by the directory's name as it is written, so any other
/// spelling of the same directory would be another login.
fn plain_absolute(home: &std::path::Path) -> bool {
    let Some(text) = home.to_str() else {
        return false;
    };
    text.len() > 1
        && text.starts_with('/')
        && !text.ends_with('/')
        && !text.contains("//")
        && !text.contains('~')
        && text[1..].split('/').all(|part| part != "." && part != "..")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{connection, profile};

    fn id(text: &str) -> ProfileId {
        text.parse().unwrap()
    }

    #[test]
    fn an_id_is_lowercase_letters_digits_and_hyphens_starting_with_a_letter_or_digit() {
        for good in ["claude", "claude-work", "2nd", "a"] {
            assert!(good.parse::<ProfileId>().is_ok(), "{good}");
            assert!(good.parse::<ConnectionId>().is_ok(), "{good}");
        }
        for bad in [
            "",
            "Claude",
            "-work",
            "a_b",
            "a b",
            "a/b",
            "ü",
            &"a".repeat(65),
        ] {
            assert!(bad.parse::<ProfileId>().is_err(), "{bad}");
            assert!(bad.parse::<ConnectionId>().is_err(), "{bad}");
        }
        let error = "Work".parse::<ProfileId>().unwrap_err().to_string();
        assert!(error.contains("`Work` is not a valid id"), "{error}");
    }

    #[test]
    fn every_agent_s_own_environment_is_a_connection_named_by_its_key() {
        for agent in Agent::ALL {
            let own = ConnectionId::implicit(agent);
            assert_eq!(own.as_str(), agent.key());
            assert_eq!(own.implicit_agent(), Some(agent));
        }
        assert_eq!(
            "work".parse::<ConnectionId>().unwrap().implicit_agent(),
            None
        );
    }

    #[test]
    fn a_built_in_profile_starts_its_agent_in_its_own_environment_as_the_cli_says() {
        let codex = AgentProfile::builtin(Agent::Codex);
        assert_eq!(codex.id.as_str(), "codex");
        assert_eq!(codex.label(), "Codex");
        assert_eq!(codex.connection, ConnectionId::implicit(Agent::Codex));
        assert_eq!(codex.codex(), Some(&CodexOptions::default()));
        assert_eq!(codex.claude(), None);
        assert_eq!(codex.launch, Launch::default());
    }

    #[test]
    fn a_profile_left_unnamed_is_called_by_its_id() {
        let unnamed = AgentProfile {
            name: " ".into(),
            ..profile("review", Agent::Codex).build()
        };
        assert_eq!(unnamed.label(), "review");
    }

    #[test]
    fn a_profile_holds_options_for_its_own_agent_only() {
        let read = |text: &str| serde_json::from_str::<AgentProfile>(text);
        let claude = read(r#"{"id":"p","agent":"claude","claude":{"model":"opus"}}"#).unwrap();
        assert_eq!(claude.claude().unwrap().model, "opus");
        // Options left out are the agent's with nothing chosen.
        let codex = read(r#"{"id":"p","agent":"codex"}"#).unwrap();
        assert_eq!(codex.codex(), Some(&CodexOptions::default()));
        let error = read(r#"{"id":"p","agent":"claude","codex":{"model":"x"}}"#)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("profile `p` starts Claude Code and cannot hold options for Codex"),
            "{error}"
        );
        assert!(read(r#"{"id":"p","agent":"claude","colour":"red"}"#).is_err());
    }

    #[test]
    fn a_profile_in_its_agent_s_own_environment_is_written_without_a_connection() {
        let own = profile("p", Agent::Claude).build();
        let written = serde_json::to_value(&own).unwrap();
        assert_eq!(written.get("connection"), None);
        assert_eq!(written.get("archived"), None);
        assert_eq!(
            serde_json::from_value::<AgentProfile>(written).unwrap(),
            own
        );
        // Naming the agent's key is the same as leaving it out.
        let named: AgentProfile =
            serde_json::from_str(r#"{"id":"p","agent":"claude","connection":"claude"}"#).unwrap();
        assert_eq!(named.connection, own.connection);
        let elsewhere = profile("p", Agent::Claude).connection("work").build();
        let written = serde_json::to_value(&elsewhere).unwrap();
        assert_eq!(written["connection"], "work");
        assert_eq!(
            serde_json::from_value::<AgentProfile>(written).unwrap(),
            elsewhere
        );
    }

    #[test]
    fn profiles_and_connections_that_hold_together_pass() {
        let profiles = [
            AgentProfile::builtin(Agent::Claude),
            profile("claude-work", Agent::Claude)
                .connection("work")
                .build(),
            profile("codex-review", Agent::Codex).build(),
        ];
        let connections = [connection("work", Agent::Claude, "/w").build()];
        assert_eq!(check(&id("claude-work"), &profiles, &connections), Ok(()));
    }

    #[test]
    fn profiles_and_connections_that_do_not_hold_together_say_why() {
        let claude = || profile("claude", Agent::Claude);
        let work = || connection("work", Agent::Claude, "/w");
        let refused = |default: &str,
                       profiles: Vec<AgentProfile>,
                       connections: Vec<AgentConnection>,
                       why: &str| {
            let error = check(&id(default), &profiles, &connections).unwrap_err();
            assert!(error.contains(why), "{error} does not say {why}");
        };
        refused(
            "claude",
            vec![claude().build(), claude().build()],
            vec![],
            "profile `claude` is defined twice",
        );
        refused(
            "claude",
            vec![claude().build()],
            vec![work().build(), work().build()],
            "connection `work` is defined twice",
        );
        refused(
            "claude",
            vec![claude().build()],
            vec![connection("codex", Agent::Codex, "/c").build()],
            "connection id `codex` is reserved for Codex's own environment",
        );
        for home in [
            "",
            "/",
            "relative/dir",
            "~/claude",
            "/Users/me/~work",
            "/Users/me/work/",
            "/Users/me//work",
            "/Users/me/./work",
            "/Users/me/../me/work",
        ] {
            refused(
                "claude",
                vec![claude().build()],
                vec![connection("work", Agent::Claude, home).build()],
                "connection `work` needs an absolute path as its home",
            );
        }
        refused(
            "claude",
            vec![claude().connection("gone").build()],
            vec![],
            "profile `claude` names connection `gone`, which is not defined",
        );
        refused(
            "claude",
            vec![claude().connection("work").build()],
            vec![connection("work", Agent::Codex, "/w").build()],
            "profile `claude` starts Claude Code but connection `work` belongs to Codex",
        );
        refused(
            "claude",
            vec![claude().connection("codex").build()],
            vec![],
            "profile `claude` starts Claude Code but connection `codex` belongs to Codex",
        );
        refused(
            "other",
            vec![claude().build()],
            vec![],
            "profile `other` is not defined",
        );
        refused(
            "claude",
            vec![claude().archived().build()],
            vec![],
            "profile `claude` is archived and cannot be the default",
        );
    }
}
