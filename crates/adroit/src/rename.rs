//! Editing a session name where it is shown.

use adroit_core::msg::Msg;
use dioxus::prelude::*;

pub use adroit_core::overlay::rename::*;

use crate::bridge::AppCtx;

#[component]
pub fn SessionName(id: String, name: String, place: Place) -> Element {
    let app = use_context::<AppCtx>();
    let rename = app
        .overlays
        .read()
        .rename
        .clone()
        .filter(|r| r.is_for(&id, place));
    let finish = {
        let id = id.clone();
        move |commit: bool| {
            app.dispatch(Msg::RenameEnd {
                id: id.clone(),
                place,
                commit,
            })
        }
    };
    rsx! {
        if let Some(rename) = rename {
            input {
                class: "session-rename-input",
                value: "{rename.text}",
                aria_label: "Session name",
                autocomplete: "off",
                spellcheck: "false",
                "autocorrect": "off",
                "autocapitalize": "off",
                onmounted: move |e: MountedEvent| async move {
                    let _ = e.data().set_focus(true).await;
                    document::eval("document.querySelector('.session-rename-input')?.select()");
                },
                onmousedown: move |e: Event<MouseData>| e.stop_propagation(),
                onclick: move |e: Event<MouseData>| e.stop_propagation(),
                ondoubleclick: move |e: Event<MouseData>| e.stop_propagation(),
                oninput: {
                    let id = id.clone();
                    move |e: Event<FormData>| {
                        app.dispatch(Msg::RenameText { id: id.clone(), place, text: e.value() })
                    }
                },
                onblur: {
                    let finish = finish.clone();
                    move |_| finish(true)
                },
                onkeydown: move |e: Event<KeyboardData>| {
                    e.stop_propagation();
                    match crate::keys::of_field(&e).map(|k| k.key).as_deref() {
                        Some("Enter") => { e.prevent_default(); finish(true); }
                        Some("Escape") => { e.prevent_default(); finish(false); }
                        _ => {}
                    }
                },
            }
        } else {
            span {
                class: "session-name",
                ondoubleclick: move |e: Event<MouseData>| {
                    e.stop_propagation();
                    app.dispatch(Msg::RenameStart { id: id.clone(), place });
                },
                "{name}"
            }
        }
    }
}
