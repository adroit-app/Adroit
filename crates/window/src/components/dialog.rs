use dioxus::prelude::*;

use super::classes;
use crate::icons::{Icon, IconView};

/// A dialog over the window, shaded around: `title` at its head, with
/// `icon` before it, and `children` below. `kind` names a look of its own.
#[component]
pub fn Dialog(
    #[props(default)] kind: String,
    #[props(default)] icon: Option<Icon>,
    title: String,
    children: Element,
) -> Element {
    rsx! {
        div { class: "dialog-scrim",
            div { class: classes("dialog", &kind),
                // A click inside is not one on the shade around it, which closes.
                onclick: move |e| e.stop_propagation(),
                div { class: "dialog-title",
                    if let Some(icon) = icon {
                        IconView { icon, size: 16 }
                    }
                    "{title}"
                }
                {children}
            }
        }
    }
}
