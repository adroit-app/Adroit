use dioxus::prelude::*;

/// Facts in two columns, each a [`Fact`].
#[component]
pub fn Facts(children: Element) -> Element {
    rsx! {
        div { class: "details-facts", {children} }
    }
}

/// One fact: what it is, `label`, and its value in `children`.
#[component]
pub fn Fact(label: String, children: Element) -> Element {
    rsx! {
        span { class: "details-facts-key", "{label}" }
        {children}
    }
}
