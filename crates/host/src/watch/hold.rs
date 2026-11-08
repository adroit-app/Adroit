//! How a session read from its screen stands from one read to the next: a
//! spinner missed by one read must not turn a working session idle, and
//! then done, so idle without a sign of its own waits for a second read.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use adroit_agents::Judgement;
use adroit_model::State;

/// What a session read from its screen was last shown as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shown {
    pub state: State,
    /// Whether its screen read idle with no sign of it, and it is shown
    /// working until the next read says so too.
    pub held: bool,
}

/// What a session last shown as `last` is shown as now its screen reads as
/// `read`; none while its screen tells nothing.
pub(crate) fn settle(last: Option<&Shown>, read: &Judgement<State>) -> Option<Shown> {
    let state = read.value.clone()?;
    let unsigned_idle = state == State::Idle && read.verdict.default;
    Some(match last {
        Some(Shown {
            state: State::Working,
            held: false,
        }) if unsigned_idle => Shown {
            state: State::Working,
            held: true,
        },
        _ => Shown { state, held: false },
    })
}

/// What each session read from its screen was last shown as, by its id in
/// the terminal host.
static SHOWN: LazyLock<Mutex<HashMap<u128, Shown>>> = LazyLock::new(Default::default);

/// What the session `id`, whose screen reads as `read`, is shown as now,
/// remembered for its next read; none when it has no screen read.
pub(crate) fn settled(id: u128, read: Option<&Judgement<State>>) -> Option<State> {
    let mut shown = SHOWN.lock().unwrap_or_else(|e| e.into_inner());
    let now = read.and_then(|read| settle(shown.get(&id), read));
    match &now {
        Some(now) => shown.insert(id, now.clone()),
        None => shown.remove(&id),
    };
    now.map(|now| now.state)
}

/// Forgets every session but those `kept`.
pub(crate) fn keep_only(kept: &[u128]) {
    let mut shown = SHOWN.lock().unwrap_or_else(|e| e.into_inner());
    shown.retain(|id, _| kept.contains(id));
}

/// What the session `id` was last shown as by its screen.
pub(crate) fn shown(id: u128) -> Option<Shown> {
    let shown = SHOWN.lock().unwrap_or_else(|e| e.into_inner());
    shown.get(&id).cloned()
}

#[cfg(test)]
mod tests {
    use adroit_agents::{Judgement, Verdict};
    use adroit_model::{State, Wait};

    use super::*;

    fn read(state: Option<State>, default: bool) -> Judgement<State> {
        let verdict = match default {
            true => Verdict::defaulted("t.default", &[""], []),
            false => Verdict::matched("t.sign", &[""], []),
        };
        Judgement::new(state, verdict)
    }

    /// The states shown as the screen reads `reads` in turn.
    fn shown(reads: &[Judgement<State>]) -> Vec<Option<State>> {
        let mut last = None;
        reads
            .iter()
            .map(|r| {
                last = settle(last.as_ref(), r);
                last.as_ref().map(|s| s.state.clone())
            })
            .collect()
    }

    const WORKING: Option<State> = Some(State::Working);
    const IDLE: Option<State> = Some(State::Idle);

    #[test]
    fn idle_with_no_sign_of_it_is_shown_only_once_a_second_read_agrees() {
        let working = read(WORKING, false);
        let idle = read(IDLE, true);
        assert_eq!(
            shown(&[working.clone(), idle.clone(), working.clone()]),
            [WORKING, WORKING, WORKING]
        );
        assert_eq!(
            shown(&[working.clone(), idle.clone(), idle.clone()]),
            [WORKING, WORKING, IDLE]
        );
        assert_eq!(
            shown(&[working.clone(), idle.clone(), working, idle.clone()]),
            [WORKING, WORKING, WORKING, WORKING]
        );
    }

    #[test]
    fn idle_with_a_sign_of_its_own_is_shown_at_once() {
        assert_eq!(
            shown(&[read(WORKING, false), read(IDLE, false)]),
            [WORKING, IDLE]
        );
    }

    #[test]
    fn anything_but_idle_after_work_is_shown_at_once() {
        let waiting = Some(State::Waiting(Wait::Permission));
        assert_eq!(
            shown(&[read(WORKING, false), read(waiting.clone(), false)]),
            [WORKING, waiting]
        );
        assert_eq!(shown(&[read(IDLE, true)]), [IDLE]);
        assert_eq!(
            shown(&[read(WORKING, false), read(None, true), read(IDLE, true)]),
            [WORKING, None, IDLE]
        );
    }
}
