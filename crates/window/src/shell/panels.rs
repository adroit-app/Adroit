//! The side panels' edges, which the pointer drags.

use adroit_core::msg::Msg;
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::layout::Side;

/// The edge of `side`'s panel next to the console: a drag resizes the panel,
/// a double-click brings back its width as designed.
#[component]
pub(crate) fn Splitter(side: Side) -> Element {
    let app = use_context::<AppCtx>();
    let title = match side {
        Side::Sidebar => "Drag to resize the tree; double-click for its width as designed",
        Side::Details => "Drag to resize the details panel; double-click for its width as designed",
    };
    rsx! {
        div {
            class: "panel-splitter",
            title,
            onmousedown: move |e: Event<MouseData>| {
                e.stop_propagation();
                e.prevent_default();
                app.dispatch(Msg::PanelDragStart { side, x: e.client_coordinates().x });
            },
            ondoubleclick: move |_| app.dispatch(Msg::PanelReset(side)),
        }
    }
}

/// The pointer moved over the window's body to `x`, carrying a panel's edge
/// if it holds one.
pub(crate) fn pointer_moved(app: AppCtx, x: f64) {
    if app.chrome.peek().panels.drag.is_some() {
        app.dispatch(Msg::PanelDragMove { x });
    }
}

/// The pointer let go, or left the window's body.
pub(crate) fn pointer_released(app: AppCtx) {
    if app.chrome.peek().panels.drag.is_some() {
        app.dispatch(Msg::PanelDragEnd);
    }
}
