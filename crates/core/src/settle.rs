//! What must hold across slices after every message: each rule brings one
//! slice in line with the others, and runs in the order listed.

use crate::details;
use crate::effect::Effect;
use crate::state::ViewState;

type Rule = fn(&mut ViewState, &mut Vec<Effect>);

const RULES: &[Rule] = &[details_refreshed, details_gone, details_watched];

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

/// The host keeps read what the details panel shows.
fn details_watched(state: &mut ViewState, effects: &mut Vec<Effect>) {
    effects.extend(details::watch(&mut state.details));
}

/// `state` with every rule applied once, and what the rules ask done.
pub fn settle(state: &mut ViewState) -> Vec<Effect> {
    let mut effects = Vec::new();
    for rule in RULES {
        rule(state, &mut effects);
    }
    effects
}
