use dioxus::prelude::*;

use crate::icons::{Icon, IconView};

/// How something stands, as an icon coloured by `class`, such as
/// `state-glyph run`.
#[component]
pub fn StateGlyph(class: String, icon: Icon, #[props(default = 14)] size: u32) -> Element {
    rsx! {
        span { class: "{class}", IconView { icon, size } }
    }
}
