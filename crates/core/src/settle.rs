//! What must hold across slices after every message: each rule brings one
//! slice in line with the others, and runs in the order listed.

use adroit_protocol::{Request, Sight};

use crate::effect::Effect;
use crate::state::ViewState;
use crate::{details, tree};

type Rule = fn(&mut ViewState, &mut Vec<Effect>);

const RULES: &[Rule] = &[
    tree_failing,
    tree_rows,
    tree_successor,
    tree_kept,
    tree_switched,
    details_shown,
    details_refreshed,
    details_gone,
    watched,
];

/// The status line says why the sessions cannot be listed, while they
/// cannot.
fn tree_failing(state: &mut ViewState, effects: &mut Vec<Effect>) {
    let work = state.host.work.as_ref();
    effects.extend(tree::tell_failing(
        &mut state.tree,
        &mut state.chrome.status,
        work,
    ));
}

/// The tree's rows are built from what the host listed.
fn tree_rows(state: &mut ViewState, _: &mut Vec<Effect>) {
    tree::build(&mut state.tree, state.host.work.as_ref());
}

/// The selected row follows its session once it is taken over.
fn tree_successor(state: &mut ViewState, _: &mut Vec<Effect>) {
    tree::follow_successor(&mut state.tree, state.host.work.as_ref());
}

/// A selected row that leaves the rows gives way to another.
fn tree_kept(state: &mut ViewState, _: &mut Vec<Effect>) {
    tree::keep_selection(&mut state.tree, state.host.work.as_ref());
}

/// The selection follows the focused window to what it now shows.
fn tree_switched(state: &mut ViewState, _: &mut Vec<Effect>) {
    tree::follow_switch(&mut state.tree, state.host.work.as_ref());
}

/// The details panel reads by the session and the worktree the focused
/// window shows.
fn details_shown(state: &mut ViewState, _: &mut Vec<Effect>) {
    details::follow(&mut state.details, &state.tree.showing);
}

/// What the host read again for the spot the details panel shows moves its
/// selection along, or steps back from a row opened that is gone.
fn details_refreshed(state: &mut ViewState, _: &mut Vec<Effect>) {
    details::refresh(&mut state.details, state.host.work.as_ref());
}

/// A subagent or process opened that the overview no longer lists is gone
/// back from.
fn details_gone(state: &mut ViewState, _: &mut Vec<Effect>) {
    let work = state.host.work.as_ref().map(|w| &w.details);
    details::forget_gone(&mut state.details, work);
}

/// The host keeps read what the tree and the details panel show, asked
/// once each time it changes.
fn watched(state: &mut ViewState, effects: &mut Vec<Effect>) {
    let sight = sight(state);
    if state.host.watched == sight {
        return;
    }
    state.host.watched = sight.clone();
    effects.push(Effect::Host(Request::Watch(Box::new(sight))));
}

/// What the window shows that the host keeps read.
pub(crate) fn sight(state: &ViewState) -> Sight {
    Sight {
        tree: tree::sight(&state.tree),
        ..details::sight(&state.details)
    }
}

/// `state` with every rule applied once, and what the rules ask done.
pub fn settle(state: &mut ViewState) -> Vec<Effect> {
    let mut effects = Vec::new();
    for rule in RULES {
        rule(state, &mut effects);
    }
    effects
}
