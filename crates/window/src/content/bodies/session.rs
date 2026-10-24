//! What a window shows of a session it has no screen of yet: one still
//! starting.

use dioxus::prelude::*;

use crate::components::EmptyState;

/// What a window shows of a session or a shell until its screen comes.
#[component]
pub(crate) fn Starting() -> Element {
    rsx! {
        EmptyState { "Starting…" }
    }
}
