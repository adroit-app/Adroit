//! The rules that read an agent's screen, written as data: which lines on
//! the screen mean something, the regions found by them, and the rules,
//! tried in order, that tell from them how the agent stands, whether a
//! dialog holds the keys, and whether it is ready at startup.

mod compile;
mod load;
mod screen;
mod spec;

use adroit_model::{Agent, StartupReadiness, State};

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
