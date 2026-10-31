//! The window's body between the header and the status bar: the tree, the
//! content and the details panel side by side, with the edges between them
//! and the drags across them.

use adroit_core::actions::Action;
use adroit_core::content::{Area, Axis};
use adroit_core::keymap::Context;
use adroit_core::msg::Msg;
use adroit_core::overlay::confirm::Ask;
use adroit_core::overlay::context_menu::Target;
use adroit_model::{Process, Session};
use dioxus::prelude::*;

use super::Ui;
use super::panels::{self, Splitter};
use crate::content::ContentPanel;
use crate::details::{self, Entry, Sheet};
use crate::layout::Side;
use crate::tree::{RowContext, RowEvents, TreePanel};

#[component]
pub(crate) fn Body() -> Element {
    let ui = use_context::<Ui>();
    let app = ui.app;
    let local = ui.local;
    // The finished sessions the user has seen, and the tabs with something
    // written for them kept aside.
    let completions = use_memo(move || app.tree.read().completions.clone());
    let drafted = use_memo(move || app.prompt.read().drafted.clone());
    let area = app.content.read().focus;
    let layout = app.chrome.read().panels.layout;
    let (waiting_prev, waiting_next) = {
        let chrome = app.chrome.read();
        let km = &chrome.keymap;
        (
            km.keys_for(Context::Tree, &Action::WaitingPrev),
            km.keys_for(Context::Tree, &Action::WaitingNext),
        )
    };
    let background = ui.hub.palette.read().background.hex();
    let class = match (
        app.chrome.read().panels.drag.is_some(),
        app.content.read().drag.as_ref().map(|d| d.border.axis),
    ) {
        (_, Some(Axis::Column)) => "app-body dragging rows",
        (true, _) | (_, Some(Axis::Row)) => "app-body dragging",
        _ => "app-body",
    };
    rsx! {
        div { class,
            onresize: move |e: Event<ResizeData>| {
                if let Ok(sz) = e.data().get_border_box_size() {
                    app.dispatch(Msg::BodyResized(sz.width));
                }
            },
            onmousemove: move |e: Event<MouseData>| {
                let dragging = app.content.peek().drag.as_ref().map(|d| d.border.axis);
                if let Some(axis) = dragging {
                    let at = e.client_coordinates();
                    let pos = if axis == Axis::Row { at.x } else { at.y };
                    app.dispatch(Msg::WindowDragMove(pos));
                }
                panels::pointer_moved(app, e.client_coordinates().x);
            },
            onmouseup: move |_| {
                if app.content.peek().drag.is_some() {
                    app.dispatch(Msg::WindowDragEnd);
                }
                panels::pointer_released(app);
            },
            onmouseleave: move |_| {
                if app.content.peek().drag.is_some() {
                    app.dispatch(Msg::WindowDragEnd);
                }
                panels::pointer_released(app);
            },
            TreePanel {
                focused: area == Area::Sidebar,
                context: RowContext {
                    completions: completions.into(),
                    drafted: drafted.into(),
                },
                on: RowEvents {
                    onopen: EventHandler::new(move |item| app.dispatch(Msg::RowClicked(item))),
                    onstart: EventHandler::new(move |item| {
                        app.dispatch(Msg::MenuRun { target: Target::Row(item), action: Action::SessionNew });
                    }),
                },
                field: local.filter_field,
                waiting_keys: (waiting_prev, waiting_next),
                ontaken: move |_| app.dispatch(Msg::Focus(Area::Sidebar)),
                onfilter: move |on| local.focus_filter(on),
                onworktrees: {
                    let ui = ui.clone();
                    move |_| ui.perform(Action::WorktreesToggle, None)
                },
                onpick: move |_| app.dispatch(Msg::PickerOpen),
            }
            if layout.show_sidebar {
                Splitter { side: Side::Sidebar }
            }
            ContentPanel {
                focused: matches!(area, Area::Console | Area::Prompt),
                viewer_command: local.viewer_command,
                gallery: local.gallery,
                completions,
                drafted,
                boxes: local.win_boxes,
                frame: local.frame,
                cell_width: (local.cell_width)(),
                background,
                ontabclose: move |target| app.dispatch(Msg::MenuRun { target, action: Action::TabClose }),
            }
            if layout.show_details {
                Splitter { side: Side::Details }
            }
            DetailsSide { focused: area == Area::Details, shown: layout.show_details }
        }
    }
}

/// The details panel at the window's right, for what the focused window
/// shows.
#[component]
fn DetailsSide(focused: bool, shown: bool) -> Element {
    let ui = use_context::<Ui>();
    let app = ui.app;
    // Read, so the last input shown follows a pane started.
    let _ = (ui.local.attached)();
    let current = app.content.read().described().cloned();
    let active = app.content.read().active().cloned();
    let outside_git = {
        let hosts = app.hosts.read();
        let work = hosts.work(hosts.environment_of(active.as_ref()));
        let tree = adroit_core::tree::listed(work);
        let workspace = current
            .as_ref()
            .and_then(|p| tree.workspace_of(&p.worktree));
        // A workspace outside git may still be a jj repository, which the
        // details panel reads through jj.
        current.as_ref().is_some_and(|p| {
            let jj = work
                .and_then(|w| w.place.of(&p.worktree))
                .is_some_and(|r| r.jj);
            workspace.is_some_and(|w| !w.git) && !jj
        })
    };
    let live_input_ms = active.as_ref().and_then(|tab| {
        ui.hub
            .panes
            .borrow()
            .get(tab)
            .and_then(|pane| pane.last_input_ms())
    });
    let agent = app.agent_of(active.as_ref());
    let usage_settings = app.config.read().settings.usage;
    let (sheet_keys, back_keys) = {
        let chrome = app.chrome.read();
        let km = &chrome.keymap;
        let sheet_keys = Sheet::ALL
            .map(|s| {
                km.keys_for(Context::Global, &details::sheet_action(s))
                    .unwrap_or_default()
            })
            .to_vec();
        let back_keys = km
            .keys_in(&[Context::Details, Context::Tree], &Action::CursorFold)
            .unwrap_or_default();
        (sheet_keys, back_keys)
    };
    let width = app.chrome.read().panels.drawn().1;
    rsx! {
        div { class: if focused { "app-panel details focused" } else { "app-panel details" },
            hidden: !shown,
            style: "width:{width}px",
            onmousedown: move |_| app.dispatch(Msg::Focus(Area::Details)),
            details::DetailsPanel {
                worktree: current.as_ref().map(|p| p.worktree.clone()),
                kind: if outside_git { details::Worktree::OutsideGit } else { details::Worktree::Repository },
                live_input_ms,
                agent,
                usage_settings,
                sheet_keys,
                back_keys,
                on_kill: move |(session, process): (Session, Process)| {
                    if let Some(ask) = Ask::kill_process(Some(&session), Some(&Entry::Process(process))) {
                        app.dispatch(Msg::Confirm(Box::new(ask)));
                    }
                },
                on_menu: move |m: details::LinkMenu| {
                    app.dispatch(Msg::MenuOpen { x: m.x, y: m.y, target: Target::Link { at: m.at, file: m.file } });
                },
            }
        }
    }
}
