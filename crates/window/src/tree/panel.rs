//! The tree's panel: its bar with the filter and the buttons that list
//! every worktree and start a session, the rows, and the sessions waiting
//! on the user below them.

use std::rc::Rc;

use adroit_core::tree;
use dioxus::prelude::*;

use super::filter_field::FilterField;
use super::row::{RowContext, RowEvents, TreeRow, WaitingRow};
use crate::bridge::AppCtx;
use crate::components::{Hint, Keys, ListRow};
use crate::icons::{Icon, IconView};
use crate::nav::Item;

/// The panel, drawn `focused` while it has the keys. `waiting_keys` are the
/// keys that go to the previous and the next session waiting.
#[component]
pub(crate) fn TreePanel(
    focused: bool,
    context: RowContext,
    on: RowEvents,
    field: Signal<Option<Rc<MountedData>>>,
    waiting_keys: (Option<String>, Option<String>),
    /// The panel was pressed on, or its filter took the keys.
    ontaken: EventHandler<()>,
    onfilter: EventHandler<bool>,
    onworktrees: EventHandler<()>,
    onpick: EventHandler<()>,
) -> Element {
    let app = use_context::<AppCtx>();
    let (shown, width) = {
        let chrome = app.chrome.read();
        (chrome.panels.layout.show_sidebar, chrome.panels.drawn().0)
    };
    let (rows, waiting, every) = {
        let view = app.tree.read();
        (
            view.rows.clone(),
            view.narrowed_waiting.clone(),
            view.every_worktree,
        )
    };
    let empty = {
        let host = app.host.read();
        tree::listed(host.work.as_ref()).workspaces.is_empty()
    };
    let (waiting_prev, waiting_next) = waiting_keys;
    rsx! {
        div { class: if focused { "app-panel sidebar focused" } else { "app-panel sidebar" },
            hidden: !shown,
            style: "width:{width}px",
            onmousedown: move |_| ontaken.call(()),
            div { class: "sidebar-bar",
                FilterField { field, onfocus: onfilter, ontaken }
                span { class: "sidebar-tools",
                    button {
                        class: if every { "sidebar-tool on" } else { "sidebar-tool" },
                        "aria-pressed": every,
                        title: if every { "List only the worktrees with sessions" } else { "List every worktree, those without sessions too" },
                        onclick: move |_| onworktrees.call(()),
                        IconView { icon: Icon::ListTree }
                    }
                    span {
                        class: "sidebar-tool",
                        title: "Start a session: choose where it works",
                        onclick: move |_| onpick.call(()),
                        IconView { icon: Icon::Plus }
                    }
                }
            }
            div { class: "list-rows", style: "flex:1",
                if empty {
                    ListRow { kind: "dim", "No sessions yet" }
                }
                for item in rows.into_iter().filter(|i| !matches!(i, Item::Waiting(_))) {
                    TreeRow { key: "{item:?}", item, context, on }
                }
            }
            if !waiting.is_empty() {
                div { class: "sidebar-divider" }
                div { class: "sidebar-title",
                    span { "Waiting on you" }
                    Hint {
                        if let Some(k) = waiting_prev { Keys { keys: k } }
                        if let Some(k) = waiting_next { Keys { keys: k } }
                    }
                }
                div { class: "list-rows sidebar-inbox",
                    for it in waiting {
                        WaitingRow { key: "w-{it.session.id}", waiting: it, on }
                    }
                }
            }
        }
    }
}
