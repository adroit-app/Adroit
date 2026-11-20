//! The rule files' data as written: lines, regions, conditions and rules
//! by name, read from TOML.

use std::collections::HashMap;

use adroit_model::{StartupReadiness, State, Wait};
use serde::Deserialize;

use super::Labels;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct File {
    pub(super) agent: String,
    #[serde(default)]
    pub(super) lines: HashMap<String, LineSpec>,
    #[serde(default)]
    pub(super) regions: HashMap<String, RegionSpec>,
    #[serde(default)]
    pub(super) conditions: HashMap<String, GateSpec>,
    #[serde(default)]
    pub(super) rules: Vec<RuleSpec>,
    #[serde(default)]
    pub(super) startup: Vec<RuleSpec>,
    pub(super) answers: Option<AnswersSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AnswersSpec {
    #[serde(rename = "in")]
    pub(super) within: String,
    pub(super) option: String,
    pub(super) tool: Labels,
    pub(super) plan: Labels,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum LineSpec {
    Pattern(String),
    Full { regex: String, next: Option<String> },
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RegionSpec {
    pub(super) within: Option<String>,
    pub(super) first: Option<String>,
    pub(super) last: Option<String>,
    pub(super) since: Option<String>,
    pub(super) after: Option<String>,
    pub(super) before: Option<String>,
    pub(super) from: Option<String>,
    pub(super) head: Option<usize>,
    pub(super) tail: Option<usize>,
    pub(super) tail_non_empty: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RuleSpec {
    pub(super) id: String,
    pub(super) state: Option<StateName>,
    pub(super) startup: Option<ReadinessName>,
    pub(super) dialog: Option<DialogSpec>,
    #[serde(default)]
    pub(super) default: bool,
    pub(super) when: Option<GateSpec>,
    pub(super) shows: Option<String>,
}

/// Whether a dialog holds the keys while a rule decides: always, never, or
/// while a condition of its own holds.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum DialogSpec {
    Always(bool),
    While(GateSpec),
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GateSpec {
    pub(super) holds: Option<String>,
    pub(super) present: Option<String>,
    pub(super) absent: Option<String>,
    #[serde(rename = "in")]
    pub(super) within: Option<String>,
    pub(super) has: Option<String>,
    #[serde(default)]
    pub(super) all: Vec<GateSpec>,
    #[serde(default)]
    pub(super) any: Vec<GateSpec>,
    pub(super) not: Option<Box<GateSpec>>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum StateName {
    Working,
    Background,
    Idle,
    WaitingInput,
    WaitingPermission,
}

impl From<StateName> for State {
    fn from(name: StateName) -> Self {
        match name {
            StateName::Working => State::Working,
            StateName::Background => State::Background,
            StateName::Idle => State::Idle,
            StateName::WaitingInput => State::Waiting(Wait::Input),
            StateName::WaitingPermission => State::Waiting(Wait::Permission),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ReadinessName {
    Ready,
    AwaitingTrust,
    AwaitingPermission,
    AwaitingInput,
}

impl From<ReadinessName> for StartupReadiness {
    fn from(name: ReadinessName) -> Self {
        match name {
            ReadinessName::Ready => StartupReadiness::Ready,
            ReadinessName::AwaitingTrust => StartupReadiness::AwaitingTrust,
            ReadinessName::AwaitingPermission => StartupReadiness::AwaitingPermission,
            ReadinessName::AwaitingInput => StartupReadiness::AwaitingInput,
        }
    }
}
