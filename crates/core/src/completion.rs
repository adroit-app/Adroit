//! Completions acknowledged in this window, separate from reported state.

use std::collections::HashSet;

use adroit_model::{Session, State, TerminalSession, Tree};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Completions {
    seen: HashSet<String>,
    seen_terminals: HashSet<u128>,
}

/// What the focused window shows while the user is in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewed<'a> {
    Session(&'a str),
    Terminal(u128),
}

impl Completions {
    pub fn updated(&self, tree: &Tree, shown: Option<Viewed>) -> Self {
        let seen = tree
            .sessions()
            .filter(|(_, s)| s.state == State::Done)
            .filter(|(_, s)| {
                self.seen.contains(&s.id) || shown == Some(Viewed::Session(s.id.as_str()))
            })
            .map(|(_, s)| s.id.clone())
            .collect();
        let seen_terminals = tree
            .worktrees()
            .flat_map(|w| w.terminals.iter())
            .filter(|t| t.state == Some(State::Done))
            .filter(|t| {
                self.seen_terminals.contains(&t.id) || shown == Some(Viewed::Terminal(t.id))
            })
            .map(|t| t.id)
            .collect();
        Self {
            seen,
            seen_terminals,
        }
    }

    pub fn state<'a>(&self, session: &'a Session) -> &'a State {
        if session.state == State::Done && self.seen.contains(&session.id) {
            &State::Idle
        } else {
            &session.state
        }
    }

    /// How the program in `terminal` stands, as this window marks it; `None`
    /// while it has reported nothing.
    pub fn terminal_state<'a>(&self, terminal: &'a TerminalSession) -> Option<&'a State> {
        match &terminal.state {
            Some(State::Done) if self.seen_terminals.contains(&terminal.id) => Some(&State::Idle),
            state => state.as_ref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_model::testing;
    use adroit_model::{Agent, State, Wait};

    fn session(agent: Agent, state: State) -> adroit_model::Session {
        testing::session("review")
            .cwd("/project")
            .agent(agent)
            .state(state)
            .build()
    }

    fn tree(session: &adroit_model::Session) -> adroit_model::Tree {
        testing::tree().session(session.clone()).build()
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
            let marks = Completions::default().updated(&tree(&s), Some(Viewed::Session(&s.id)));
            assert_eq!(marks.state(&s), &State::Idle);
            let marks = marks.updated(&tree(&s), Some(Viewed::Session("another-session")));
            assert_eq!(marks.state(&s), &State::Idle);
            assert_eq!(s.state, State::Done);
        }
    }

    #[test]
    fn a_new_turn_completed_out_of_view_gets_a_new_check() {
        let mut s = session(Agent::Claude, State::Done);
        let marks = Completions::default().updated(&tree(&s), Some(Viewed::Session(&s.id)));
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
        let marks = Completions::default().updated(&tree(&s), Some(Viewed::Session(&s.id)));
        s.state = State::Done;
        let marks = marks.updated(&tree(&s), Some(Viewed::Session(&s.id)));
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
            let marks = Completions::default().updated(&tree(&s), Some(Viewed::Session(&s.id)));
            assert_eq!(marks.state(&s), &state);
        }
    }

    fn terminal(state: Option<State>) -> (adroit_model::TerminalSession, adroit_model::Tree) {
        let t = adroit_model::TerminalSession {
            state,
            ..testing::shell(7, "/project")
        };
        let anchor = session(Agent::Claude, State::Idle);
        let tree = testing::tree().session(anchor).terminal(t.clone()).build();
        assert!(tree.terminal(7).is_some());
        (t, tree)
    }

    #[test]
    fn a_terminal_s_finished_program_keeps_its_check_until_the_terminal_is_viewed() {
        let (t, tree) = terminal(Some(State::Done));
        let marks = Completions::default().updated(&tree, None);
        assert_eq!(marks.terminal_state(&t), Some(&State::Done));
        let marks = marks.updated(&tree, Some(Viewed::Terminal(7)));
        assert_eq!(marks.terminal_state(&t), Some(&State::Idle));
        let marks = marks.updated(&tree, None);
        assert_eq!(marks.terminal_state(&t), Some(&State::Idle));
    }

    #[test]
    fn a_terminal_whose_program_reports_nothing_has_no_state() {
        let (t, tree) = terminal(None);
        let marks = Completions::default().updated(&tree, Some(Viewed::Terminal(7)));
        assert_eq!(marks.terminal_state(&t), None);
    }

    #[test]
    fn viewing_a_terminal_does_not_clear_its_work_or_failure() {
        for state in [State::Working, State::Waiting(Wait::Input), State::Failed] {
            let (t, tree) = terminal(Some(state.clone()));
            let marks = Completions::default().updated(&tree, Some(Viewed::Terminal(7)));
            assert_eq!(marks.terminal_state(&t), Some(&state));
        }
    }

    #[test]
    fn a_removed_session_does_not_leave_a_seen_completion_behind() {
        let s = session(Agent::Claude, State::Done);
        let marks = Completions::default().updated(&tree(&s), Some(Viewed::Session(&s.id)));
        let marks = marks.updated(&adroit_model::Tree::default(), None);
        assert_eq!(marks, Completions::default());
    }
}
