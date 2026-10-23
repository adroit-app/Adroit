//! The filter at the top of the tree, which narrows the rows as it is typed
//! into.

use std::rc::Rc;

use adroit_core::msg::Msg;
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::icons::{Icon, IconView};
use crate::keys;

/// The field mounts into `field`, through which the window gives it the
/// keys and takes them back; `onfocus` asks for that.
#[component]
pub(crate) fn FilterField(
    field: Signal<Option<Rc<MountedData>>>,
    onfocus: EventHandler<bool>,
    ontaken: EventHandler<()>,
) -> Element {
    let app = use_context::<AppCtx>();
    let (on, text) = {
        let view = app.tree.read();
        (view.filtering, view.filter.clone())
    };
    rsx! {
        div {
            class: if on { "filter-field on" } else { "filter-field" },
            title: "Narrow the list by name; Enter keeps it, Esc clears it",
            // A click goes on with what it holds, where `/` starts afresh.
            onclick: move |_| onfocus.call(true),
            span { class: "filter-field-funnel", IconView { icon: Icon::Filter, size: 13 } }
            input {
                class: "filter-field-input",
                placeholder: "Filter",
                value: "{text}",
                autocomplete: "off",
                spellcheck: "false",
                "autocorrect": "off",
                "autocapitalize": "off",
                onmounted: move |e: MountedEvent| field.set(Some(e.data())),
                onfocus: move |_| {
                    ontaken.call(());
                    app.dispatch(Msg::TreeFiltering(true));
                },
                onblur: move |_| app.dispatch(Msg::TreeFiltering(false)),
                oninput: move |e: Event<FormData>| app.dispatch(Msg::TreeFilter(e.value())),
                onkeydown: move |e: Event<KeyboardData>| {
                    match keys::of_field(&e).map(|k| k.key).as_deref() {
                        Some("Enter") => onfocus.call(false),
                        Some("Escape") => {
                            app.dispatch(Msg::TreeFilter(String::new()));
                            onfocus.call(false);
                        }
                        _ => {}
                    }
                },
            }
        }
    }
}
