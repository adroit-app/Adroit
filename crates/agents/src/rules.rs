//! The rules that read an agent's screen, written as data: which lines on
//! the screen mean something, the regions found by them, and the rules,
//! tried in order, that tell from them how the agent stands, whether a
//! dialog holds the keys, and whether it is ready at startup.

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::SystemTime;

use adroit_model::{Agent, StartupReadiness, State, Wait};
use regex::{Regex, RegexBuilder};

use crate::{Judgement, Verdict};
use serde::Deserialize;

/// The rules Adroit carries, one set an agent.
pub(crate) const BUNDLED: &[(Agent, &str)] = &[
    (Agent::Claude, include_str!("../rules/claude.toml")),
    (Agent::Codex, include_str!("../rules/codex.toml")),
    (Agent::Pi, include_str!("../rules/pi.toml")),
    (Agent::OpenCode, include_str!("../rules/opencode.toml")),
];

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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    agent: String,
    #[serde(default)]
    lines: HashMap<String, LineSpec>,
    #[serde(default)]
    regions: HashMap<String, RegionSpec>,
    #[serde(default)]
    conditions: HashMap<String, GateSpec>,
    #[serde(default)]
    rules: Vec<RuleSpec>,
    #[serde(default)]
    startup: Vec<RuleSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum LineSpec {
    Pattern(String),
    Full { regex: String, next: Option<String> },
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegionSpec {
    within: Option<String>,
    last: Option<String>,
    since: Option<String>,
    after: Option<String>,
    before: Option<String>,
    from: Option<String>,
    head: Option<usize>,
    tail: Option<usize>,
    tail_non_empty: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleSpec {
    id: String,
    state: Option<StateName>,
    startup: Option<ReadinessName>,
    dialog: Option<DialogSpec>,
    #[serde(default)]
    default: bool,
    when: Option<GateSpec>,
    shows: Option<String>,
}

/// Whether a dialog holds the keys while a rule decides: always, never, or
/// while a condition of its own holds.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DialogSpec {
    Always(bool),
    While(GateSpec),
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct GateSpec {
    holds: Option<String>,
    present: Option<String>,
    absent: Option<String>,
    #[serde(rename = "in")]
    within: Option<String>,
    has: Option<String>,
    #[serde(default)]
    all: Vec<GateSpec>,
    #[serde(default)]
    any: Vec<GateSpec>,
    not: Option<Box<GateSpec>>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum StateName {
    Working,
    Idle,
    WaitingInput,
    WaitingPermission,
}

impl From<StateName> for State {
    fn from(name: StateName) -> Self {
        match name {
            StateName::Working => State::Working,
            StateName::Idle => State::Idle,
            StateName::WaitingInput => State::Waiting(Wait::Input),
            StateName::WaitingPermission => State::Waiting(Wait::Permission),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ReadinessName {
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

#[derive(Debug)]
struct Line {
    regex: Regex,
    next: Option<usize>,
}

#[derive(Debug)]
enum Select {
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
struct Region {
    within: Option<usize>,
    select: Select,
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
struct Gate {
    holds: Option<usize>,
    present: Option<usize>,
    absent: Option<usize>,
    has: Option<(Option<usize>, usize)>,
    all: Vec<Gate>,
    any: Vec<Gate>,
    not: Option<Box<Gate>>,
}

#[derive(Debug)]
struct Rule {
    id: String,
    state: Option<State>,
    startup: Option<StartupReadiness>,
    dialog: Option<Gate>,
    always_dialog: bool,
    default: bool,
    when: Option<Gate>,
    shows: Option<usize>,
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
    /// The rules Adroit carries for `agent`, if it reads that agent's screen.
    pub fn bundled(agent: Agent) -> Option<Self> {
        let (_, text) = BUNDLED.iter().find(|(a, _)| *a == agent)?;
        Some(
            Self::parse(agent.key(), text)
                .unwrap_or_else(|e| panic!("the rules bundled for {}: {e}", agent.key())),
        )
    }

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

    /// How `screen` shows the agent to stand, by the first rule that holds.
    pub fn judge<S: AsRef<str>>(&self, screen: &[S]) -> Reading {
        let screen = Screen::new(self, screen);
        let (rule, matched) = self.first(&self.state, &screen);
        let dialog = rule.always_dialog
            || rule
                .dialog
                .as_ref()
                .is_some_and(|gate| screen.holds(gate, &mut Vec::new()));
        Reading {
            state: rule.state.clone(),
            dialog,
            matched,
        }
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
