use dioxus::prelude::*;

use crate::icons::{Icon, IconView};

/// How something stands, as an icon coloured by `class`, such as
/// `state-glyph run`, told in words by `title` when the pointer is over it.
#[component]
pub fn StateGlyph(
    class: String,
    icon: Icon,
    #[props(default = 14)] size: u32,
    #[props(default)] title: Option<String>,
) -> Element {
    rsx! {
        span { class: "{class}", title, IconView { icon, size } }
    }
}
