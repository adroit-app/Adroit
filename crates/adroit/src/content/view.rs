//! The content's panel: the tabs bar over the shown tab's windows, each
//! where its share of the content puts it, and the borders between them.

use std::collections::{HashMap, HashSet};

use adroit_core::completion::Completions;
use adroit_core::msg::Msg;
use adroit_core::tree as tree_view;
use dioxus::prelude::*;

use super::bodies::{UnfocusedBody, ViewerWindow};
use super::tabs_bar::TabsBar;
use super::window::{ContentWindow, WindowBorder, window_strip};
use crate::actions::Action;
use crate::bridge::AppCtx;
use crate::content::{Tab, WindowId};
use crate::menu::Target;
use crate::rename::{Place as RenamePlace, Rename};
use crate::viewers;

/// The panel, drawn `focused` while the console or the prompt has the keys.
/// `focused_body` is what the focused window shows under its strip, the
/// console and the prompt, or the whole panel's body while nothing is open.
/// The other windows draw their programs at the sizes they put in `boxes`.
#[component]
pub(crate) fn ContentPanel(
    focused: bool,
    focused_body: Element,
    viewer_command: ReadSignal<Option<(WindowId, viewers::Command)>>,
    completions: ReadSignal<Completions>,
    drafted: ReadSignal<HashSet<Tab>>,
    renaming: Signal<Option<Rename>>,
    boxes: Signal<HashMap<WindowId, (f64, f64)>>,
    frame: Signal<u64>,
    cell_width: f64,
    background: String,
    onmenu: EventHandler<(f64, f64, Target)>,
    /// A tab's close button was pressed, after its tab was shown.
    ontabclose: EventHandler<Target>,
    onrename: EventHandler<(String, RenamePlace)>,
    onrenamed: EventHandler<Rename>,
) -> Element {
    let app = use_context::<AppCtx>();
    // How large the content's windows are drawn, which a border's drag is
    // measured against.
    let mut drawn = use_signal(|| (0.0f64, 0.0f64));
    let windows = app.content.read().windows.clone();
    let focused_window = windows.focused_window();
    let mut focused_body = Some(focused_body);
    let windows_el: Vec<Element> = match windows.current() {
        None => vec![focused_body.take().unwrap_or_else(|| rsx! {})],
        Some(tab) => {
            let host = app.host.read();
            let work = host.work.as_ref();
            let (t, terms) = (tree_view::listed(work), tree_view::terminals(work));
            let mut out = Vec::new();
            for (id, r) in tab.rects() {
                let Some(shows) = windows.shows(id).cloned() else {
                    continue;
                };
                let focused = Some(id) == focused_window;
                let strip = window_strip(t, terms, &completions.read(), &shows);
                // A viewer is drawn the same focused or not, so it keeps its
                // zoom, page and playback as the focus moves.
                let body = if let Tab::Viewer(path) = &shows {
                    rsx! {
                        ViewerWindow {
                            key: "{id}",
                            window: id,
                            path: path.clone(),
                            commands: viewer_command,
                            on_close: move |_| app.dispatch(Msg::Window(Action::WindowQuit)),
                        }
                    }
                } else if focused {
                    focused_body.take().unwrap_or_else(|| rsx! {})
                } else {
                    rsx! {
                        UnfocusedBody { id, shows, background: background.clone(), frame, cell_width, boxes }
                    }
                };
                out.push(rsx! {
                    ContentWindow { key: "{id}", id, r, focused, strip, body }
                });
            }
            for b in tab.borders() {
                let key = format!("border-{:?}-{}", b.split, b.after);
                out.push(rsx! {
                    WindowBorder { key: "{key}", border: b, drawn }
                });
            }
            out
        }
    };
    rsx! {
        div { class: if focused { "app-panel center-panel focused" } else { "app-panel center-panel" },
            onmousedown: move |_| app.dispatch(Msg::Focus(crate::content::Area::Console)),
            TabsBar { completions, drafted, renaming, onmenu, onclose: ontabclose, onrename, onrenamed }
            div { class: "content-windows",
                onresize: move |e: Event<ResizeData>| {
                    e.stop_propagation();
                    if let Ok(sz) = e.data().get_content_box_size() {
                        drawn.set((sz.width, sz.height));
                    }
                },
                for w in windows_el { {w} }
            }
        }
    }
}
