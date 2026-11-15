use dioxus::prelude::*;

use super::classes;
use crate::icons::{Icon, IconView};

/// A dialog over the window, shaded around: `title` at its head, with
/// `icon` before it, and `children` below; with no `title`, `children` draw
/// the head themselves and `label` names the dialog. `kind` names a look of
/// its own. A click on the shade runs `onclose`; so does Escape inside it
/// with `escapes`, for a dialog whose buttons take the focus, since the
/// window leaves a dialog's keys to it.
#[component]
pub fn Dialog(
    #[props(default)] kind: String,
    #[props(default)] icon: Option<Icon>,
    #[props(default)] title: String,
    #[props(default)] label: String,
    #[props(default)] escapes: bool,
    #[props(default)] onclose: Option<EventHandler<()>>,
    children: Element,
) -> Element {
    let named = if label.is_empty() {
        title.clone()
    } else {
        label
    };
    rsx! {
        div {
            class: "dialog-scrim",
            onclick: move |_| {
                if let Some(close) = onclose {
                    close.call(());
                }
            },
            div {
                class: classes("dialog", &kind),
                role: "dialog",
                aria_label: "{named}",
                // A click inside is not one on the shade around it.
                onclick: move |e| e.stop_propagation(),
                onkeydown: move |e: Event<KeyboardData>| {
                    if escapes
                        && let Some(close) = onclose
                        && crate::keys::of_field(&e).is_some_and(|k| k.key == "Escape")
                    {
                        e.prevent_default();
                        close.call(());
                    }
                },
                if !title.is_empty() {
                    div { class: "dialog-title",
                        if let Some(icon) = icon {
                            IconView { icon, size: 16 }
                        }
                        "{title}"
                    }
                }
                {children}
            }
        }
    }
}
