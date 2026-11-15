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

/// A button beside a fact's value that copies `text`, shown while the
/// pointer is over the value; it says so for a moment once it copied.
#[component]
pub fn CopyValue(text: String) -> Element {
    let mut copied = use_signal(|| false);
    rsx! {
        button {
            class: if copied() { "facts-copy copied" } else { "facts-copy" },
            r#type: "button",
            title: "Copy",
            aria_label: "Copy {text}",
            tabindex: "-1",
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                if crate::clipboard::copy_to_clipboard(&text).is_ok() {
                    copied.set(true);
                    spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                        copied.set(false);
                    });
                }
            },
            crate::icons::IconView {
                icon: if copied() { crate::icons::Icon::Check } else { crate::icons::Icon::Copy },
                size: 11,
            }
        }
    }
}
