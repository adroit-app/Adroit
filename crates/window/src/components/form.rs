use dioxus::prelude::*;

/// Settings that belong together, under `title` when they have one.
#[component]
pub fn SettingGroup(#[props(default)] title: Option<String>, children: Element) -> Element {
    rsx! {
        section { class: "settings-section",
            if let Some(title) = title {
                h2 { class: "settings-section-title", "{title}" }
            }
            div { class: "settings-group", {children} }
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
            class: "switch",
            r#type: "checkbox",
            role: "switch",
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

/// A value and what it is called, one of a [`Select`]'s choices.
pub type Choice = (String, String);

/// `items`, each a value and what it is called, as a [`Select`]'s choices.
pub fn choices_of<V: ToString, N: ToString>(
    items: impl IntoIterator<Item = (V, N)>,
) -> Vec<Choice> {
    items
        .into_iter()
        .map(|(value, name)| (value.to_string(), name.to_string()))
        .collect()
}

/// Whether `value` is one of `choices` or of the choices of `groups`.
fn offers(value: &str, choices: &[Choice], groups: &[(String, Vec<Choice>)]) -> bool {
    choices
        .iter()
        .chain(groups.iter().flat_map(|(_, g)| g))
        .any(|(key, _)| key == value)
}

/// One of `choices`, each a value and what it is called, or of `groups`,
/// each choices under a title, telling `onchange` the value chosen. A
/// `value` none of them has is offered first, so what the settings hold
/// still shows.
#[component]
pub fn Select(
    #[props(default)] label: Option<String>,
    #[props(default)] class: Option<String>,
    #[props(default)] disabled: bool,
    value: String,
    #[props(default)] choices: Vec<Choice>,
    #[props(default)] groups: Vec<(String, Vec<Choice>)>,
    onchange: EventHandler<String>,
) -> Element {
    let held = !offers(&value, &choices, &groups);
    rsx! {
        select {
            aria_label: label,
            class,
            disabled,
            onchange: move |e| onchange.call(e.value()),
            if held {
                option { value: value.clone(), selected: true, "{value}" }
            }
            for (key, name) in choices {
                option { key: "{key}", selected: value == key, value: key, "{name}" }
            }
            for (title, group) in groups {
                optgroup { key: "{title}", label: title,
                    for (key, name) in group {
                        option { key: "{key}", selected: value == key, value: key, "{name}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_among_the_choices_is_offered() {
        let choices = choices_of([("on", "On"), ("off", "Off")]);
        assert!(offers("off", &choices, &[]));
    }

    #[test]
    fn a_value_among_a_groups_choices_is_offered() {
        let groups = vec![("Cursor".to_string(), choices_of([("cursor.down", "Down")]))];
        assert!(offers("cursor.down", &[], &groups));
    }

    #[test]
    fn a_value_no_choice_has_is_not_offered() {
        let choices = choices_of([("on", "On")]);
        let groups = vec![("Cursor".to_string(), choices_of([("cursor.down", "Down")]))];
        assert!(!offers("sideways", &choices, &groups));
    }
}
