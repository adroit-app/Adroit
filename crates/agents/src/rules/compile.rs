//! Rules as written compiled to rules that judge: names turned to the
//! indices they stand for, patterns built, and every limit checked.

use std::collections::HashMap;

use adroit_model::{StartupReadiness, State};
use regex::{Regex, RegexBuilder};

use super::spec::{DialogSpec, File, GateSpec, LineSpec, RegionSpec, RuleSpec};
use super::{Rules, RulesError, refuse};

/// The most rules a list may hold, so that judging every read stays cheap.
pub(crate) const MAX_RULES: usize = 64;
/// The most lines or regions a set may name.
const MAX_NAMES: usize = 64;
/// The longest pattern a line may be written with, in characters.
pub(crate) const MAX_PATTERN: usize = 512;
/// How deep conditions may nest in one another.
pub(crate) const MAX_DEPTH: usize = 8;
/// The most memory one compiled pattern may take.
const MAX_COMPILED: usize = 1 << 20;

#[derive(Debug)]
pub(super) struct Line {
    pub(super) regex: Regex,
    pub(super) next: Option<usize>,
}

#[derive(Debug)]
pub(super) enum Select {
    Last(usize),
    Since(usize),
    After(usize),
    Before(usize),
    From(usize),
    Head(usize),
    Tail(usize),
    TailNonEmpty(usize),
}

#[derive(Debug)]
pub(super) struct Region {
    pub(super) within: Option<usize>,
    pub(super) select: Select,
}

impl Region {
    fn needs(&self) -> impl Iterator<Item = usize> + '_ {
        let selected = match self.select {
            Select::After(r) | Select::Before(r) | Select::From(r) => Some(r),
            _ => None,
        };
        self.within.into_iter().chain(selected)
    }
}

#[derive(Debug, Default)]
pub(super) struct Gate {
    pub(super) holds: Option<usize>,
    pub(super) present: Option<usize>,
    pub(super) absent: Option<usize>,
    pub(super) has: Option<(Option<usize>, usize)>,
    pub(super) all: Vec<Gate>,
    pub(super) any: Vec<Gate>,
    pub(super) not: Option<Box<Gate>>,
}

#[derive(Debug)]
pub(super) struct Rule {
    pub(super) id: String,
    pub(super) state: Option<State>,
    pub(super) startup: Option<StartupReadiness>,
    pub(super) dialog: Option<Gate>,
    pub(super) always_dialog: bool,
    pub(super) default: bool,
    pub(super) when: Option<Gate>,
    pub(super) shows: Option<usize>,
}

/// Turns names into the indices they stand for.
struct Names {
    lines: HashMap<String, usize>,
    regions: HashMap<String, usize>,
    conditions: HashMap<String, usize>,
}

/// The names `map` holds, in order, and the index each is found at.
fn indexed<T>(map: &HashMap<String, T>) -> (Vec<&String>, HashMap<String, usize>) {
    let mut names: Vec<&String> = map.keys().collect();
    names.sort();
    let at = names
        .iter()
        .enumerate()
        .map(|(i, name)| ((*name).clone(), i))
        .collect();
    (names, at)
}

impl Names {
    fn condition(&self, name: &str) -> Result<usize, RulesError> {
        match self.conditions.get(name) {
            Some(&at) => Ok(at),
            None => refuse(format!("there is no condition {name}")),
        }
    }
    fn line(&self, name: &str) -> Result<usize, RulesError> {
        match self.lines.get(name) {
            Some(&at) => Ok(at),
            None => refuse(format!("there is no line {name}")),
        }
    }
    fn region(&self, name: &str) -> Result<usize, RulesError> {
        match self.regions.get(name) {
            Some(&at) => Ok(at),
            None => refuse(format!("there is no region {name}")),
        }
    }
}

impl Rules {
    /// The rules `text` writes for `agent`, or why they cannot be used.
    pub fn parse(agent: &str, text: &str) -> Result<Self, RulesError> {
        let file: File = match toml::from_str(text) {
            Ok(file) => file,
            Err(e) => return refuse(format!("not TOML as rules are written: {e}")),
        };
        if file.agent != agent {
            return refuse(format!(
                "these rules are written for {}, not {agent}",
                file.agent
            ));
        }
        if [file.lines.len(), file.regions.len(), file.conditions.len()]
            .iter()
            .any(|&n| n > MAX_NAMES)
        {
            return refuse(format!(
                "more than {MAX_NAMES} lines, regions or conditions are named"
            ));
        }
        let (line_names, lines) = indexed(&file.lines);
        let (region_names, regions) = indexed(&file.regions);
        let (condition_names, conditions) = indexed(&file.conditions);
        let names = Names {
            lines,
            regions,
            conditions,
        };
        let lines = line_names
            .iter()
            .map(|name| line(name, &file.lines[*name], &names))
            .collect::<Result<Vec<_>, _>>()?;
        let regions = region_names
            .iter()
            .map(|name| region(name, &file.regions[*name], &names))
            .collect::<Result<Vec<_>, _>>()?;
        let order = ordered(&regions, &region_names)?;
        let conditions = condition_names
            .iter()
            .map(|name| {
                compile_gate(&file.conditions[*name], &names, 0, true)
                    .map_err(|RulesError(why)| RulesError(format!("condition {name}: {why}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if file.rules.is_empty() {
            return refuse("there are no rules");
        }
        let state = rules(&file.rules, &names, false)?;
        let startup = rules(&file.startup, &names, true)?;
        Ok(Self {
            agent: file.agent,
            lines,
            regions,
            order,
            conditions,
            state,
            startup,
        })
    }
}

fn line(name: &str, spec: &LineSpec, names: &Names) -> Result<Line, RulesError> {
    let (pattern, next) = match spec {
        LineSpec::Pattern(pattern) => (pattern, None),
        LineSpec::Full { regex, next } => (regex, next.as_deref()),
    };
    if pattern.chars().count() > MAX_PATTERN {
        return refuse(format!(
            "line {name} is written with more than {MAX_PATTERN} characters"
        ));
    }
    let regex = match RegexBuilder::new(pattern).size_limit(MAX_COMPILED).build() {
        Ok(regex) => regex,
        Err(e) => return refuse(format!("line {name} is no pattern: {e}")),
    };
    let next = next.map(|next| names.line(next)).transpose()?;
    Ok(Line { regex, next })
}

fn region(name: &str, spec: &RegionSpec, names: &Names) -> Result<Region, RulesError> {
    let chosen = [
        spec.last.as_ref().map(|l| names.line(l).map(Select::Last)),
        spec.since
            .as_ref()
            .map(|l| names.line(l).map(Select::Since)),
        spec.after
            .as_ref()
            .map(|r| names.region(r).map(Select::After)),
        spec.before
            .as_ref()
            .map(|r| names.region(r).map(Select::Before)),
        spec.from
            .as_ref()
            .map(|r| names.region(r).map(Select::From)),
        spec.head.map(|n| Ok(Select::Head(n))),
        spec.tail.map(|n| Ok(Select::Tail(n))),
        spec.tail_non_empty.map(|n| Ok(Select::TailNonEmpty(n))),
    ];
    let mut chosen = chosen.into_iter().flatten();
    let (Some(select), None) = (chosen.next(), chosen.next()) else {
        return refuse(format!(
            "region {name} must choose its rows by exactly one of last, since, after, before, from, head, tail and tail_non_empty"
        ));
    };
    Ok(Region {
        within: spec.within.as_ref().map(|r| names.region(r)).transpose()?,
        select: select?,
    })
}

/// The regions in an order where each follows those it is built on.
fn ordered(regions: &[Region], names: &[&String]) -> Result<Vec<usize>, RulesError> {
    fn visit(
        at: usize,
        regions: &[Region],
        names: &[&String],
        marks: &mut [u8],
        order: &mut Vec<usize>,
    ) -> Result<(), RulesError> {
        match marks[at] {
            2 => return Ok(()),
            1 => return refuse(format!("region {} refers to itself", names[at])),
            _ => {}
        }
        marks[at] = 1;
        for need in regions[at].needs() {
            visit(need, regions, names, marks, order)?;
        }
        marks[at] = 2;
        order.push(at);
        Ok(())
    }
    let mut marks = vec![0u8; regions.len()];
    let mut order = Vec::with_capacity(regions.len());
    for at in 0..regions.len() {
        visit(at, regions, names, &mut marks, &mut order)?;
    }
    Ok(order)
}

fn rules(specs: &[RuleSpec], names: &Names, startup: bool) -> Result<Vec<Rule>, RulesError> {
    if specs.len() > MAX_RULES {
        return refuse(format!("a list holds more than {MAX_RULES} rules"));
    }
    let mut seen = std::collections::HashSet::new();
    let mut rules = Vec::with_capacity(specs.len());
    for spec in specs {
        if !seen.insert(spec.id.as_str()) {
            return refuse(format!("rule {} is written twice", spec.id));
        }
        if startup && (spec.state.is_some() || spec.dialog.is_some()) {
            return refuse(format!(
                "startup rule {} sets a state or a dialog; a startup rule sets only startup",
                spec.id
            ));
        }
        let when = spec
            .when
            .as_ref()
            .map(|gate| compile_gate(gate, names, 0, false))
            .transpose()
            .map_err(|RulesError(why)| RulesError(format!("rule {}: {why}", spec.id)))?;
        rules.push(Rule {
            id: spec.id.clone(),
            state: spec.state.map(State::from),
            startup: spec.startup.map(StartupReadiness::from),
            dialog: match &spec.dialog {
                Some(DialogSpec::While(gate)) => Some(
                    compile_gate(gate, names, 0, false).map_err(|RulesError(why)| {
                        RulesError(format!("rule {}: dialog: {why}", spec.id))
                    })?,
                ),
                _ => None,
            },
            always_dialog: matches!(spec.dialog, Some(DialogSpec::Always(true))),
            default: spec.default,
            when,
            shows: spec
                .shows
                .as_ref()
                .map(|r| names.region(r))
                .transpose()
                .map_err(|RulesError(why)| RulesError(format!("rule {}: {why}", spec.id)))?,
        });
    }
    if let Some(last) = specs.last()
        && last.when.is_some()
    {
        return refuse(format!(
            "the last rule, {}, must hold on any screen, so it takes no when",
            last.id
        ));
    }
    Ok(rules)
}

/// `spec` with its names turned to indices; `named` while compiling a named
/// condition, which may not name another, so that none can name itself.
fn compile_gate(
    spec: &GateSpec,
    names: &Names,
    depth: usize,
    named: bool,
) -> Result<Gate, RulesError> {
    if depth > MAX_DEPTH {
        return refuse(format!("its conditions nest more than {MAX_DEPTH} deep"));
    }
    if named && spec.holds.is_some() {
        return refuse("a named condition names another");
    }
    let has = match (&spec.within, &spec.has) {
        (Some(_), None) => return refuse("a condition names in without has"),
        (within, Some(has)) => Some((
            within.as_ref().map(|r| names.region(r)).transpose()?,
            names.line(has)?,
        )),
        (None, None) => None,
    };
    let gate = Gate {
        holds: spec
            .holds
            .as_ref()
            .map(|c| names.condition(c))
            .transpose()?,
        present: spec.present.as_ref().map(|r| names.region(r)).transpose()?,
        absent: spec.absent.as_ref().map(|r| names.region(r)).transpose()?,
        has,
        all: spec
            .all
            .iter()
            .map(|g| compile_gate(g, names, depth + 1, named))
            .collect::<Result<_, _>>()?,
        any: spec
            .any
            .iter()
            .map(|g| compile_gate(g, names, depth + 1, named))
            .collect::<Result<_, _>>()?,
        not: spec
            .not
            .as_ref()
            .map(|g| compile_gate(g, names, depth + 1, named).map(Box::new))
            .transpose()?,
    };
    if gate.holds.is_none()
        && gate.present.is_none()
        && gate.absent.is_none()
        && gate.has.is_none()
        && gate.all.is_empty()
        && gate.any.is_empty()
        && gate.not.is_none()
    {
        return refuse("a condition names no condition");
    }
    Ok(gate)
}
