//! A window that is not focused: the screen of the session or the shell it
//! shows at the window's own size, or a draft's heading.

use std::collections::HashMap;

use dioxus::prelude::*;

use super::{DraftHead, Starting};
use crate::console::{ConsoleScreen, Hub};
use crate::content::{Tab, WindowId};

/// The body of the window `id`, which shows `shows`, measured into `boxes`
/// so its program is sized to it.
#[component]
pub(crate) fn UnfocusedBody(
    id: WindowId,
    shows: Tab,
    background: String,
    frame: Signal<u64>,
    cell_width: f64,
    boxes: Signal<HashMap<WindowId, (f64, f64)>>,
) -> Element {
    let hub = use_context::<Hub>();
    let has = hub.panes.borrow().contains_key(&shows);
    let mut boxes = boxes;
    rsx! {
        div {
            class: "console",
            style: "--console-bg:{background};background:{background}",
            onresize: move |e: Event<ResizeData>| {
                e.stop_propagation();
                if let Ok(sz) = e.data().get_content_box_size() {
                    let size = (sz.width, sz.height);
                    if boxes.peek().get(&id) != Some(&size) {
                        boxes.write().insert(id, size);
                    }
                }
            },
            match &shows {
                Tab::Draft { worktree, .. } => rsx! {
                    DraftHead { worktree: worktree.clone() }
                },
                _ if has => rsx! {
                    ConsoleScreen { tab: shows.clone(), frame, cell_width, focused: false }
                },
                _ => rsx! {
                    Starting {}
                },
            }
        }
    }
}
