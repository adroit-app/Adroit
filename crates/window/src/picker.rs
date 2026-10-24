//! The picker drawn over the window, whichever is open. It only draws: the
//! view state holds it, and its keys go there.

use adroit_core::msg::Msg;
use adroit_core::picker::Picking;
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::components::KeyHint;
use crate::keys;

mod go;
mod resume;

#[component]
pub(crate) fn Picker() -> Element {
    let app = use_context::<AppCtx>();
    let open = app.picker.read().open.clone();
    // The row the cursor is on is kept in view, after it is drawn.
    let cursor = use_memo(move || app.picker.read().open.as_ref().map(Picking::cursor));
    use_effect(move || {
        let _ = cursor();
        document::eval(
            "requestAnimationFrame(() => document.querySelectorAll('.dialog.picker-dialog .list-row.selected')
               .forEach((r) => r.scrollIntoView({ block: 'nearest' })));",
        );
    });
    rsx! {
        if let Some(open) = open {
            div {
                onclick: move |_| app.dispatch(Msg::PickerClose),
                match open {
                    Picking::Go(picker) => rsx! { go::Go { picker: *picker } },
                    Picking::Resume(list) => rsx! { resume::Resume { list } },
                }
            }
        }
    }
}

/// The picker's query field, which takes the keys while it is open: what it
/// does not type with goes to the picker.
#[component]
fn QueryField(query: String, #[props(default)] placeholder: String) -> Element {
    let app = use_context::<AppCtx>();
    rsx! {
        input {
            class: "filter-field-input",
            value: "{query}",
            placeholder: "{placeholder}",
            autocomplete: "off",
            spellcheck: "false",
            "autocorrect": "off",
            "autocapitalize": "off",
            onmounted: move |e: MountedEvent| async move {
                let _ = e.data().set_focus(true).await;
            },
            oninput: move |e: Event<FormData>| app.dispatch(Msg::PickerQuery(e.value())),
            onkeydown: move |e: Event<KeyboardData>| {
                if let Some(k) = keys::of_field(&e).filter(|k| !keys::edits_text(k)) {
                    // Tab and the arrows would move the focus or the caret.
                    e.prevent_default();
                    app.dispatch(Msg::PickerKey(k));
                }
            },
        }
    }
}

/// The keys a picker takes, each with what it does, along its foot.
#[component]
fn PickerKeys(keys: Vec<(String, String)>) -> Element {
    rsx! {
        div { class: "picker-keys",
            for (k, what) in keys.into_iter() {
                KeyHint { key: "{k}", keys: k.clone(), does: what }
            }
        }
    }
}
