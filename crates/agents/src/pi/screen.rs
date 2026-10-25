//! What Pi's screen says of it when its program status does not: the
//! project trust picker it shows before its interface, which reports no
//! status, and the editor, pickers and working rule of the interface.

use adroit_model::{StartupReadiness, State, Wait};

/// Whether `line` is one of the rules Pi draws above and below its editor,
/// the upper one carrying `Working` while a turn runs.
fn rule(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("──") && line.ends_with('─')
}

/// The rows between the last two rules: Pi's editor, or the picker or
/// dialog drawn in its place; none while Pi draws no editor yet.
fn editor(screen: &[String]) -> Option<(&String, &[String])> {
    let mut rules = screen
        .iter()
        .enumerate()
        .filter(|(_, l)| rule(l))
        .map(|(i, _)| i);
    let lower = rules.next_back()?;
    let upper = rules.next_back()?;
    Some((&screen[upper], &screen[upper + 1..lower]))
}

/// Whether a picker or dialog holds the keys: one of Pi's hints to cancel
/// it is drawn where the editor would be.
pub(super) fn dialog_open(screen: &[String]) -> bool {
    editor(screen).is_some_and(|(_, rows)| {
        rows.iter().any(|row| {
            let row = row.to_lowercase();
            row.contains("cancel") && row.contains("esc")
        })
    })
}

/// Whether Pi asks whether to trust the project's own files.
fn asks_trust(screen: &[String]) -> bool {
    editor(screen).is_some_and(|(_, rows)| {
        rows.iter()
            .any(|row| row.trim().eq_ignore_ascii_case("trust project folder?"))
    })
}

pub(super) fn startup_readiness(screen: &[String]) -> Option<StartupReadiness> {
    if asks_trust(screen) {
        return Some(StartupReadiness::AwaitingTrust);
    }
    state(screen).and_then(crate::readiness)
}

pub(super) fn state(screen: &[String]) -> Option<State> {
    let (upper, _) = editor(screen)?;
    if asks_trust(screen) || dialog_open(screen) {
        return Some(State::Waiting(Wait::Input));
    }
    Some(match upper.contains("Working") {
        true => State::Working,
        false => State::Idle,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{recorded, screen};

    #[test]
    fn the_trust_picker_before_the_interface_awaits_the_user_s_trust() {
        let trust = recorded("pi-trust");
        assert_eq!(
            startup_readiness(&trust),
            Some(StartupReadiness::AwaitingTrust)
        );
        assert!(dialog_open(&trust));
        assert_eq!(state(&trust), Some(State::Waiting(Wait::Input)));
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
        assert_eq!(state(&recorded("pi-ready")), Some(State::Idle));
        assert_eq!(state(&recorded("pi-after-a-reply")), Some(State::Idle));
        assert_eq!(state(&recorded("pi-working")), Some(State::Working));
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
        assert_eq!(state(&starting), None);
        assert_eq!(state(&[]), None);
    }

    #[test]
    fn a_reply_quoting_a_dialog_above_the_editor_holds_no_keys() {
        let quoted = screen(
            "Trust project folder?\n↑↓ navigate  enter select  escape/ctrl+c cancel\n────────────\n\n────────────\n~/src\n0.0%/33k (auto)",
        );
        assert!(!dialog_open(&quoted));
        assert_eq!(state(&quoted), Some(State::Idle));
        assert_eq!(startup_readiness(&quoted), Some(StartupReadiness::Ready));
    }
}
