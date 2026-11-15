//! A message of the overview, opened: whom it went between and all it says.

use dioxus::prelude::*;

use crate::components::{Breadcrumbs, Section};
use crate::details::Note;

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

/// A prompt opened, told at `now_ms`; `on_back` goes back to its list, as
/// `back_keys` do.
#[component]
pub fn PromptView(
    prompt: adroit_model::links::Prompt,
    now_ms: u64,
    back_keys: String,
    on_back: EventHandler<()>,
) -> Element {
    let summary = prompt.text.lines().next().unwrap_or_default().to_owned();
    let when = prompt
        .at_ms
        .map(|at| adroit_model::time::ago(now_ms.saturating_sub(at)))
        .unwrap_or_default();
    rsx! {
        Breadcrumbs {
            title: "Back to the list ({back_keys})",
            crumbs: vec![summary],
            onback: move |_| on_back.call(()),
        }
        Section { label: "Turn {prompt.turn} · {when}",
            div { class: "message-text", "{prompt.text}" }
        }
    }
}
