//! The tabs bar: the worktree in the middle by name, and a tab for each tab
//! of the content, named after what its focused window shows. The tabs scroll
//! beside the name, by buttons too where they do not fit.

use std::collections::HashSet;

use adroit_core::completion::Completions;
use adroit_core::msg::Msg;
use adroit_core::tab_scroll::TabScroll;
use adroit_core::tree as tree_view;
use adroit_model::AgentPermissions;
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;

use super::short;
use crate::bridge::AppCtx;
use crate::components::{
    DraftMark, IconButton, PlaceMark, SessionMark, StateGlyph, TerminalMark, glyph, runs_of,
    session_mark,
};
use crate::content::{DropAt, Tab, WindowId, thing_code, thing_name};
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
                    Tab::Terminal(_, _)
                    | Tab::Viewer(_, _)
                    | Tab::Drawn { .. }
                    | Tab::GitHub { .. } => (None, None),
                };
                let name = tree_view::github_title(work, &shows)
                    .unwrap_or_else(|| thing_name(t, terms, &shows));
                let code = thing_code(t, &shows);
                (
                    shows,
                    name,
                    code,
                    state,
                    mark,
                    tab.windows().len(),
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
    let mut order: Vec<usize> = (0..rows.len()).collect();
    let ghost = match (dropping, from) {
        (Some(Some(DropAt::Place(to))), Some(from)) if from < rows.len() && to < rows.len() => {
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
    let tabs: Vec<Element> = laid
        .into_iter()
        .map(|(place, at, (tab, name, code, state, mark, count, focused))| {
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
                    if let Some(code) = code {
                        span { class: "tab-code", title: "The code it keeps", "{code}" }
                    }
                    SessionMark { mark }
                    span { class: "tab-count", title: "{windows}", "{count}" }
                    if has_draft {
                        DraftMark {}
                    }
                    IconButton {
                        icon: Icon::X,
                        place: "tab-close",
                        title: "Close the tab; what its windows show runs on",
                        onclick: move |e: Event<MouseData>| {
                            e.stop_propagation();
                            app.dispatch(Msg::TabShow(at));
                            onclose.call(closing.clone());
                        },
                    }
                }
            }
        })
        .collect();
    let mut scroll = use_signal(TabScroll::default);
    let overflows = use_memo(move || scroll.read().overflows());
    // The buttons narrow the view as they come, which may leave the tab
    // shown partly outside it.
    use_effect(move || {
        if overflows() {
            show_active_tab();
        }
    });
    rsx! {
        div { class: "tabs",
            // The worktree by name only: the header above holds its whole
            // path, and here it would crowd the tabs out.
            if let Some(p) = &current {
                span { class: "tabs-where", title: "{short(&p.worktree)}",
                    IconView { icon: Icon::Folder, size: 13 }
                    span { class: "tabs-where-name",
                        "{p.worktree.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| short(&p.worktree))}"
                    }
                }
            }
            div {
                class: "tabs-area",
                onresize: move |e: Event<ResizeData>| {
                    if let Some(width) = resized(&e) {
                        scroll.write().room = width;
                    }
                },
                if overflows() {
                    IconButton {
                        icon: Icon::ChevronLeft,
                        place: "tabs-step",
                        size: 14,
                        title: "Scroll the tabs left",
                        disabled: !scroll.read().more_left(),
                        onclick: move |_| scroll_tabs(-scroll.peek().press()),
                    }
                }
                div {
                    class: "tabs-scroll",
                    onresize: move |e: Event<ResizeData>| {
                        if let Some(width) = resized(&e) {
                            scroll.write().view = width;
                        }
                    },
                    onscroll: move |e: Event<ScrollData>| scroll.write().left = e.scroll_left(),
                    div {
                        class: "tabs-row",
                        onresize: move |e: Event<ResizeData>| {
                            if let Some(width) = resized(&e) {
                                scroll.write().length = width;
                            }
                        },
                        for tab in tabs {
                            {tab}
                        }
                    }
                }
                if overflows() {
                    IconButton {
                        icon: Icon::ChevronRight,
                        place: "tabs-step",
                        size: 14,
                        title: "Scroll the tabs right",
                        disabled: !scroll.read().more_right(),
                        onclick: move |_| scroll_tabs(scroll.peek().press()),
                    }
                }
            }
        }
    }
}

/// How wide the element resized is. The event is kept from the elements
/// around it, which are told of their own.
fn resized(e: &Event<ResizeData>) -> Option<f64> {
    e.stop_propagation();
    Some(e.data().get_border_box_size().ok()?.width)
}

/// Scroll the tabs sideways by `by` pixels.
fn scroll_tabs(by: f64) {
    document::eval(&format!(
        "document.querySelector('.tabs-scroll')?.scrollBy({{ left: {by}, behavior: 'smooth' }});"
    ));
}

/// Bring the tab shown into view along the line of tabs, after it is drawn.
pub(crate) fn show_active_tab() {
    document::eval(
        "requestAnimationFrame(() => document.querySelectorAll('.tabs .tab.active')
           .forEach((t) => t.scrollIntoView({ block: 'nearest', inline: 'nearest' })));",
    );
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
