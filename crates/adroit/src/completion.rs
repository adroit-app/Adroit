//! Completions acknowledged in this window, separate from reported state.

use std::collections::HashSet;

use adroit_model::{Session, State, Tree};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Completions {
    seen: HashSet<String>,
}

impl Completions {
    pub fn updated(&self, tree: &Tree, shown: Option<&str>) -> Self {
        let seen = tree
            .sessions()
            .filter(|(_, s)| s.state == State::Done)
            .filter(|(_, s)| self.seen.contains(&s.id) || shown == Some(s.id.as_str()))
            .map(|(_, s)| s.id.clone())
            .collect();
        Self { seen }
    }

    pub fn state<'a>(&self, session: &'a Session) -> &'a State {
        if session.state == State::Done && self.seen.contains(&session.id) {
            &State::Idle
        } else {
            &session.state
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_model::{Agent, State, Wait, build};
    use std::collections::HashMap;

    fn session(agent: Agent, state: State) -> adroit_model::Session {
        adroit_model::Session {
            agent,
            id: "review".into(),
            cwd: "/project".into(),
            state,
            ..Default::default()
        }
    }

    fn tree(session: &adroit_model::Session) -> adroit_model::Tree {
        build(
            std::slice::from_ref(session),
            &[],
            &HashMap::new(),
            &[],
            None,
        )
    }

    #[test]
    fn an_unviewed_completion_keeps_its_check_for_either_agent() {
        for agent in Agent::ALL {
            let s = session(agent, State::Done);
            let marks = Completions::default().updated(&tree(&s), None);
            assert_eq!(marks.state(&s), &State::Done);
        }
    }

    #[test]
    fn viewing_a_completion_clears_its_check_across_refreshes_and_tab_switches() {
        for agent in Agent::ALL {
            let s = session(agent, State::Done);
            let marks = Completions::default().updated(&tree(&s), Some(&s.id));
            assert_eq!(marks.state(&s), &State::Idle);
            let marks = marks.updated(&tree(&s), Some("another-session"));
            assert_eq!(marks.state(&s), &State::Idle);
            assert_eq!(s.state, State::Done);
        }
    }

    #[test]
    fn a_new_turn_completed_out_of_view_gets_a_new_check() {
        let mut s = session(Agent::Claude, State::Done);
        let marks = Completions::default().updated(&tree(&s), Some(&s.id));
        s.state = State::Working;
        let marks = marks.updated(&tree(&s), None);
        assert_eq!(marks.state(&s), &State::Working);
        s.state = State::Done;
        let marks = marks.updated(&tree(&s), None);
        assert_eq!(marks.state(&s), &State::Done);
    }

    #[test]
    fn a_completion_already_on_screen_is_seen() {
        let mut s = session(Agent::Codex, State::Working);
        let marks = Completions::default().updated(&tree(&s), Some(&s.id));
        s.state = State::Done;
        let marks = marks.updated(&tree(&s), Some(&s.id));
        assert_eq!(marks.state(&s), &State::Idle);
    }

    #[test]
    fn viewing_does_not_clear_work_waiting_or_failure() {
        for state in [
            State::Idle,
            State::Working,
            State::Waiting(Wait::Permission),
            State::Failed,
        ] {
            let s = session(Agent::Claude, state.clone());
            let marks = Completions::default().updated(&tree(&s), Some(&s.id));
            assert_eq!(marks.state(&s), &state);
        }
    }

    #[test]
    fn a_removed_session_does_not_leave_a_seen_completion_behind() {
        let s = session(Agent::Claude, State::Done);
        let marks = Completions::default().updated(&tree(&s), Some(&s.id));
        let marks = marks.updated(&adroit_model::Tree::default(), None);
        assert_eq!(marks, Completions::default());
    }
}
