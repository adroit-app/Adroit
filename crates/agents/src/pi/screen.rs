//! What Pi's screen says of it when its program status does not: the
//! project trust picker it shows before its interface, which reports no
//! status, and the editor, pickers and working rule of the interface, as
//! the rules in force for Pi read them.

use adroit_model::{Agent, StartupReadiness, State};

use crate::Judgement;

pub(super) fn dialog_open(screen: &[String]) -> bool {
    crate::rules::dialog_open(Agent::Pi, screen)
}

pub(super) fn judge_startup(screen: &[String]) -> Judgement<StartupReadiness> {
    crate::rules::judge_startup(Agent::Pi, screen)
}

pub(super) fn judge_state(screen: &[String]) -> Judgement<State> {
    crate::rules::judge_state(Agent::Pi, screen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{recorded, screen};
    use adroit_model::Wait;

    fn startup_readiness(screen: &[String]) -> Option<StartupReadiness> {
        judge_startup(screen).value
    }

    fn state_of(screen: &[String]) -> Option<State> {
        judge_state(screen).value
    }

    #[test]
    fn each_recorded_screen_is_read_by_the_rule_named() {
        for (name, rule, default) in [
            ("pi-trust", "pi.trust_asked", false),
            ("pi-model", "pi.dialog_in_editor", false),
            ("pi-working", "pi.working_rule", false),
            ("pi-ready", "pi.editor_idle", true),
            ("pi-after-a-reply", "pi.editor_idle", true),
        ] {
            let verdict = judge_state(&recorded(name)).verdict;
            assert_eq!(
                (verdict.rule.as_str(), verdict.default),
                (rule, default),
                "{name}"
            );
        }
        assert_eq!(judge_state(&[]).verdict.rule, "pi.no_editor");
        assert_eq!(
            judge_startup(&recorded("pi-trust")).verdict.rule,
            "pi.trust_asked"
        );
        let working = judge_state(&recorded("pi-working")).verdict;
        assert!(working.evidence[0].text.contains("Working"));
    }

    #[test]
    fn the_trust_picker_before_the_interface_awaits_the_user_s_trust() {
        let trust = recorded("pi-trust");
        assert_eq!(
            startup_readiness(&trust),
            Some(StartupReadiness::AwaitingTrust)
        );
        assert!(dialog_open(&trust));
        assert_eq!(state_of(&trust), Some(State::Waiting(Wait::Input)));
    }

    #[test]
    fn pi_is_ready_once_its_editor_is_drawn_working_or_not() {
        for name in ["pi-ready", "pi-working", "pi-after-a-reply"] {
            assert_eq!(
                startup_readiness(&recorded(name)),
                Some(StartupReadiness::Ready),
                "{name}"
            );
            assert!(!dialog_open(&recorded(name)), "{name}");
        }
        assert_eq!(state_of(&recorded("pi-ready")), Some(State::Idle));
        assert_eq!(state_of(&recorded("pi-after-a-reply")), Some(State::Idle));
        assert_eq!(state_of(&recorded("pi-working")), Some(State::Working));
    }

    #[test]
    fn a_picker_in_place_of_the_editor_takes_the_keys() {
        let picker = recorded("pi-model");
        assert!(dialog_open(&picker));
        assert_eq!(
            startup_readiness(&picker),
            Some(StartupReadiness::AwaitingInput)
        );
    }

    #[test]
    fn nothing_is_told_before_pi_draws_its_interface() {
        let starting = screen(
            "Warning: No project session found with id 'x'; creating a new session with that id.",
        );
        assert_eq!(startup_readiness(&starting), None);
        assert_eq!(state_of(&starting), None);
        assert_eq!(state_of(&[]), None);
    }

    #[test]
    fn a_reply_quoting_a_dialog_above_the_editor_holds_no_keys() {
        let quoted = screen(
            "Trust project folder?\n↑↓ navigate  enter select  escape/ctrl+c cancel\n────────────\n\n────────────\n~/src\n0.0%/33k (auto)",
        );
        assert!(!dialog_open(&quoted));
        assert_eq!(state_of(&quoted), Some(State::Idle));
        assert_eq!(startup_readiness(&quoted), Some(StartupReadiness::Ready));
    }
}
