//! Editing a session's name, or an environment's, where it is shown.

use adroit_core::msg::Msg;
use adroit_model::EnvironmentId;
use dioxus::prelude::*;

pub use adroit_core::overlay::rename::*;

use crate::bridge::AppCtx;
use crate::components::InlineField;

#[component]
pub fn SessionName(environment: EnvironmentId, id: String, name: String, place: Place) -> Element {
    let app = use_context::<AppCtx>();
    let rename = app
        .overlays
        .read()
        .rename
        .clone()
        .filter(|r| r.is_for(&environment, &id, place));
    let finish = {
        let (environment, id) = (environment.clone(), id.clone());
        move |commit: bool| {
            app.dispatch(Msg::RenameEnd {
                environment: environment.clone(),
                id: id.clone(),
                place,
                commit,
            })
        }
    };
    rsx! {
        if let Some(rename) = rename {
            InlineField {
                class: "session-rename-input",
                value: rename.text,
                aria_label: if place == Place::Heading { "Environment name" } else { "Session name" },
                focus: true,
                contained: true,
                onmounted: move |_| {
                    document::eval("document.querySelector('.session-rename-input')?.select()");
                },
                oninput: {
                    let (environment, id) = (environment.clone(), id.clone());
                    move |text: String| {
                        app.dispatch(Msg::RenameText {
                            environment: environment.clone(),
                            id: id.clone(),
                            place,
                            text,
                        })
                    }
                },
                onenter: {
                    let finish = finish.clone();
                    move |()| finish(true)
                },
                onescape: {
                    let finish = finish.clone();
                    move |()| finish(false)
                },
                onblur: move |()| finish(false),
            }
        } else {
            span {
                class: "session-name",
                ondoubleclick: move |e: Event<MouseData>| {
                    e.stop_propagation();
                    app.dispatch(Msg::RenameStart {
                        environment: environment.clone(),
                        id: id.clone(),
                        place,
                    });
                },
                "{name}"
            }
        }
    }
}
