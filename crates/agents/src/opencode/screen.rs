//! What OpenCode's screen says of it while it reports no status: the prompt
//! box it waits at, the hint to interrupt a turn running, and the permission
//! it asks for in the prompt's place, as the rules in force for OpenCode
//! read them.

use adroit_model::{Agent, State};

use crate::Judgement;

pub(super) fn dialog_open(screen: &[String]) -> bool {
    crate::rules::dialog_open(Agent::OpenCode, screen)
}

pub(super) fn judge_state(screen: &[String]) -> Judgement<State> {
    crate::rules::judge_state(Agent::OpenCode, screen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentAdapter;
    use crate::testing::{recorded, screen};
    use adroit_model::Wait;

    fn state_of(screen: &[String]) -> Option<State> {
        judge_state(screen).value
    }

    #[test]
    fn each_recorded_screen_is_read_by_the_rule_named() {
        for (name, rule, default) in [
            ("opencode-permission", "opencode.permission_asked", false),
            ("opencode-working", "opencode.interrupt_hint", false),
            ("opencode-home", "opencode.prompt_idle", true),
            ("opencode-after-a-reply", "opencode.prompt_idle", true),
        ] {
            let verdict = judge_state(&recorded(name)).verdict;
            assert_eq!(
                (verdict.rule.as_str(), verdict.default),
                (rule, default),
                "{name}"
            );
        }
        let starting = judge_state(&screen("Starting background server...")).verdict;
        assert_eq!(starting.rule, "opencode.no_prompt");
    }
    use adroit_model::StartupReadiness;

    #[test]
    fn opencode_is_ready_and_idle_at_its_prompt_at_home_and_after_a_reply() {
        for name in ["opencode-home", "opencode-after-a-reply"] {
            let shown = recorded(name);
            assert_eq!(state_of(&shown), Some(State::Idle), "{name}");
            assert_eq!(
                super::super::OpenCode.startup_readiness(&shown),
                Some(StartupReadiness::Ready),
                "{name}"
            );
            assert!(!dialog_open(&shown), "{name}");
        }
    }

    #[test]
    fn a_turn_runs_while_the_hint_to_interrupt_it_is_drawn() {
        let shown = recorded("opencode-working");
        assert_eq!(state_of(&shown), Some(State::Working));
        assert_eq!(
            super::super::OpenCode.startup_readiness(&shown),
            Some(StartupReadiness::Ready)
        );
        let again = screen("  ┃\n  ╹▀▀▀▀▀▀\n   ■⬝⬝⬝ esc again to interrupt     ctrl+p commands");
        assert_eq!(state_of(&again), Some(State::Working));
    }

    #[test]
    fn a_permission_asked_in_the_prompt_s_place_waits_on_the_user_and_takes_the_keys() {
        let shown = recorded("opencode-permission");
        assert_eq!(state_of(&shown), Some(State::Waiting(Wait::Permission)));
        assert!(dialog_open(&shown));
        assert_eq!(
            super::super::OpenCode.startup_readiness(&shown),
            Some(StartupReadiness::AwaitingPermission)
        );
    }

    #[test]
    fn a_reply_that_quotes_opencode_s_words_tells_nothing_by_them() {
        let quoted = screen(
            "     It said △ Permission required and esc interrupt.\n  ┃\n  ╹▀▀▀▀▀▀\n  ~/src/project  ctrl+p commands",
        );
        assert_eq!(state_of(&quoted), Some(State::Idle));
        assert!(!dialog_open(&quoted));
    }

    #[test]
    fn nothing_is_told_before_opencode_draws_its_prompt() {
        assert_eq!(state_of(&screen("Starting background server...")), None);
        assert_eq!(super::super::OpenCode.startup_readiness(&[]), None);
    }
}
