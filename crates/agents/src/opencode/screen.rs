//! What OpenCode's screen says of it, which reports no status of its own:
//! the prompt box it waits at, the hint to interrupt a turn running, and the
//! permission it asks for in the prompt's place.

use adroit_model::{StartupReadiness, State, Wait};

/// Whether `line` is the foot of OpenCode's prompt box, drawn while it takes
/// a prompt.
fn prompt_foot(line: &str) -> bool {
    let line = line.trim();
    line.starts_with('╹') && line.contains('▀')
}

/// Whether OpenCode asks to allow a tool call, its choices drawn in the
/// prompt's place.
fn asks_permission(screen: &[String]) -> bool {
    let at = screen.iter().rposition(|line| {
        line.trim_start_matches(['┃', ' '])
            .starts_with("△ Permission required")
    });
    at.is_some_and(|at| {
        screen[at..]
            .iter()
            .any(|line| line.contains("Allow once") && line.contains("Reject"))
    })
}

/// Whether a turn runs: the hint to interrupt it under the prompt.
fn working(screen: &[String]) -> bool {
    let drawn = screen.iter().rev().filter(|line| !line.trim().is_empty());
    drawn
        .take(2)
        .any(|line| line.contains(" esc interrupt") || line.contains(" esc again to interrupt"))
}

pub(super) fn dialog_open(screen: &[String]) -> bool {
    asks_permission(screen)
}

pub(super) fn startup_readiness(screen: &[String]) -> Option<StartupReadiness> {
    match state(screen)? {
        State::Waiting(_) => Some(StartupReadiness::AwaitingPermission),
        _ => Some(StartupReadiness::Ready),
    }
}

pub(super) fn state(screen: &[String]) -> Option<State> {
    if asks_permission(screen) {
        return Some(State::Waiting(Wait::Permission));
    }
    if working(screen) {
        return Some(State::Working);
    }
    screen
        .iter()
        .any(|line| prompt_foot(line))
        .then_some(State::Idle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{recorded, screen};

    #[test]
    fn opencode_is_ready_and_idle_at_its_prompt_at_home_and_after_a_reply() {
        for name in ["opencode-home", "opencode-after-a-reply"] {
            let shown = recorded(name);
            assert_eq!(state(&shown), Some(State::Idle), "{name}");
            assert_eq!(
                startup_readiness(&shown),
                Some(StartupReadiness::Ready),
                "{name}"
            );
            assert!(!dialog_open(&shown), "{name}");
        }
    }

    #[test]
    fn a_turn_runs_while_the_hint_to_interrupt_it_is_drawn() {
        let shown = recorded("opencode-working");
        assert_eq!(state(&shown), Some(State::Working));
        assert_eq!(startup_readiness(&shown), Some(StartupReadiness::Ready));
        let again = screen("  ┃\n  ╹▀▀▀▀▀▀\n   ■⬝⬝⬝ esc again to interrupt     ctrl+p commands");
        assert_eq!(state(&again), Some(State::Working));
    }

    #[test]
    fn a_permission_asked_in_the_prompt_s_place_waits_on_the_user_and_takes_the_keys() {
        let shown = recorded("opencode-permission");
        assert_eq!(state(&shown), Some(State::Waiting(Wait::Permission)));
        assert!(dialog_open(&shown));
        assert_eq!(
            startup_readiness(&shown),
            Some(StartupReadiness::AwaitingPermission)
        );
    }

    #[test]
    fn a_reply_that_quotes_opencode_s_words_tells_nothing_by_them() {
        let quoted = screen(
            "     It said △ Permission required and esc interrupt.\n  ┃\n  ╹▀▀▀▀▀▀\n  ~/src/project  ctrl+p commands",
        );
        assert_eq!(state(&quoted), Some(State::Idle));
        assert!(!dialog_open(&quoted));
    }

    #[test]
    fn nothing_is_told_before_opencode_draws_its_prompt() {
        assert_eq!(state(&screen("Starting background server...")), None);
        assert_eq!(startup_readiness(&[]), None);
    }
}
