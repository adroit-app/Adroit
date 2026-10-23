//! A message of the overview, opened: whom it went between and all it says.

use dioxus::prelude::*;

use crate::components::{Breadcrumbs, Section};
use crate::details::Note;

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
        Breadcrumbs {
            title: "Back to the overview ({back_keys})",
            crumbs: vec![summary],
            onback: move |_| on_back.call(()),
        }
        Section { label: tag,
            div { class: "message-text", "{note.body}" }
        }
    }
}
