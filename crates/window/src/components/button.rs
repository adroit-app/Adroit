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

/// A button a component of the details panel is, as its placement says:
/// coloured by its tone, with its icon, and its label unless it shows its
/// icon alone, when the label is its tip.
#[component]
pub fn PlacedButton(
    placed: adroit_core::details::Placed,
    #[props(default)] disabled: bool,
    #[props(default)] tip: Option<String>,
    onclick: EventHandler<()>,
) -> Element {
    use adroit_core::details::Tone;
    let label = placed.label();
    let class = match placed.tone() {
        Tone::Plain => "button",
        Tone::Primary => "button primary",
        Tone::Danger => "button danger",
    };
    let icon = placed.button_icon();
    let icon_only = placed.icon_only && icon.is_some();
    rsx! {
        div { class: if icon_only { "details-block details-button icon-only" } else { "details-block details-button" },
            button {
                class: "{class}",
                r#type: "button",
                disabled,
                title: tip.clone().unwrap_or_else(|| label.clone()),
                aria_label: "{label}",
                onclick: move |_| onclick.call(()),
                if let Some(icon) = icon {
                    IconView { icon, size: 13 }
                }
                if !icon_only {
                    "{label}"
                }
            }
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
