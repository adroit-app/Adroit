//! What OpenCode's screen says of it while it reports no status: the prompt
//! box it waits at, the hint to interrupt a turn running, and the permission
//! it asks for in the prompt's place.

use adroit_model::{State, Wait};

use crate::{Judgement, Verdict};

/// Whether `line` is the foot of OpenCode's prompt box, drawn while it takes
/// a prompt.
fn prompt_foot(line: &str) -> bool {
    let line = line.trim();
    line.starts_with('╹') && line.contains('▀')
}

/// The row where OpenCode asks to allow a tool call, its choices drawn in
/// the prompt's place.
fn permission_row(screen: &[String]) -> Option<usize> {
    let at = screen.iter().rposition(|line| {
        line.trim_start_matches(['┃', ' '])
            .starts_with("△ Permission required")
    })?;
    screen[at..]
        .iter()
        .any(|line| line.contains("Allow once") && line.contains("Reject"))
        .then_some(at)
}

/// The row of the hint to interrupt a turn running, under the prompt.
fn working_row(screen: &[String]) -> Option<usize> {
    (0..screen.len())
        .rev()
        .filter(|&i| !screen[i].trim().is_empty())
        .take(2)
        .find(|&i| {
            let line = &screen[i];
            line.contains(" esc interrupt") || line.contains(" esc again to interrupt")
        })
}

pub(super) fn dialog_open(screen: &[String]) -> bool {
    permission_row(screen).is_some()
}

pub(super) fn judge_state(screen: &[String]) -> Judgement<State> {
    if let Some(at) = permission_row(screen) {
        return Judgement::new(
            Some(State::Waiting(Wait::Permission)),
            Verdict::matched("opencode.permission_asked", screen, [at]),
        );
    }
    if let Some(at) = working_row(screen) {
        return Judgement::new(
            Some(State::Working),
            Verdict::matched("opencode.interrupt_hint", screen, [at]),
        );
    }
    match screen.iter().rposition(|line| prompt_foot(line)) {
        Some(at) => Judgement::new(
            Some(State::Idle),
            Verdict::defaulted("opencode.prompt_idle", screen, [at]),
        ),
        None => Judgement::new(None, Verdict::defaulted("opencode.no_prompt", screen, [])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentAdapter;
    use crate::testing::{recorded, screen};

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
