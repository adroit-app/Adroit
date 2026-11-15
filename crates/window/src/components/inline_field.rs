use std::rc::Rc;

use adroit_model::Key;
use dioxus::prelude::*;

use crate::keys;

/// A one-line text field, `oninput` given what it holds as it is typed in.
///
/// It never completes, corrects or capitalises what is typed: what goes in
/// is a name, a path or a query, not prose.
///
/// - `focus` takes the keys once it is drawn, before `onmounted` is called.
/// - `contained` keeps clicks and keys from reaching what it sits in, such
///   as the tree row it edits.
/// - `onenter` and `onescape` are given those keys; `onkey`, every other
///   key that does not edit the text, such as Tab or the arrows, which would
///   otherwise move the focus or the caret.
#[component]
pub fn InlineField(
    class: String,
    value: String,
    #[props(default)] aria_label: Option<String>,
    #[props(default)] placeholder: Option<String>,
    #[props(default)] focus: bool,
    #[props(default)] contained: bool,
    #[props(default)] onmounted: Option<EventHandler<Rc<MountedData>>>,
    #[props(default)] onfocus: Option<EventHandler<()>>,
    #[props(default)] onblur: Option<EventHandler<()>>,
    #[props(default)] onenter: Option<EventHandler<()>>,
    #[props(default)] onescape: Option<EventHandler<()>>,
    #[props(default)] onkey: Option<EventHandler<Key>>,
    oninput: EventHandler<String>,
) -> Element {
    let contain = move |e: Event<MouseData>| {
        if contained {
            e.stop_propagation();
        }
    };
    rsx! {
        input {
            r#type: "text",
            class,
            value,
            aria_label,
            placeholder,
            autocomplete: "off",
            spellcheck: "false",
            "autocorrect": "off",
            "autocapitalize": "off",
            onmounted: move |e: MountedEvent| async move {
                if focus {
                    let _ = e.data().set_focus(true).await;
                }
                if let Some(onmounted) = onmounted {
                    onmounted.call(e.data());
                }
            },
            onmousedown: contain,
            onclick: contain,
            ondoubleclick: contain,
            onfocus: move |_| {
                if let Some(onfocus) = onfocus {
                    onfocus.call(());
                }
            },
            onblur: move |_| {
                if let Some(onblur) = onblur {
                    onblur.call(());
                }
            },
            oninput: move |e: Event<FormData>| oninput.call(e.value()),
            onkeydown: move |e: Event<KeyboardData>| {
                if contained {
                    e.stop_propagation();
                }
                let Some(key) = keys::of_field(&e) else {
                    return;
                };
                let named = match key.key.as_str() {
                    "Enter" => onenter,
                    "Escape" => onescape,
                    _ => None,
                };
                if let Some(named) = named {
                    e.prevent_default();
                    named.call(());
                } else if let Some(onkey) = onkey.filter(|_| !keys::edits_text(&key)) {
                    e.prevent_default();
                    onkey.call(key);
                }
            },
        }
    }
}
