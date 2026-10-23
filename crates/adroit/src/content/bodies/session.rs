//! What a window shows of a session it has no screen of yet: one still
//! starting.

use dioxus::prelude::*;

/// What a window shows of a session or a shell until its screen comes.
#[component]
pub(crate) fn Starting() -> Element {
    rsx! {
        div { class: "console-empty", "Starting…" }
    }
}
