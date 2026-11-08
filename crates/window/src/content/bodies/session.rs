//! What a window shows of a session it has no screen of yet: one still
//! starting, or one the sessions' daemon took with it.

use adroit_core::content::{Tab, waits_to_come_back};
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::components::EmptyState;

/// What a window shows of the session or the shell `tab` names until its
/// screen comes.
#[component]
pub(crate) fn Starting(tab: Tab) -> Element {
    let app = use_context::<AppCtx>();
    if waits_to_come_back(&app.hosts.read(), &tab) {
        return rsx! {
            EmptyState { "The sessions' daemon took this with it when it went. Bring it back from the banner, or with Bring Back Sessions… in the Session menu." }
        };
    }
    rsx! {
        EmptyState { "Starting…" }
    }
}
