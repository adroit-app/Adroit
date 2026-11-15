//! What the keys help lists: every binding in force, area by area, the areas
//! the keys are in first, and what a key does while it is shown.

use std::mem::discriminant;

use adroit_model::Key;

use crate::actions::Action;
use crate::icons::Icon;
use crate::keymap::{Context, Keymap, Step};

/// One area's bindings.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub context: Context,
    /// Whether the keys are in the area now.
    pub here: bool,
    pub rows: Vec<Row>,
}

/// One action of an area, with every sequence bound to it there.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub keys: Vec<Keys>,
    pub label: String,
    pub icon: Option<Icon>,
    /// Whether the last key of its sequences runs the action again alone.
    pub repeats: bool,
}

/// Sequences of a row that start alike: the keys they share, then the last
/// key of each, as in `Ctrl+q` `h` / `ArrowLeft`. A single key shares
/// nothing and stands alone.
#[derive(Debug, Clone, PartialEq)]
pub struct Keys {
    pub lead: Vec<String>,
    pub ends: Vec<String>,
}

/// The groups of `keymap`'s bindings, the areas of `here` first and in
/// their order, then every other area in the order the file names them. An
/// area that binds nothing is left out.
pub fn groups(keymap: &Keymap, here: &[Context]) -> Vec<Group> {
    let listed = keymap.listed();
    let rest = Context::ALL.into_iter().filter(|c| !here.contains(c));
    here.iter()
        .copied()
        .chain(rest)
        .filter_map(|ctx| {
            let mut bound: Vec<(Action, Vec<String>)> = Vec::new();
            for (area, keys, name) in &listed {
                let Ok(action) = name.parse::<Action>() else {
                    continue;
                };
                if *area != ctx.name() {
                    continue;
                }
                match bound.iter_mut().find(|(a, _)| *a == action) {
                    Some((_, seqs)) => seqs.push(keys.clone()),
                    None => bound.push((action, vec![keys.clone()])),
                }
            }
            let labelled = bound
                .into_iter()
                .map(|(action, seqs)| {
                    let label = action.label();
                    (action, label, seqs)
                })
                .collect();
            let folded = NUMBERED.iter().fold(labelled, fold_numbers);
            let rows: Vec<Row> = folded
                .into_iter()
                .map(|(action, label, seqs)| Row {
                    keys: share_leads(&seqs),
                    label,
                    icon: action.icon(),
                    repeats: action.steps().is_some(),
                })
                .collect();
            (!rows.is_empty()).then(|| Group {
                context: ctx,
                here: here.contains(&ctx),
                rows,
            })
        })
        .collect()
}

/// The actions that take a tab's number, each standing for every number.
const NUMBERED: &[Action] = &[Action::TabGo(1), Action::TabMerge(1)];

/// The tab's number `action` names, when it is of the same kind as `kind`.
fn number_of(action: &Action, kind: &Action) -> Option<u8> {
    action
        .number()
        .filter(|_| discriminant(action) == discriminant(kind))
}

/// `bound` with the sequences of the actions of `kind` that differ only in a
/// run of numbers ending them folded into one row, `Ctrl+q 1–9`, where the
/// first of those numbers stood.
fn fold_numbers(
    bound: Vec<(Action, String, Vec<String>)>,
    kind: &Action,
) -> Vec<(Action, String, Vec<String>)> {
    let mut heads: Vec<(&str, Vec<u8>)> = Vec::new();
    for (action, _, seqs) in &bound {
        let Some(n) = number_of(action, kind) else {
            continue;
        };
        for head in seqs.iter().filter_map(|s| before_number(s, n)) {
            match heads.iter_mut().find(|(h, _)| *h == head) {
                Some((_, ns)) => ns.push(n),
                None => heads.push((head, vec![n])),
            }
        }
    }
    let mut runs = Vec::new();
    let mut taken: Vec<(String, u8)> = Vec::new();
    let (mut lo, mut hi) = (u8::MAX, 0);
    for (head, mut ns) in heads {
        ns.sort_unstable();
        for run in ns.chunk_by(|a, b| b - a == 1) {
            if let [first, .., last] = run {
                runs.push(format!("{head}{first}–{last}"));
                taken.extend(run.iter().map(|n| (head.to_string(), *n)));
                (lo, hi) = (lo.min(*first), hi.max(*last));
            }
        }
    }
    let mut runs = Some(runs).filter(|r| !r.is_empty());
    let mut rows = Vec::new();
    for (action, label, seqs) in bound {
        let Some(n) = number_of(&action, kind) else {
            rows.push((action, label, seqs));
            continue;
        };
        if let Some(runs) = runs.take() {
            rows.push((
                action.clone(),
                format!("{}–{hi}", kind.numbered(lo).label()),
                runs,
            ));
        }
        let seqs: Vec<String> = seqs
            .into_iter()
            .filter(|s| !before_number(s, n).is_some_and(|h| taken.contains(&(h.to_string(), n))))
            .collect();
        if !seqs.is_empty() {
            rows.push((action, label, seqs));
        }
    }
    rows
}

/// What `seq` presses before the number `n` its last key is, when it ends
/// in that number: `Ctrl+q ` of `Ctrl+q 3`, `Cmd+` of `Cmd+3`.
fn before_number(seq: &str, n: u8) -> Option<&str> {
    let head = seq.strip_suffix(char::from(b'0' + n))?;
    (head.is_empty() || head.ends_with([' ', '+'])).then_some(head)
}

/// `seqs` with the ones that start with the same keys written as one
/// [`Keys`], in the order the first of each stands.
fn share_leads(seqs: &[String]) -> Vec<Keys> {
    let mut keys: Vec<Keys> = Vec::new();
    for seq in seqs {
        let (lead, end) = match seq.rsplit_once(' ') {
            Some((lead, end)) => (lead.split(' ').map(str::to_string).collect(), end),
            None => (Vec::new(), seq.as_str()),
        };
        match keys.iter_mut().find(|k| !lead.is_empty() && k.lead == lead) {
            Some(k) => k.ends.push(end.to_string()),
            None => keys.push(Keys {
                lead,
                ends: vec![end.to_string()],
            }),
        }
    }
    keys
}

/// What a key does while the keys help is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Close,
    /// Scroll by this many lines, down when positive.
    Scroll(i32),
    Nothing,
}

/// What `key` does while the keys help is shown, opened from `contexts`.
pub fn press(keymap: &Keymap, contexts: &[Context], key: &Key) -> Press {
    let opens =
        |contexts: &[Context]| keymap.resolve(contexts, &[], key) == Step::Run(Action::KeysHelp);
    if opens(contexts) || opens(&[Context::Global]) {
        return Press::Close;
    }
    let plain = !key.ctrl && !key.alt && !key.meta;
    let ctrl = key.ctrl && !key.alt && !key.meta;
    match key.key.as_str() {
        "Escape" | "q" if plain => Press::Close,
        "j" | "ArrowDown" if plain => Press::Scroll(1),
        "k" | "ArrowUp" if plain => Press::Scroll(-1),
        "d" if ctrl => Press::Scroll(10),
        "u" if ctrl => Press::Scroll(-10),
        _ => Press::Nothing,
    }
}

#[cfg(test)]
mod tests;
