use dioxus::prelude::*;

/// Keys as the bindings write them, a cap for each key of the sequence.
#[component]
pub fn Keys(keys: String) -> Element {
    rsx! {
        for (i , chord) in keys.split(' ').map(str::to_string).enumerate() {
            kbd { key: "{i}", "{chord}" }
        }
    }
}

/// One entry of a footer's list: keys and what they do, or what `children`
/// say alone.
#[component]
pub fn Hint(children: Element) -> Element {
    rsx! {
        span { class: "kbd-list", {children} }
    }
}

/// Keys and what they do, as a footer lists them.
#[component]
pub fn KeyHint(keys: String, does: String) -> Element {
    rsx! {
        Hint { Keys { keys } " {does}" }
    }
}

/// A table of keys and what they do, each row a [`LegendRow`].
#[component]
pub fn Legend(children: Element) -> Element {
    rsx! {
        div { class: "console-legend", {children} }
    }
}

/// The keys `children` draw, and what they do, `does`.
#[component]
pub fn LegendRow(does: String, children: Element) -> Element {
    rsx! {
        div { class: "console-legend-keys", {children} }
        div { "{does}" }
    }
}
