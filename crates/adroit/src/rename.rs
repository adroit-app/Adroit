//! Editing a session name where it is shown.

use dioxus::prelude::*;

pub use adroit_core::rename::*;

#[component]
pub fn SessionName(
    id: String,
    name: String,
    place: Place,
    mut editing: Signal<Option<Rename>>,
    onstart: EventHandler<(String, Place)>,
    onfinish: EventHandler<Rename>,
) -> Element {
    let rename = editing().filter(|r| r.is_for(&id, place));
    let finish = {
        let id = id.clone();
        EventHandler::new(move |commit: bool| {
            if let Some(rename) = Rename::take(&mut editing.write(), &id, place)
                && commit
            {
                onfinish.call(rename);
            }
        })
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
                oninput: move |e: Event<FormData>| {
                    if let Some(r) = editing.write().as_mut().filter(|r| r.is_for(&id, place)) { r.text = e.value(); }
                },
                onblur: move |_| finish.call(true),
                onkeydown: move |e: Event<KeyboardData>| {
                    e.stop_propagation();
                    match crate::keys::of_field(&e).map(|k| k.key).as_deref() {
                        Some("Enter") => { e.prevent_default(); finish.call(true); }
                        Some("Escape") => { e.prevent_default(); finish.call(false); }
                        _ => {}
                    }
                },
            }
        } else {
            span {
                class: "session-name",
                ondoubleclick: move |e: Event<MouseData>| {
                    e.stop_propagation();
                    onstart.call((id.clone(), place));
                },
                "{name}"
            }
        }
    }
}
