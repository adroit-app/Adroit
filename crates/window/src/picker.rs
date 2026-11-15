//! The picker drawn over the window, whichever is open. It only draws: the
//! view state holds it, and its keys go there.

use adroit_core::msg::Msg;
use adroit_core::overlay::hints;
use adroit_core::picker::Picking;
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::components::{InlineField, KeyHint};

mod go;
mod interrupted;
mod resume;

#[component]
pub(crate) fn Picker() -> Element {
    let app = use_context::<AppCtx>();
    let open = app.picker.read().open.clone();
    // The row the cursor is on is kept in view, after it is drawn: the
    // list's, and the file tree's, whose cursor is not the list's.
    let cursor = use_memo(move || app.picker.read().open.clone());
    use_effect(move || {
        let _ = cursor();
        document::eval(
            "requestAnimationFrame(() => document.querySelectorAll('.dialog.picker-dialog .list-row.selected')
               .forEach((r) => r.scrollIntoView({ block: 'nearest' })));",
        );
    });
    rsx! {
        if let Some(open) = open {
            match open {
                Picking::Go(picker) => rsx! { go::Go { picker: *picker } },
                Picking::Resume(list) => rsx! { resume::Resume { list } },
                Picking::Interrupted(list) => rsx! { interrupted::Interrupted { list } },
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
        InlineField {
            class: "filter-field-input",
            value: query,
            placeholder,
            focus: true,
            oninput: move |text| app.dispatch(Msg::PickerQuery(text)),
            onkey: move |k| app.dispatch(Msg::PickerKey(k)),
        }
    }
}

/// The keys the picker open takes, each with what it does, along its foot.
#[component]
fn PickerKeys() -> Element {
    let app = use_context::<AppCtx>();
    let hints = match &app.picker.read().open {
        Some(open) => hints::picker(&app.chrome.read().keymap, open),
        None => Vec::new(),
    };
    rsx! {
        div { class: "picker-keys",
            for hint in hints {
                KeyHint { key: "{hint.keys}", keys: hint.keys.clone(), does: hint.does }
            }
        }
    }
}
