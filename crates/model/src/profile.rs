//! What a session starts from, as settings.toml names it: a profile, which
//! pins an agent, where the agent keeps its login and how it is launched.

pub mod earlier;

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::Agent;
use crate::config::{CodexOptions, Launch, OpenCodeOptions, PiOptions, SessionOptions};

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

/// How sessions of a profile run, in the terms of its agent alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentOptions {
    Claude(SessionOptions),
    Codex(CodexOptions),
    Pi(PiOptions),
    OpenCode(OpenCodeOptions),
}

impl AgentOptions {
    /// `agent`'s options with nothing chosen.
    pub fn of(agent: Agent) -> Self {
        match agent {
            Agent::Claude => Self::Claude(SessionOptions::default()),
            Agent::Codex => Self::Codex(CodexOptions::default()),
            Agent::Pi => Self::Pi(PiOptions::default()),
            Agent::OpenCode => Self::OpenCode(OpenCodeOptions::default()),
        }
    }

    pub fn agent(&self) -> Agent {
        match self {
            Self::Claude(_) => Agent::Claude,
            Self::Codex(_) => Agent::Codex,
            Self::Pi(_) => Agent::Pi,
            Self::OpenCode(_) => Agent::OpenCode,
        }
    }

    /// The model sessions start with, in the agent's own terms; empty for
    /// the agent's own choice.
    pub fn model(&self) -> &str {
        match self {
            Self::Claude(options) => &options.model,
            Self::Codex(options) => &options.model,
            Self::Pi(options) => &options.model,
            Self::OpenCode(options) => &options.model,
        }
    }

    pub fn model_mut(&mut self) -> &mut String {
        match self {
            Self::Claude(options) => &mut options.model,
            Self::Codex(options) => &mut options.model,
            Self::Pi(options) => &mut options.model,
            Self::OpenCode(options) => &mut options.model,
        }
    }
}

/// What a session is started from: an agent, where it keeps its login, how
/// it is launched and the agent's own options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "ProfileEntry", into = "ProfileEntry")]
pub struct AgentProfile {
    pub id: ProfileId,
    pub name: String,
    /// The folder of the profile's own its agent keeps its login, history and
    /// settings in; none for the agent's own environment, shared with the
    /// terminal.
    pub home: Option<PathBuf>,
    pub launch: Launch,
    pub options: AgentOptions,
}

impl AgentProfile {
    /// The profile adroit comes with for `agent`: the CLI's own environment,
    /// started as its own settings say.
    pub fn builtin(agent: Agent) -> Self {
        Self {
            id: ProfileId::builtin(agent),
            name: agent.label().to_string(),
            home: None,
            launch: Launch::default(),
            options: AgentOptions::of(agent),
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
            _ => None,
        }
    }

    pub fn claude_mut(&mut self) -> Option<&mut SessionOptions> {
        match &mut self.options {
            AgentOptions::Claude(options) => Some(options),
            _ => None,
        }
    }

    /// Codex's options, for a Codex profile.
    pub fn codex(&self) -> Option<&CodexOptions> {
        match &self.options {
            AgentOptions::Codex(options) => Some(options),
            _ => None,
        }
    }

    pub fn codex_mut(&mut self) -> Option<&mut CodexOptions> {
        match &mut self.options {
            AgentOptions::Codex(options) => Some(options),
            _ => None,
        }
    }

    /// Pi's options, for a Pi profile.
    pub fn pi(&self) -> Option<&PiOptions> {
        match &self.options {
            AgentOptions::Pi(options) => Some(options),
            _ => None,
        }
    }

    pub fn pi_mut(&mut self) -> Option<&mut PiOptions> {
        match &mut self.options {
            AgentOptions::Pi(options) => Some(options),
            _ => None,
        }
    }

    /// OpenCode's options, for an OpenCode profile.
    pub fn opencode(&self) -> Option<&OpenCodeOptions> {
        match &self.options {
            AgentOptions::OpenCode(options) => Some(options),
            _ => None,
        }
    }

    pub fn opencode_mut(&mut self) -> Option<&mut OpenCodeOptions> {
        match &mut self.options {
            AgentOptions::OpenCode(options) => Some(options),
            _ => None,
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
    /// The agent it starts, `claude`, `codex`, `pi` or `opencode`.
    agent: Agent,
    /// The folder of its own its agent keeps its login, history and settings
    /// in, as an absolute path without `~`, `.` or `..` and with no `/`
    /// doubled or at its end, which no other profile names; it is fixed when
    /// the login is made, since the CLIs tell logins apart by it as it is
    /// written. Left out, the agent runs in its own environment, shared with
    /// the terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    home: Option<PathBuf>,
    /// The connection files written before profiles owned their logins name;
    /// read by [`earlier`] alone.
    #[serde(default, skip_serializing)]
    #[schemars(skip)]
    connection: Option<String>,
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
    /// Pi's model as `provider/id`, thinking level and tools, for a `pi`
    /// profile; empty values leave Pi's own settings in effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pi: Option<PiOptions>,
    /// OpenCode's model as `provider/model`, the agent it starts as, its
    /// permission rules and whether it approves on its own, for an `opencode`
    /// profile; empty values leave OpenCode's own settings in effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    opencode: Option<OpenCodeOptions>,
    /// Whether files written while profiles were archived rather than
    /// deleted kept it for the sessions that ran from it alone; read by
    /// [`earlier`] alone.
    #[serde(default, skip_serializing)]
    #[schemars(skip)]
    archived: bool,
}

impl TryFrom<ProfileEntry> for AgentProfile {
    type Error = String;

    fn try_from(entry: ProfileEntry) -> Result<Self, Self::Error> {
        match entry.connection {
            Some(connection) => Err(format!(
                "profile `{}` names connection `{connection}`, which settings name no more; give it a `home` of its own, or leave it out for its agent's own environment",
                entry.id
            )),
            None => entry.profile(),
        }
    }
}

impl ProfileEntry {
    /// The profile as written, whatever connection it names.
    fn profile(self) -> Result<AgentProfile, String> {
        let agent = self.agent;
        let held = [
            (Agent::Claude, self.claude.is_some()),
            (Agent::Codex, self.codex.is_some()),
            (Agent::Pi, self.pi.is_some()),
            (Agent::OpenCode, self.opencode.is_some()),
        ];
        if let Some((other, _)) = held.iter().find(|(a, held)| *held && *a != agent) {
            return Err(format!(
                "profile `{}` starts {} and cannot hold options for {}",
                self.id,
                agent.label(),
                other.label()
            ));
        }
        let options = match agent {
            Agent::Claude => AgentOptions::Claude(self.claude.unwrap_or_default()),
            Agent::Codex => AgentOptions::Codex(self.codex.unwrap_or_default()),
            Agent::Pi => AgentOptions::Pi(self.pi.unwrap_or_default()),
            Agent::OpenCode => AgentOptions::OpenCode(self.opencode.unwrap_or_default()),
        };
        Ok(AgentProfile {
            home: self.home,
            id: self.id,
            name: self.name,
            launch: self.launch,
            options,
        })
    }
}

impl From<AgentProfile> for ProfileEntry {
    fn from(profile: AgentProfile) -> Self {
        let agent = profile.agent();
        let (mut claude, mut codex, mut pi, mut opencode) = (None, None, None, None);
        match profile.options {
            AgentOptions::Claude(options) => claude = Some(options),
            AgentOptions::Codex(options) => codex = Some(options),
            AgentOptions::Pi(options) => pi = Some(options),
            AgentOptions::OpenCode(options) => opencode = Some(options),
        }
        Self {
            id: profile.id,
            name: profile.name,
            agent,
            home: profile.home,
            connection: None,
            launch: profile.launch,
            claude,
            codex,
            pi,
            opencode,
            archived: false,
        }
    }
}

/// Whether a profile's CLI says it is logged in where the profile runs it,
/// as it answered when asked; adroit never takes a login for granted.
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

/// Whether `profiles` hold together, with `default` naming the profile a
/// session starts from when nothing else is named; why not otherwise.
pub fn check(default: &ProfileId, profiles: &[AgentProfile]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for profile in profiles {
        if !seen.insert(&profile.id) {
            return Err(format!("profile `{}` is defined twice", profile.id));
        }
    }
    for (at, profile) in profiles.iter().enumerate() {
        let Some(home) = &profile.home else {
            continue;
        };
        if !plain_absolute(home) {
            return Err(format!(
                "profile `{}` needs an absolute path as its home, written without `~`, `.` or `..` and with no `/` doubled or at its end",
                profile.id
            ));
        }
        if let Some(other) = profiles[..at]
            .iter()
            .find(|p| p.home.as_ref() == Some(home))
        {
            return Err(format!(
                "profiles `{}` and `{}` both keep their login in {}; each profile needs a home of its own",
                other.id,
                profile.id,
                home.display()
            ));
        }
    }
    if profiles.iter().any(|p| &p.id == default) {
        Ok(())
    } else {
        Err(format!("profile `{default}` is not defined"))
    }
}

/// Whether `home` is an absolute path written the one way: the agents' programs
/// tell logins apart by the directory's name as it is written, so any other
/// spelling of the same directory would be another login.
pub(crate) fn plain_absolute(home: &Path) -> bool {
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
    use crate::testing::profile;

    fn id(text: &str) -> ProfileId {
        text.parse().unwrap()
    }

    #[test]
    fn an_id_is_lowercase_letters_digits_and_hyphens_starting_with_a_letter_or_digit() {
        for good in ["claude", "claude-work", "2nd", "a"] {
            assert!(good.parse::<ProfileId>().is_ok(), "{good}");
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
        }
        let error = "Work".parse::<ProfileId>().unwrap_err().to_string();
        assert!(error.contains("`Work` is not a valid id"), "{error}");
    }

    #[test]
    fn a_built_in_profile_starts_its_agent_in_its_own_environment_as_the_cli_says() {
        let codex = AgentProfile::builtin(Agent::Codex);
        assert_eq!(codex.id.as_str(), "codex");
        assert_eq!(codex.label(), "Codex");
        assert_eq!(codex.home, None);
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
    fn an_opencode_profile_holds_its_model_agent_permission_rules_and_auto_approval() {
        use crate::config::{OpenCodeOptions, OpenCodeRule};
        let read = |text: &str| serde_json::from_str::<AgentProfile>(text);
        let opencode = read(
            r#"{"id":"o","agent":"opencode","opencode":{"model":"ollama/qwen3:8b","agent":"plan","auto":true,
                "permissions":[{"action":"bash","resource":"git *","effect":"allow"}]}}"#,
        )
        .unwrap();
        assert_eq!(
            opencode.opencode(),
            Some(&OpenCodeOptions {
                model: "ollama/qwen3:8b".into(),
                agent: "plan".into(),
                permissions: vec![OpenCodeRule {
                    action: "bash".into(),
                    resource: "git *".into(),
                    effect: "allow".into(),
                }],
                auto: true,
            })
        );
        assert_eq!(opencode.options.model(), "ollama/qwen3:8b");
        assert_eq!(opencode.pi(), None);
        let written = serde_json::to_value(&opencode).unwrap();
        assert_eq!(written["opencode"]["agent"], "plan");
        assert_eq!(written.get("pi"), None);
        assert_eq!(
            serde_json::from_value::<AgentProfile>(written).unwrap(),
            opencode
        );
        let error = read(r#"{"id":"o","agent":"opencode","pi":{}}"#)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("profile `o` starts OpenCode and cannot hold options for Pi"),
            "{error}"
        );
        assert!(read(r#"{"id":"o","agent":"opencode","opencode":{"tools":[]}}"#).is_err());
        let builtin = AgentProfile::builtin(Agent::OpenCode);
        assert_eq!(
            (builtin.id.as_str(), builtin.label()),
            ("opencode", "OpenCode")
        );
        assert_eq!(builtin.opencode(), Some(&OpenCodeOptions::default()));
    }

    #[test]
    fn a_pi_profile_holds_its_model_thinking_and_tools() {
        let read = |text: &str| serde_json::from_str::<AgentProfile>(text);
        let pi = read(
            r#"{"id":"p","agent":"pi","pi":{"model":"ollama/qwen3","thinking":"high","tools":["read","grep"]}}"#,
        )
        .unwrap();
        assert_eq!(
            pi.pi(),
            Some(&PiOptions {
                model: "ollama/qwen3".into(),
                thinking: "high".into(),
                tools: vec!["read".into(), "grep".into()],
            })
        );
        assert_eq!(pi.claude(), None);
        assert_eq!(pi.codex(), None);
        let written = serde_json::to_value(&pi).unwrap();
        assert_eq!(written["pi"]["model"], "ollama/qwen3");
        assert_eq!(written.get("codex"), None);
        assert_eq!(serde_json::from_value::<AgentProfile>(written).unwrap(), pi);
        let error = read(r#"{"id":"p","agent":"pi","codex":{"model":"x"}}"#)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("profile `p` starts Pi and cannot hold options for Codex"),
            "{error}"
        );
        let error = read(r#"{"id":"p","agent":"codex","pi":{}}"#)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("profile `p` starts Codex and cannot hold options for Pi"),
            "{error}"
        );
        assert!(read(r#"{"id":"p","agent":"pi","pi":{"approval_policy":"never"}}"#).is_err());
        let builtin = AgentProfile::builtin(Agent::Pi);
        assert_eq!((builtin.id.as_str(), builtin.label()), ("pi", "Pi"));
        assert_eq!(builtin.pi(), Some(&PiOptions::default()));
    }

    #[test]
    fn a_profile_in_its_agent_s_own_environment_is_written_without_a_home() {
        let own = profile("p", Agent::Claude).build();
        let written = serde_json::to_value(&own).unwrap();
        assert_eq!(written.get("home"), None);
        assert_eq!(written.get("connection"), None);
        assert_eq!(written.get("archived"), None);
        assert_eq!(
            serde_json::from_value::<AgentProfile>(written).unwrap(),
            own
        );
        let apart = profile("p", Agent::Claude).home("/w").build();
        let written = serde_json::to_value(&apart).unwrap();
        assert_eq!(written["home"], "/w");
        assert_eq!(
            serde_json::from_value::<AgentProfile>(written).unwrap(),
            apart
        );
    }

    #[test]
    fn profiles_that_hold_together_pass() {
        let profiles = [
            AgentProfile::builtin(Agent::Claude),
            profile("claude-work", Agent::Claude).home("/w").build(),
            profile("codex-review", Agent::Codex).home("/c").build(),
        ];
        assert_eq!(check(&id("claude-work"), &profiles), Ok(()));
    }

    #[test]
    fn profiles_that_do_not_hold_together_say_why() {
        let claude = || profile("claude", Agent::Claude);
        let refused = |default: &str, profiles: Vec<AgentProfile>, why: &str| {
            let error = check(&id(default), &profiles).unwrap_err();
            assert!(error.contains(why), "{error} does not say {why}");
        };
        refused(
            "claude",
            vec![claude().build(), claude().build()],
            "profile `claude` is defined twice",
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
                vec![claude().home(home).build()],
                "profile `claude` needs an absolute path as its home",
            );
        }
        // Whatever agent, no two share a login.
        refused(
            "claude",
            vec![
                claude().home("/w").build(),
                profile("codex", Agent::Codex).home("/w").build(),
            ],
            "profiles `claude` and `codex` both keep their login in /w",
        );
        refused(
            "other",
            vec![claude().build()],
            "profile `other` is not defined",
        );
    }
}
