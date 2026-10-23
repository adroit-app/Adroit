//! What must hold across slices after every message: each rule brings one
//! slice in line with the others, and runs in the order listed.

use crate::effect::Effect;
use crate::state::ViewState;

type Rule = fn(&mut ViewState, &mut Vec<Effect>);

const RULES: &[Rule] = &[];

/// `state` with every rule applied once, and what the rules ask done.
pub fn settle(state: &mut ViewState) -> Vec<Effect> {
    let mut effects = Vec::new();
    for rule in RULES {
        rule(state, &mut effects);
    }
    effects
}
