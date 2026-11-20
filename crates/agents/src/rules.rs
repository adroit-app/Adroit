//! The rules that read an agent's screen, written as data: which lines on
//! the screen mean something, the regions found by them, and the rules,
//! tried in order, that tell from them how the agent stands, whether a
//! dialog holds the keys, and whether it is ready at startup.

mod compile;
mod load;
mod screen;
mod spec;

use adroit_model::{Agent, StartupReadiness, State};
use regex::Regex;
use serde::Deserialize;

use crate::{Judgement, Verdict};
use compile::{Gate, Line, Region, Rule};

pub use load::{InForce, Kept, in_force, user_file};

/// Why a set of rules was refused.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct RulesError(String);

fn refuse<T>(why: impl Into<String>) -> Result<T, RulesError> {
    Err(RulesError(why.into()))
}

/// A set of rules, read and checked, ready to judge screens.
#[derive(Debug)]
pub struct Rules {
    agent: String,
    lines: Vec<Line>,
    regions: Vec<Region>,
    /// Region indices in an order where each comes after those it is built on.
    order: Vec<usize>,
    conditions: Vec<Gate>,
    state: Vec<Rule>,
    startup: Vec<Rule>,
    answers: Option<Answers>,
}

/// The rule that decided, and the rows whose signs it matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matched {
    /// `<agent>.<id>`.
    pub rule: String,
    /// Whether the rule is a default, taken for want of any sign.
    pub default: bool,
    /// The rows, by index into the screen, that the rule's signs matched.
    pub evidence: Vec<(usize, String)>,
}

/// How a screen shows an agent to stand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub state: Option<State>,
    /// Whether a dialog holds the keys.
    pub dialog: bool,
    pub matched: Matched,
}

/// How far a screen shows a session just started to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupReading {
    pub readiness: Option<StartupReadiness>,
    pub matched: Matched,
}

/// The labels of the options that allow and deny, as a dialog draws them;
/// none to deny when its dialog offers no option that does.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Labels {
    pub allow: String,
    pub deny: Option<String>,
}

/// Where a dialog asking the user is drawn, how its options are, and which
/// of them allow and deny a tool and a plan.
#[derive(Debug)]
pub struct Answers {
    within: usize,
    option: Regex,
    pub tool: Labels,
    pub plan: Labels,
}

/// An option a dialog offers: what is typed to choose it, and its label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offered {
    pub key: String,
    pub label: String,
}

/// A dialog holding the keys, as the rows of the region it asks in and the
/// options found there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub rows: Vec<String>,
    pub options: Vec<Offered>,
}

impl Matched {
    fn verdict<S: AsRef<str>>(&self, screen: &[S]) -> Verdict {
        let rows = self.evidence.iter().map(|(row, _)| *row);
        match self.default {
            true => Verdict::defaulted(&self.rule, screen, rows),
            false => Verdict::matched(&self.rule, screen, rows),
        }
    }
}

/// How `screen` shows `agent` to stand, by the rules in force for it.
pub fn judge_state<S: AsRef<str>>(agent: Agent, screen: &[S]) -> Judgement<State> {
    let reading = in_force(agent).rules.judge(screen);
    Judgement::new(reading.state, reading.matched.verdict(screen))
}

/// How far `screen` shows a session of `agent` just started to be, by the
/// rules in force for it.
pub fn judge_startup<S: AsRef<str>>(agent: Agent, screen: &[S]) -> Judgement<StartupReadiness> {
    let reading = in_force(agent).rules.judge_startup(screen);
    Judgement::new(reading.readiness, reading.matched.verdict(screen))
}

/// Whether `screen` shows a dialog of `agent`'s holding the keys, by the
/// rules in force for it.
pub fn dialog_open<S: AsRef<str>>(agent: Agent, screen: &[S]) -> bool {
    in_force(agent).rules.judge(screen).dialog
}

#[cfg(test)]
mod tests;
