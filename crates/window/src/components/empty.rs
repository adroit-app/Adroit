use dioxus::prelude::*;

use super::classes;
use crate::icons::{Icon, IconView};

/// What a console shows where it has nothing to show: `icon` and `title`
/// above what `children` say, in the look `kind` names.
#[component]
pub fn EmptyState(
    #[props(default)] kind: String,
    #[props(default)] icon: Option<Icon>,
    #[props(default)] title: Option<String>,
    children: Element,
) -> Element {
    rsx! {
        div { class: classes("console-empty", &kind),
            if let Some(icon) = icon {
                IconView { icon, size: 28 }
            }
            if let Some(title) = title {
                div { class: "console-empty-title", "{title}" }
            }
            {children}
        }
    }
}
