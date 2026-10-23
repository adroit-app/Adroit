use dioxus::prelude::*;

use super::classes;

/// What a console shows where it has nothing to show: `title` above what
/// `children` say, in the look `kind` names.
#[component]
pub fn EmptyState(
    #[props(default)] kind: String,
    #[props(default)] title: Option<String>,
    children: Element,
) -> Element {
    rsx! {
        div { class: classes("console-empty", &kind),
            if let Some(title) = title {
                div { class: "console-empty-title", "{title}" }
            }
            {children}
        }
    }
}
