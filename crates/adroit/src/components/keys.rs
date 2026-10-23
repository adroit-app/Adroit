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

/// Keys and what they do, as a footer lists them.
#[component]
pub fn KeyHint(keys: String, does: String) -> Element {
    rsx! {
        span { class: "kbd-list", Keys { keys } " {does}" }
    }
}
