//! How much each session needs the user: the order of its state as shown,
//! and when the window last saw that state change.

use std::collections::{BTreeMap, BTreeSet};

use adroit_model::{EnvironmentId, State, Tree};

/// The words of the states as shown, the one that needs the user most first:
/// one waiting on the user, one finished the user has not seen, one that
/// failed, then those still at work, those whose work runs behind a turn that
/// ended, and last those with nothing to say.
pub const ORDER: [&str; 7] = [
    "waiting",
    "done",
    "failed",
    "working",
    "background",
    "starting",
    "idle",
];

/// Where `word`, a state as shown, stands in [`ORDER`]; past every state for
/// a word that is not one.
pub fn rank(word: &str) -> usize {
    ORDER.iter().position(|w| *w == word).unwrap_or(ORDER.len())
}

/// The state whose word starts with `prefix`, the first of [`ORDER`] that
/// does; every state for no prefix at all.
pub fn named(prefix: &str) -> Vec<&'static str> {
    let prefix = prefix.to_lowercase();
    if prefix.is_empty() {
        return ORDER.to_vec();
    }
    ORDER
        .iter()
        .find(|w| w.starts_with(&prefix))
        .into_iter()
        .copied()
        .collect()
}

/// When the window saw each session's state last change, counted in the
/// changes it has seen: a later change counts higher.
///
/// The window knows only what it has seen since it opened, so the sessions
/// listed when it first hears from a host count as never changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    last: BTreeMap<(EnvironmentId, String), (State, u64)>,
    heard: BTreeSet<EnvironmentId>,
    count: u64,
}

impl Changes {
    /// Take in the sessions `environment`'s host lists as `tree`: one whose
    /// state differs from the last seen, or that came after the host was
    /// first heard from, counts as changed now.
    pub fn see(&mut self, environment: &EnvironmentId, tree: &Tree) {
        let first = self.heard.insert(environment.clone());
        let mut listed = BTreeSet::new();
        for (_, s) in tree.sessions() {
            let key = (environment.clone(), s.id.clone());
            let changed = match self.last.get(&key) {
                Some((state, _)) => *state != s.state,
                None => !first,
            };
            if changed {
                self.count += 1;
                self.last.insert(key.clone(), (s.state.clone(), self.count));
            } else if !self.last.contains_key(&key) {
                self.last.insert(key.clone(), (s.state.clone(), 0));
            }
            listed.insert(key);
        }
        self.last
            .retain(|key, _| key.0 != *environment || listed.contains(key));
    }

    /// When `id` of `environment` was last seen to change; 0 for one never
    /// seen to.
    pub fn last(&self, environment: &EnvironmentId, id: &str) -> u64 {
        self.last
            .get(&(environment.clone(), id.to_string()))
            .map_or(0, |(_, at)| *at)
    }
}

#[cfg(test)]
mod tests {
    use adroit_model::Wait;
    use adroit_model::testing::{self, session};

    use super::*;

    fn tree(states: &[(&str, State)]) -> Tree {
        states
            .iter()
            .fold(testing::tree(), |t, (id, state)| {
                t.session(session(*id).cwd("/r").state(state.clone()))
            })
            .build()
    }

    fn here() -> EnvironmentId {
        EnvironmentId::default()
    }

    #[test]
    fn the_states_stand_waiting_then_done_failed_working_background_starting_and_idle_last() {
        let words: Vec<&str> = [
            State::Idle,
            State::Starting,
            State::Background,
            State::Working,
            State::Failed,
            State::Done,
            State::Waiting(Wait::Input),
        ]
        .iter()
        .map(State::word)
        .collect();
        let mut sorted = words.clone();
        sorted.sort_by_key(|w| rank(w));
        assert_eq!(
            sorted,
            [
                "waiting",
                "done",
                "failed",
                "working",
                "background",
                "starting",
                "idle"
            ]
        );
        assert_eq!(rank("no such state"), ORDER.len());
    }

    #[test]
    fn a_prefix_names_the_first_state_in_that_order_whose_word_starts_with_it() {
        for (prefix, states) in [
            ("w", &["waiting"][..]),
            ("wa", &["waiting"]),
            ("wo", &["working"]),
            ("W", &["waiting"]),
            ("d", &["done"]),
            ("f", &["failed"]),
            ("b", &["background"]),
            ("s", &["starting"]),
            ("i", &["idle"]),
            ("idle", &["idle"]),
            ("x", &[]),
            ("idler", &[]),
        ] {
            assert_eq!(named(prefix), states, "{prefix:?}");
        }
        assert_eq!(named(""), ORDER);
    }

    #[test]
    fn the_sessions_listed_when_a_host_is_first_heard_from_count_as_never_changed() {
        let mut changes = Changes::default();
        changes.see(&here(), &tree(&[("a", State::Working), ("b", State::Done)]));
        assert_eq!(changes.last(&here(), "a"), 0);
        assert_eq!(changes.last(&here(), "b"), 0);
    }

    #[test]
    fn a_state_seen_to_change_counts_after_every_change_seen_before_it() {
        let mut changes = Changes::default();
        changes.see(
            &here(),
            &tree(&[("a", State::Working), ("b", State::Working)]),
        );
        changes.see(&here(), &tree(&[("a", State::Idle), ("b", State::Working)]));
        changes.see(&here(), &tree(&[("a", State::Idle), ("b", State::Done)]));
        assert!(changes.last(&here(), "b") > changes.last(&here(), "a"));
        assert!(changes.last(&here(), "a") > 0);
        // The same state listed again is no change.
        let before = changes.clone();
        changes.see(&here(), &tree(&[("a", State::Idle), ("b", State::Done)]));
        assert_eq!(changes, before);
    }

    #[test]
    fn a_session_that_comes_after_its_host_was_heard_from_counts_as_changed() {
        let mut changes = Changes::default();
        changes.see(&here(), &tree(&[("a", State::Working)]));
        changes.see(
            &here(),
            &tree(&[("a", State::Working), ("n", State::Starting)]),
        );
        assert!(changes.last(&here(), "n") > 0);
    }

    #[test]
    fn each_environment_is_heard_from_on_its_own_and_keeps_its_own_sessions() {
        let there = EnvironmentId::new("there");
        let mut changes = Changes::default();
        changes.see(&here(), &tree(&[("a", State::Working)]));
        changes.see(&there, &tree(&[("a", State::Idle)]));
        assert_eq!(changes.last(&there, "a"), 0);
        changes.see(&there, &tree(&[("a", State::Done)]));
        assert!(changes.last(&there, "a") > 0);
        assert_eq!(changes.last(&here(), "a"), 0);
    }

    #[test]
    fn a_session_gone_is_forgotten_and_counts_as_changed_should_it_come_back() {
        let mut changes = Changes::default();
        changes.see(&here(), &tree(&[("a", State::Working)]));
        changes.see(&here(), &tree(&[]));
        assert!(changes.last.is_empty());
        changes.see(&here(), &tree(&[("a", State::Working)]));
        assert!(changes.last(&here(), "a") > 0);
    }
}
