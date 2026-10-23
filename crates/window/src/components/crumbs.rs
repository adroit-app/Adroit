use dioxus::prelude::*;

use crate::icons::{Icon, IconView};

/// The way down to what a sheet shows, each step in `crumbs`; a click goes
/// a step back through `onback`, which `title` tells.
#[component]
pub fn Breadcrumbs(title: String, crumbs: Vec<String>, onback: EventHandler<()>) -> Element {
    rsx! {
        div { class: "details-crumbs",
            title: "{title}",
            onclick: move |_| onback.call(()),
            span { class: "details-crumb-back", IconView { icon: Icon::ChevronLeft } }
            for (i , c) in crumbs.iter().enumerate() {
                if i > 0 {
                    span { class: "details-crumb-sep", IconView { icon: Icon::ChevronRight, size: 12 } }
                }
                span { class: "details-crumb", "{c}" }
            }
        }
    }
}
