//! The tabs bar: the worktree in the middle by name, and a tab for each tab
//! of the content, named after what its focused window shows.

use std::collections::HashSet;

use adroit_core::completion::Completions;
use adroit_core::msg::Msg;
use adroit_core::tree as tree_view;
use adroit_model::AgentPermissions;
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;

use super::short;
use crate::bridge::AppCtx;
use crate::components::{
    DraftMark, PlaceMark, SessionMark, StateGlyph, TerminalMark, glyph, runs_of, session_mark,
};
use crate::content::{DropAt, Tab, WindowId, thing_name};
use crate::icons::{Icon, IconView};
use crate::nav;
use crate::rename::{Place as RenamePlace, SessionName};
use crate::shell::overlays::menu_opener;
use adroit_core::overlay::context_menu::Target;

/// The bar. `drafted` are the tabs with something written for them kept
/// aside.
#[component]
pub(super) fn TabsBar(
    completions: ReadSignal<Completions>,
    drafted: ReadSignal<HashSet<Tab>>,
    onclose: EventHandler<Target>,
) -> Element {
    let app = use_context::<AppCtx>();
    // Each tab named after what its focused window shows.
    let tabs = use_memo(move || {
        let hosts = app.hosts.read();
        let view = app.content.read();
        view.windows
            .tabs()
            .iter()
            .map(|tab| {
                let shows = tab.focused().shows.clone();
                let work = hosts.work(shows.environment());
                let (t, terms) = (tree_view::listed(work), tree_view::terminals(work));
                let (state, mark) = match &shows {
                    Tab::Session(_, id) => nav::find(t, id).map_or((None, None), |(_, s)| {
                        (
                            Some(completions.read().state(s).clone()),
                            session_mark(s.reads_only()),
                        )
                    }),
                    Tab::Draft { .. } => {
                        let reader = view
                            .drafts
                            .options
                            .get(&shows)
                            .is_some_and(|o| o.permissions == AgentPermissions::ReadOnly);
                        (None, session_mark(reader))
                    }
                    Tab::Terminal(_, _) | Tab::Viewer(_, _) | Tab::Drawn { .. } => (None, None),
                };
                let name = thing_name(t, terms, &shows);
                (
                    shows,
                    name,
                    state,
                    mark,
                    tab.windows().len(),
                    tab.is_preview(),
                    tab.focused().id,
                )
            })
            .collect::<Vec<_>>()
    });
    let current = app.content.read().described().cloned();
    let shown_at = app.content.read().windows.current_index();
    // While a window is dragged, the tab it would go into or the place a tab
    // dragged would take, if any, and the place of the tab dragged.
    let (dropping, from) = app
        .content
        .read()
        .moving
        .as_ref()
        .filter(|m| m.dragging)
        .map_or((None, None), |m| (Some(m.over), m.tab));
    let dragging = dropping.is_some();
    // A tab dragged among the tabs is drawn where it would land, the others
    // making room, so the order it leaves is seen before it is let go.
    let rows = tabs();
    let ordinary = rows.iter().filter(|r| !r.5).count();
    let mut order: Vec<usize> = (0..rows.len()).collect();
    let ghost = match (dropping, from) {
        (Some(Some(DropAt::Place(to))), Some(from)) if from < ordinary && to < ordinary => {
            let tab = order.remove(from);
            order.insert(to, tab);
            Some(from)
        }
        _ => None,
    };
    let laid: Vec<_> = order
        .into_iter()
        .enumerate()
        .map(|(place, at)| (place, at, rows[at].clone()))
        .collect();
    let hosts = app.hosts.read();
    rsx! {
        div { class: "tabs",
            // The worktree by name only: the header above holds its whole
            // path, and here it would crowd the tabs out.
            if let Some(p) = &current {
                span { class: "tabs-where", title: "{short(&p.worktree)}",
                    "{p.worktree.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| short(&p.worktree))}"
                }
            }
            for (place , at , (tab , name , state , mark , count , preview , focused)) in laid {
                {
                    let is_active = at == shown_at;
                    let shell = match &tab {
                        Tab::Terminal(environment, id) => Some(runs_of(
                            tree_view::terminals(hosts.work(environment)),
                            *id,
                        )),
                        _ => None,
                    };
                    let class = [
                        Some("tab"),
                        is_active.then_some("active"),
                        shell.is_some().then_some("shell"),
                        preview.then_some("preview"),
                        (dropping == Some(Some(DropAt::Tab(at)))).then_some("drop"),
                        (ghost == Some(at)).then_some("ghost"),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ");
                    let glyph_class = state.as_ref().map(glyph);
                    let target = Target::Tab(tab.clone());
                    let closing = Target::Tab(tab.clone());
                    let has_draft = drafted.read().contains(&tab);
                    let number = place + 1;
                    let windows = if count == 1 { "1 window".to_string() } else { format!("{count} windows") };
                    // The preview is no tab of its own yet: unnumbered, after
                    // a rule, and named as what it only shows.
                    if preview {
                        return rsx! {
                            span {
                                key: "{at}",
                                class: "{class}",
                                title: "Preview: what is looked at next takes its place; open it to keep it",
                                onclick: move |_| app.dispatch(Msg::TabShow(at)),
                                onmousedown: move |e: Event<MouseData>| press(app, e, at, focused),
                                onmousemove: move |e: Event<MouseData>| hover(app, e, place, dragging),
                                oncontextmenu: menu_opener(app, target),
                                IconView { icon: Icon::Eye, size: 13 }
                                span { class: "tab-preview-label", "Preview" }
                                if let Some((g, icon, tip)) = glyph_class {
                                    StateGlyph { class: g, icon, size: 13, title: tip }
                                }
                                if let Some(runs) = shell { TerminalMark { runs } }
                                span { class: "tab-preview-name", "{name}" }
                                span {
                                    class: "tab-close",
                                    title: "Close the preview; what it shows runs on",
                                    onclick: move |e: Event<MouseData>| {
                                        e.stop_propagation();
                                        app.dispatch(Msg::TabShow(at));
                                        onclose.call(closing.clone());
                                    },
                                    IconView { icon: Icon::X, size: 12 }
                                }
                            }
                        };
                    }
                    rsx! {
                        span {
                            key: "{at}",
                            class: "{class}",
                            onclick: move |_| app.dispatch(Msg::TabShow(at)),
                            onmousedown: move |e: Event<MouseData>| press(app, e, at, focused),
                            onmousemove: move |e: Event<MouseData>| hover(app, e, place, dragging),
                            oncontextmenu: menu_opener(app, target),
                            span { class: "tab-number", "{number}" }
                            if let Some((g, icon, tip)) = glyph_class {
                                StateGlyph { class: g, icon, size: 13, title: tip }
                            }
                            if let Some(runs) = shell { TerminalMark { runs } }
                            if matches!(tab, Tab::Draft { .. }) { PlaceMark { icon: Icon::MessagePlus, size: 13 } }
                            if let (Tab::Session(environment, id), true) = (&tab, is_active) {
                                SessionName {
                                    environment: environment.clone(), id: id.clone(), name: name.clone(), place: RenamePlace::Tab,
                                }
                            } else { "{name}" }
                            SessionMark { mark }
                            span { class: "tab-count", title: "{windows}", "{count}" }
                            if has_draft {
                                DraftMark {}
                            }
                            span {
                                class: "tab-close",
                                title: "Close the tab; what its windows show runs on",
                                onclick: move |e: Event<MouseData>| {
                                    e.stop_propagation();
                                    app.dispatch(Msg::TabShow(at));
                                    onclose.call(closing.clone());
                                },
                                IconView { icon: Icon::X, size: 12 }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The pointer pressed the tab at `tab`, to drag it, or its focused window
/// `window`.
fn press(app: AppCtx, e: Event<MouseData>, tab: usize, window: WindowId) {
    if e.trigger_button() != Some(MouseButton::Primary) {
        return;
    }
    e.prevent_default();
    let at = e.client_coordinates();
    app.dispatch(Msg::WindowMoveStart {
        window,
        tab: Some(tab),
        x: at.x,
        y: at.y,
    });
}

/// The pointer moved over the tab drawn at `at` among the tabs, into which
/// a window `dragging` would go, or whose place a tab dragged would take.
fn hover(app: AppCtx, e: Event<MouseData>, at: usize, dragging: bool) {
    if !dragging {
        return;
    }
    e.stop_propagation();
    let pos = e.client_coordinates();
    app.dispatch(Msg::WindowMoveOver {
        x: pos.x,
        y: pos.y,
        over: Some(DropAt::Tab(at)),
    });
}
