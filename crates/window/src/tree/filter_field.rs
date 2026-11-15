//! The filter at the top of the tree, which narrows the rows as it is typed
//! into.

use std::rc::Rc;

use adroit_core::msg::Msg;
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::components::InlineField;
use crate::icons::{Icon, IconView};

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
            InlineField {
                class: "filter-field-input",
                placeholder: "Filter",
                value: text,
                onmounted: move |data| field.set(Some(data)),
                onfocus: move |()| {
                    ontaken.call(());
                    app.dispatch(Msg::TreeFiltering(true));
                },
                onblur: move |()| app.dispatch(Msg::TreeFiltering(false)),
                oninput: move |text| app.dispatch(Msg::TreeFilter(text)),
                onenter: move |()| onfocus.call(false),
                onescape: move |()| {
                    app.dispatch(Msg::TreeFilter(String::new()));
                    onfocus.call(false);
                },
            }
        }
    }
}
