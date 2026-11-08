//! What Codex's screen says of it: the composer it waits at, the dialogs
//! and pickers drawn under it, and the hint to interrupt a turn running, as
//! the rules in force for Codex read them.

use adroit_model::{Agent, State};

use crate::Judgement;

pub(super) fn dialog_open(screen: &[String]) -> bool {
    crate::rules::dialog_open(Agent::Codex, screen)
}

pub(super) fn judge_state(screen: &[String]) -> Judgement<State> {
    crate::rules::judge_state(Agent::Codex, screen)
}
