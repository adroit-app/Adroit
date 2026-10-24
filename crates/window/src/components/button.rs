use dioxus::prelude::*;

use super::classes;
use crate::icons::{Icon, IconView};

/// A button of the windows' own look, `kind` naming one of its colours,
/// such as `primary` or `danger`; `children` say what it does.
#[component]
pub fn Button(
    #[props(default)] kind: String,
    #[props(default)] title: Option<String>,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    rsx! {
        button { class: classes("button", &kind), title, onclick: move |e| onclick.call(e), {children} }
    }
}

/// A small button that goes elsewhere, `icon` before what `children` say.
#[component]
pub fn LinkButton(
    icon: Icon,
    #[props(default)] title: Option<String>,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    rsx! {
        button { class: "usage-web", title, onclick: move |e| onclick.call(e),
            IconView { icon, size: 12 }
            {children}
        }
    }
}

/// The room left in a row, pushing what follows to its far end.
#[component]
pub fn Spacer() -> Element {
    rsx! {
        span { class: "flex-spacer" }
    }
}
