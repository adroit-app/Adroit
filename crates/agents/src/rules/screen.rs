//! Reading a screen with compiled rules: the regions found on it, and the
//! first rule that holds.

use std::ops::Range;

use super::compile::{Gate, Region, Rule, Select};
use super::{Answers, Matched, Offered, Reading, Rules, Shown, StartupReading};

impl Rules {
    /// How `screen` shows the agent to stand, by the first rule that holds.
    pub fn judge<S: AsRef<str>>(&self, screen: &[S]) -> Reading {
        let screen = Screen::new(self, screen);
        let (rule, matched) = self.first(&self.state, &screen);
        Reading {
            state: rule.state.clone(),
            dialog: screen.dialog(rule),
            matched,
        }
    }

    /// How the set reads a dialog asking the user; none when it reads none.
    pub fn answers(&self) -> Option<&Answers> {
        self.answers.as_ref()
    }

    /// The dialog `screen` shows holding the keys, read where the set's
    /// `[answers]` say it asks; none when no dialog holds the keys, the
    /// region is found nowhere, or the set reads no dialog so.
    pub fn shown<S: AsRef<str>>(&self, screen: &[S]) -> Option<Shown> {
        let answers = self.answers.as_ref()?;
        let screen = Screen::new(self, screen);
        let (rule, _) = self.first(&self.state, &screen);
        if !screen.dialog(rule) {
            return None;
        }
        let rows: Vec<String> = screen.regions[answers.within]
            .clone()?
            .map(|at| screen.row(at).to_string())
            .collect();
        let options = rows
            .iter()
            .filter_map(|row| {
                let found = answers.option.captures(row)?;
                Some(Offered {
                    key: found.name("key")?.as_str().to_owned(),
                    label: found.name("label")?.as_str().to_owned(),
                })
            })
            .collect();
        Some(Shown { rows, options })
    }

    /// How far `screen` shows a session just started to be: by the startup
    /// rules if the set has any, else by the state rule that holds, as its
    /// `startup` says or as its state implies.
    pub fn judge_startup<S: AsRef<str>>(&self, screen: &[S]) -> StartupReading {
        let screen = Screen::new(self, screen);
        let (rule, matched) = match self.startup.is_empty() {
            true => self.first(&self.state, &screen),
            false => self.first(&self.startup, &screen),
        };
        let readiness = rule
            .startup
            .or_else(|| rule.state.clone().and_then(crate::readiness));
        StartupReading { readiness, matched }
    }

    fn first<'r, S: AsRef<str>>(
        &self,
        rules: &'r [Rule],
        reading: &Screen<'_, S>,
    ) -> (&'r Rule, Matched) {
        let mut evidence = Vec::new();
        let rule = rules
            .iter()
            .find(|rule| {
                evidence.clear();
                rule.when
                    .as_ref()
                    .is_none_or(|gate| reading.holds(gate, &mut evidence))
            })
            .expect("the last rule holds on any screen");
        if let Some(range) = rule.shows.and_then(|r| reading.regions[r].clone()) {
            evidence.extend(range.map(|at| (at, reading.row(at).to_string())));
        }
        let matched = Matched {
            rule: format!("{}.{}", self.agent, rule.id),
            default: rule.default,
            evidence,
        };
        (rule, matched)
    }
}

/// A screen being judged, with the regions found on it.
struct Screen<'a, S> {
    rules: &'a Rules,
    rows: &'a [S],
    regions: Vec<Option<Range<usize>>>,
}

impl<'a, S: AsRef<str>> Screen<'a, S> {
    fn new(rules: &'a Rules, rows: &'a [S]) -> Self {
        let mut screen = Self {
            rules,
            rows,
            regions: vec![None; rules.regions.len()],
        };
        for &at in &rules.order {
            screen.regions[at] = screen.find(&rules.regions[at]);
        }
        screen
    }

    fn row(&self, at: usize) -> &str {
        self.rows[at].as_ref()
    }

    /// Whether a dialog holds the keys while `rule` decides.
    fn dialog(&self, rule: &Rule) -> bool {
        rule.always_dialog
            || rule
                .dialog
                .as_ref()
                .is_some_and(|gate| self.holds(gate, &mut Vec::new()))
    }

    fn is(&self, line: usize, at: usize) -> bool {
        let line = &self.rules.lines[line];
        line.regex.is_match(self.row(at))
            && line
                .next
                .is_none_or(|next| at + 1 < self.rows.len() && self.is(next, at + 1))
    }

    fn last(&self, line: usize, within: &Range<usize>) -> Option<usize> {
        within.clone().rev().find(|&at| self.is(line, at))
    }

    fn find(&self, region: &Region) -> Option<Range<usize>> {
        let base = match region.within {
            Some(within) => self.regions[within].clone()?,
            None => 0..self.rows.len(),
        };
        let clamp = |at: usize| at.clamp(base.start, base.end);
        Some(match region.select {
            Select::First(line) => base
                .clone()
                .find(|&at| self.is(line, at))
                .map(|at| at..at + 1)?,
            Select::Last(line) => self.last(line, &base).map(|at| at..at + 1)?,
            Select::Since(line) => match self.last(line, &base) {
                Some(at) => at + 1..base.end,
                None => base,
            },
            Select::After(r) => clamp(self.regions[r].as_ref()?.end)..base.end,
            Select::Before(r) => base.start..clamp(self.regions[r].as_ref()?.start),
            Select::From(r) => clamp(self.regions[r].as_ref()?.start)..base.end,
            Select::Head(n) => base.start..clamp(base.start.saturating_add(n)),
            Select::Tail(n) => clamp(base.end.saturating_sub(n))..base.end,
            Select::TailNonEmpty(n) => {
                let drawn = base
                    .clone()
                    .rev()
                    .filter(|&at| !self.row(at).trim().is_empty())
                    .take(n)
                    .last();
                drawn.unwrap_or(base.end)..base.end
            }
        })
    }

    /// Whether `gate` holds, adding to `evidence` the rows its signs matched.
    fn holds(&self, gate: &Gate, evidence: &mut Vec<(usize, String)>) -> bool {
        let kept = evidence.len();
        let held = self.holds_all(gate, evidence);
        if !held {
            evidence.truncate(kept);
        }
        held
    }

    fn holds_all(&self, gate: &Gate, evidence: &mut Vec<(usize, String)>) -> bool {
        if gate
            .holds
            .is_some_and(|c| !self.holds(&self.rules.conditions[c], evidence))
        {
            return false;
        }
        if gate.present.is_some_and(|r| self.regions[r].is_none())
            || gate.absent.is_some_and(|r| self.regions[r].is_some())
        {
            return false;
        }
        if let Some((within, line)) = gate.has {
            let within = match within {
                Some(r) => self.regions[r].clone(),
                None => Some(0..self.rows.len()),
            };
            match within.and_then(|within| self.last(line, &within)) {
                Some(at) => evidence.push((at, self.row(at).to_string())),
                None => return false,
            }
        }
        gate.all.iter().all(|g| self.holds(g, evidence))
            && (gate.any.is_empty() || gate.any.iter().any(|g| self.holds(g, evidence)))
            && gate
                .not
                .as_ref()
                .is_none_or(|g| !self.holds(g, &mut Vec::new()))
    }
}
