//! A message of the overview, opened: whom it went between and all it says.

use dioxus::prelude::*;

use crate::details::Note;
use crate::icons::{Icon, IconView};

const CSS: &str = include_str!("../assets/css/message.css");

/// A message opened from the overview, told at `now_ms`; `on_back` goes back
/// to the overview, as `back_keys` do.
#[component]
pub fn MessageView(
    note: Note,
    now_ms: u64,
    back_keys: String,
    on_back: EventHandler<()>,
) -> Element {
    let tag = note.tag(now_ms);
    let summary = note.summary().to_owned();
    rsx! {
        style { {CSS} }
        div { class: "details-crumbs",
            title: "Back to the overview ({back_keys})",
            onclick: move |_| on_back.call(()),
            span { class: "details-crumb-back", IconView { icon: Icon::ChevronLeft } }
            span { class: "details-crumb", "{summary}" }
        }
        div { class: "details-block",
            span { class: "details-label", "{tag}" }
            div { class: "message-text", "{note.body}" }
        }
    }
}
