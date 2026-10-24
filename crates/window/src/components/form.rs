use dioxus::prelude::*;

/// Settings that belong together, under `title` when they have one.
#[component]
pub fn SettingGroup(#[props(default)] title: Option<String>, children: Element) -> Element {
    rsx! {
        section { class: "settings-group",
            if let Some(title) = title {
                h2 { "{title}" }
            }
            {children}
        }
    }
}

/// Why something on the settings could not be done or read, in `children`.
#[component]
pub fn SettingsError(children: Element) -> Element {
    rsx! {
        div { class: "settings-error", {children} }
    }
}

/// One setting: what it is called and says on the left, its control on the
/// right.
#[component]
pub fn SettingRow(label: String, hint: String, children: Element) -> Element {
    rsx! {
        div { class: "settings-row",
            div { class: "settings-row-label",
                div { "{label}" }
                if !hint.is_empty() {
                    div { class: "settings-hint", "{hint}" }
                }
            }
            div { class: "settings-row-control", {children} }
        }
    }
}

/// A setting that is on or off, telling `onchange` which it became.
#[component]
pub fn Toggle(checked: bool, onchange: EventHandler<bool>) -> Element {
    rsx! {
        input {
            r#type: "checkbox",
            checked,
            onchange: move |e| onchange.call(e.checked()),
        }
    }
}

/// A line of text, telling `onchange` what it holds once it is left.
#[component]
pub fn TextField(
    value: String,
    #[props(default)] placeholder: Option<String>,
    /// The `datalist` offering what to type.
    #[props(default)]
    list: Option<String>,
    #[props(default)] aria_label: Option<String>,
    onchange: EventHandler<String>,
) -> Element {
    rsx! {
        input {
            class: "settings-command",
            r#type: "text",
            list,
            aria_label,
            value,
            placeholder,
            onchange: move |e| onchange.call(e.value()),
        }
    }
}

/// Lines of text, telling `onchange` what they hold once they are left.
#[component]
pub fn TextArea(
    value: String,
    #[props(default)] placeholder: Option<String>,
    onchange: EventHandler<String>,
) -> Element {
    rsx! {
        textarea {
            class: "settings-env",
            value,
            placeholder,
            onchange: move |e| onchange.call(e.value()),
        }
    }
}

/// A number from `min` to `max`, telling `onchange` what was typed.
#[component]
pub fn NumberField(
    value: String,
    min: String,
    max: String,
    #[props(default)] step: Option<String>,
    onchange: EventHandler<String>,
) -> Element {
    rsx! {
        input {
            r#type: "number",
            min,
            max,
            step,
            value,
            onchange: move |e| onchange.call(e.value()),
        }
    }
}

/// One of `choices`, each a value and what it is called, telling `onchange`
/// the value chosen. A `value` none of them has is offered first, so what
/// the settings hold still shows.
#[component]
pub fn Select(
    #[props(default)] label: Option<String>,
    value: String,
    choices: Vec<(&'static str, &'static str)>,
    onchange: EventHandler<String>,
) -> Element {
    rsx! {
        select {
            aria_label: label,
            onchange: move |e| onchange.call(e.value()),
            if !choices.iter().any(|(key, _)| *key == value) {
                option { value: value.clone(), selected: true, "{value}" }
            }
            for (key, name) in choices {
                option { key: "{key}", value: key, selected: value == key, "{name}" }
            }
        }
    }
}
