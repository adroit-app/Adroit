//! What Codex's screen says of it: the composer it waits at, the dialogs
//! and pickers drawn under it, and the hint to interrupt a turn running.

use adroit_model::{State, Wait};

pub(super) fn dialog_open(screen: &[String]) -> bool {
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
            let below = &screen[index + 1..];
            return below.iter().any(|s| question(s))
                || (numbered(row) && below.iter().any(|s| numbered(s) || hint(s)));
        }
    }
    screen
        .iter()
        .rev()
        .filter(|s| !s.trim().is_empty())
        .take(4)
        .any(|s| hint(s))
}

pub(super) fn state(screen: &[String]) -> Option<State> {
    let composer = screen.iter().rposition(|s| s.trim_start().starts_with('›'));
    if composer.is_none() && dialog_open(screen) {
        return Some(State::Waiting(Wait::Input));
    }
    let composer = composer?;
    let below = &screen[composer + 1..];
    if dialog_open(screen) {
        return Some(
            if below
                .iter()
                .any(|s| s.contains("to submit answer") || s.contains("to submit all"))
            {
                State::Waiting(Wait::Input)
            } else if below
                .iter()
                .any(|s| s.contains("Press enter to confirm or esc to cancel"))
            {
                State::Waiting(Wait::Permission)
            } else {
                State::Waiting(Wait::Input)
            },
        );
    }
    // A previous composer belongs to the transcript, not the live footer.
    let working = screen[..composer]
        .iter()
        .rev()
        .take_while(|s| !s.trim_start().starts_with('›'))
        .any(|s| s.trim_start().starts_with("• ") && s.contains("esc to interrupt"));
    Some(if working { State::Working } else { State::Idle })
}
