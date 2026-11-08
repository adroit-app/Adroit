//! What Codex's screen says of it: the composer it waits at, the dialogs
//! and pickers drawn under it, and the hint to interrupt a turn running.

use adroit_model::{State, Wait};

use crate::{Judgement, Verdict};

pub(super) fn dialog_open(screen: &[String]) -> bool {
    dialog_row(screen).is_some()
}

/// The row that shows a dialog or picker has the keys: the question's hint,
/// the selected row of a numbered menu, or a hint to leave it.
fn dialog_row(screen: &[String]) -> Option<usize> {
    // The composer uses the same chevron as a selected numbered row.
    // Look from the bottom so a reply quoting a menu cannot capture keys.
    let question = |line: &str| {
        let line = line.to_lowercase();
        line.contains("to submit answer") || line.contains("to submit all")
    };
    let hint = |line: &str| {
        let line = line.to_lowercase();
        question(&line)
            || line.trim_start().starts_with("enter/esc continue")
            || line.contains("esc to cancel")
            || line.contains("esc to dismiss")
            || line.contains("esc to close")
            || line.contains("esc to go back")
            || line.contains("esc back")
    };
    let numbered = |row: &str| {
        row.trim_start()
            .split_once(". ")
            .is_some_and(|(n, _)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    };
    for (index, line) in screen.iter().enumerate().rev() {
        if let Some(row) = line.trim_start().strip_prefix('›') {
            let below = index + 1..screen.len();
            if let Some(asked) = below.clone().find(|&i| question(&screen[i])) {
                return Some(asked);
            }
            let menu = numbered(row)
                && below
                    .into_iter()
                    .any(|i| numbered(&screen[i]) || hint(&screen[i]));
            return menu.then_some(index);
        }
    }
    (0..screen.len())
        .rev()
        .filter(|&i| !screen[i].trim().is_empty())
        .take(4)
        .find(|&i| hint(&screen[i]))
}

pub(super) fn judge_state(screen: &[String]) -> Judgement<State> {
    let composer = screen.iter().rposition(|s| s.trim_start().starts_with('›'));
    let dialog = dialog_row(screen);
    let Some(composer) = composer else {
        return match dialog {
            Some(at) => Judgement::new(
                Some(State::Waiting(Wait::Input)),
                Verdict::matched("codex.dialog_without_composer", screen, [at]),
            ),
            None => Judgement::new(None, Verdict::defaulted("codex.no_composer", screen, [])),
        };
    };
    let below =
        |says: &dyn Fn(&str) -> bool| (composer + 1..screen.len()).find(|&i| says(&screen[i]));
    if let Some(at) = dialog {
        let (state, rule, at) = if let Some(asked) =
            below(&|s| s.contains("to submit answer") || s.contains("to submit all"))
        {
            (Wait::Input, "codex.question_below_composer", asked)
        } else if let Some(asked) =
            below(&|s| s.contains("Press enter to confirm or esc to cancel"))
        {
            (Wait::Permission, "codex.permission_below_composer", asked)
        } else {
            (Wait::Input, "codex.dialog_below_composer", at)
        };
        return Judgement::new(
            Some(State::Waiting(state)),
            Verdict::matched(rule, screen, [at]),
        );
    }
    // A previous composer belongs to the transcript, not the live footer.
    let live = || {
        (0..composer)
            .rev()
            .take_while(|&i| !screen[i].trim_start().starts_with('›'))
    };
    let working = live().find(|&i| {
        screen[i].trim_start().starts_with("• ") && screen[i].contains("esc to interrupt")
    });
    let worked = live().find(|&i| screen[i].trim_start().starts_with("• Worked for "));
    match (working, worked) {
        (Some(at), _) => Judgement::new(
            Some(State::Working),
            Verdict::matched("codex.interrupt_hint", screen, [at]),
        ),
        (None, Some(at)) => Judgement::new(
            Some(State::Idle),
            Verdict::matched("codex.worked_for", screen, [at]),
        ),
        (None, None) => Judgement::new(
            Some(State::Idle),
            Verdict::defaulted("codex.composer_idle", screen, [composer]),
        ),
    }
}
