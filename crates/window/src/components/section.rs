use dioxus::prelude::*;

/// A block of the details panel: what it is, `label`, above `children`.
#[component]
pub fn Section(label: String, children: Element) -> Element {
    rsx! {
        div { class: "details-block",
            span { class: "details-label", "{label}" }
            {children}
        }
    }
}
