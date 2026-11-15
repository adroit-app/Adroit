use dioxus::prelude::*;

use super::classes;
use crate::icons::{Icon, IconView};

/// How large an icon button's icon is drawn, unless it says otherwise.
pub const ICON_BUTTON_SIZE: u32 = 12;

/// A button that is only an icon, `title` saying what it does; `place`
/// names the classes that set it where it sits, such as `tab-close`.
///
/// Pressing it leaves the focus where it was, so a click on a tab's or a
/// row's button does not take the keys from the console or the prompt.
#[component]
pub fn IconButton(
    icon: Icon,
    title: String,
    #[props(default)] place: String,
    #[props(default = ICON_BUTTON_SIZE)] size: u32,
    #[props(default)] aria_label: Option<String>,
    #[props(default)] onmousedown: Option<EventHandler<MouseEvent>>,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: classes("icon-button", &place),
            title,
            aria_label,
            onmousedown: move |e: MouseEvent| {
                e.prevent_default();
                if let Some(onmousedown) = onmousedown {
                    onmousedown.call(e);
                }
            },
            onclick: move |e| onclick.call(e),
            IconView { icon, size }
        }
    }
}
