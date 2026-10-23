//! What a window shows of a session it has no screen of: one running in
//! another terminal, or one still starting.

use dioxus::prelude::*;

use crate::icons::{Icon, IconView};
use crate::view::Keys;

/// A session that runs in another terminal, which shows only there until
/// it is taken over; `ready` while it is idle and can be, by `take_over`.
#[component]
pub(crate) fn Elsewhere(ready: bool, take_over: Option<String>) -> Element {
    rsx! {
        div { class: "console-empty",
            IconView { icon: Icon::Terminal, size: 28 }
            div { class: "console-empty-title", "Running in another terminal" }
            if ready {
                div { "It shows only in the terminal it was started in. Press " if let Some(k) = take_over.clone() { Keys { keys: k } } " to take it over here; Claude Code stops in that terminal." }
            } else {
                div { "It shows only in the terminal it was started in, and is in the middle of a turn. Press " if let Some(k) = take_over.clone() { Keys { keys: k } } " once it is idle to take it over here." }
            }
            div { class: "console-empty-hint", "Or press " kbd { "←" } " on its empty prompt in that terminal." }
        }
    }
}

/// What a window shows of a session or a shell until its screen comes.
#[component]
pub(crate) fn Starting() -> Element {
    rsx! {
        div { class: "console-empty", "Starting…" }
    }
}
