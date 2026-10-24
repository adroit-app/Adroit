//! The marks a row, a tab and a window's strip draw alike: how a session
//! stands, whether it only reads, and what a terminal runs.

use adroit_model::{State, TerminalProgram, TerminalSession};
use dioxus::prelude::*;

use crate::icons::{Icon, IconView};

/// A state's mark: its class, its icon, and the word told when the pointer
/// is over it.
pub(crate) fn glyph(state: &State) -> (&'static str, Icon, &'static str) {
    let (class, icon) = match state {
        State::Waiting(_) => ("state-glyph wait", Icon::Bell),
        State::Starting => ("state-glyph idle", Icon::Circle),
        State::Working => ("state-glyph run", Icon::Loader),
        State::Idle => ("state-glyph idle", Icon::Circle),
        State::Done => ("state-glyph done", Icon::Check),
        State::Failed => ("state-glyph fail", Icon::AlertTriangle),
    };
    (class, icon, state.word())
}

/// The mark of a session or a draft that only reads.
pub(crate) fn session_mark(reads_only: bool) -> Option<(Icon, &'static str)> {
    reads_only.then_some((Icon::Book, "only reads"))
}

/// The session's read-only mark, shared by rows and tabs.
#[component]
pub(crate) fn SessionMark(mark: Option<(Icon, &'static str)>) -> Element {
    rsx! {
        if let Some((icon, title)) = mark {
            span { class: "reads-only-mark", title,
                IconView { icon, size: 13 }
            }
        }
    }
}

/// The agent a session runs, by name.
#[component]
pub(crate) fn AgentBadge(label: String) -> Element {
    rsx! {
        span { class: "agent-badge", title: "Session agent", "{label}" }
    }
}

/// The mark of a session the prompt holds something written for.
#[component]
pub(crate) fn DraftMark() -> Element {
    rsx! {
        span { class: "draft-mark", title: "Something is written for it in the prompt", "✎" }
    }
}

/// The mark of a place: a workspace, a folder, a session not started yet.
#[component]
pub(crate) fn PlaceMark(icon: Icon, #[props(default = 14)] size: u32) -> Element {
    rsx! {
        span { class: "place-mark", IconView { icon, size } }
    }
}

/// A file's icon, `glyph` of the Nerd Font, in the colour `style` gives it.
#[component]
pub(crate) fn FileGlyph(glyph: String, style: String) -> Element {
    rsx! {
        span { class: "nf", style: "{style}", "aria-hidden": "true", "{glyph}" }
    }
}

/// What the shell `id` runs; a shell when it is not among `terminals`.
pub(crate) fn runs_of(terminals: &[TerminalSession], id: u128) -> TerminalProgram {
    terminals
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.program)
        .unwrap_or_default()
}

/// The mark of a shell's row or tab, by what it runs.
#[component]
pub(crate) fn TerminalMark(runs: TerminalProgram) -> Element {
    let (icon, title) = match runs {
        TerminalProgram::Shell => (Icon::Terminal2, "shell"),
        TerminalProgram::Neovim => (Icon::Code, "Neovim"),
        TerminalProgram::Command => (Icon::Terminal, "program"),
    };
    rsx! {
        span { class: "shell-mark", title,
            IconView { icon }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_and_drafts_that_only_read_are_marked() {
        assert_eq!(session_mark(true), Some((Icon::Book, "only reads")));
        assert_eq!(session_mark(false), None);
    }
}
