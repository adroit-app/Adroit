//! What Pi's screen says of it when its program status does not: the
//! project trust picker it shows before its interface, which reports no
//! status, and the editor, pickers and working rule of the interface.

use adroit_model::{StartupReadiness, State, Wait};

use crate::{Judgement, Verdict};

/// Whether `line` is one of the rules Pi draws above and below its editor,
/// the upper one carrying `Working` while a turn runs.
fn rule(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("──") && line.ends_with('─')
}

/// Where the last two rules are: the rows between them are Pi's editor, or
/// the picker or dialog drawn in its place; none while Pi draws no editor yet.
fn editor(screen: &[String]) -> Option<(usize, std::ops::Range<usize>)> {
    let mut rules = screen
        .iter()
        .enumerate()
        .filter(|(_, l)| rule(l))
        .map(|(i, _)| i);
    let lower = rules.next_back()?;
    let upper = rules.next_back()?;
    Some((upper, upper + 1..lower))
}

/// The editor's row that says `says`.
fn in_editor(screen: &[String], says: impl Fn(&str) -> bool) -> Option<usize> {
    editor(screen).and_then(|(_, mut rows)| rows.find(|&i| says(&screen[i])))
}

/// The row of one of Pi's hints to cancel a picker or dialog, drawn where
/// the editor would be while it holds the keys.
fn dialog_row(screen: &[String]) -> Option<usize> {
    in_editor(screen, |row| {
        let row = row.to_lowercase();
        row.contains("cancel") && row.contains("esc")
    })
}

pub(super) fn dialog_open(screen: &[String]) -> bool {
    dialog_row(screen).is_some()
}

/// The row where Pi asks whether to trust the project's own files.
fn trust_row(screen: &[String]) -> Option<usize> {
    in_editor(screen, |row| {
        row.trim().eq_ignore_ascii_case("trust project folder?")
    })
}

pub(super) fn judge_startup(screen: &[String]) -> Judgement<StartupReadiness> {
    if let Some(at) = trust_row(screen) {
        return Judgement::new(
            Some(StartupReadiness::AwaitingTrust),
            Verdict::matched("pi.trust_asked", screen, [at]),
        );
    }
    judge_state(screen).then(crate::readiness)
}

pub(super) fn judge_state(screen: &[String]) -> Judgement<State> {
    let Some((upper, _)) = editor(screen) else {
        return Judgement::new(None, Verdict::defaulted("pi.no_editor", screen, []));
    };
    let waiting = |rule: &str, at: usize| {
        Judgement::new(
            Some(State::Waiting(Wait::Input)),
            Verdict::matched(rule, screen, [at]),
        )
    };
    if let Some(at) = trust_row(screen) {
        return waiting("pi.trust_asked", at);
    }
    if let Some(at) = dialog_row(screen) {
        return waiting("pi.dialog_in_editor", at);
    }
    match screen[upper].contains("Working") {
        true => Judgement::new(
            Some(State::Working),
            Verdict::matched("pi.working_rule", screen, [upper]),
        ),
        false => Judgement::new(
            Some(State::Idle),
            Verdict::defaulted("pi.editor_idle", screen, [upper]),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{recorded, screen};

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
