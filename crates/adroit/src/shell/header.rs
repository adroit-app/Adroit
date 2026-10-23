//! The header: the panels' switches, where the content's focused window is,
//! and how many sessions wait or work.

use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::icons::{Icon, IconView};
use crate::layout::Side;
use crate::settings_window;

/// Where the focused window's session works: its workspace, by name and
/// whether it is a scratch one, its worktree as the header writes it, and
/// what the window shows.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Crumbs {
    pub(crate) workspace: Option<(String, bool)>,
    pub(crate) worktree: String,
    pub(crate) target: String,
}

/// A click on a panel's switch goes to `ontoggle`, which also moves the keys
/// out of a panel it hides.
#[component]
pub(crate) fn Header(
    crumbs: Option<Crumbs>,
    waiting: usize,
    working: usize,
    ontoggle: EventHandler<Side>,
) -> Element {
    let app = use_context::<AppCtx>();
    let layout = app.chrome.read().panels.layout;
    rsx! {
        div { class: "header",
            span {
                class: if layout.show_sidebar { "pane-toggle on" } else { "pane-toggle" },
                title: "Show or hide the tree (⌘B)",
                onclick: move |_| ontoggle.call(Side::Sidebar),
                IconView { icon: Icon::LayoutSidebar, size: 16 }
            }
            div { class: "header-crumbs",
                if let Some(c) = &crumbs {
                    if let Some((name, scratch)) = &c.workspace {
                        span { class: "header-crumbs-item", IconView { icon: if *scratch { Icon::Notes } else { Icon::Folder }, size: 14 } "{name}" }
                        span { class: "header-crumbs-sep", IconView { icon: Icon::ChevronRight, size: 12 } }
                    }
                    span { class: "header-crumbs-item header-crumbs-path", IconView { icon: Icon::GitBranch, size: 14 } "{c.worktree}" }
                    span { class: "header-crumbs-sep", IconView { icon: Icon::ChevronRight, size: 12 } }
                    span { "{c.target}" }
                }
            }
            div { class: "flex-spacer" }
            if waiting > 0 {
                span { class: "header-chip wait", span { class: "state-glyph wait", IconView { icon: Icon::Bell, size: 12 } } "{waiting} waiting" }
            }
            if working > 0 {
                span { class: "header-chip", span { class: "state-glyph run", IconView { icon: Icon::Loader, size: 12 } } "{working} working" }
            }
            span {
                class: if layout.show_details { "pane-toggle on" } else { "pane-toggle" },
                title: "Show or hide the details panel (⌥⌘B)",
                onclick: move |_| ontoggle.call(Side::Details),
                IconView { icon: Icon::LayoutSidebarRight, size: 16 }
            }
            span {
                class: "pane-toggle",
                title: "Settings (⌘,)",
                onclick: move |_| settings_window::open(),
                IconView { icon: Icon::Settings, size: 16 }
            }
        }
    }
}
