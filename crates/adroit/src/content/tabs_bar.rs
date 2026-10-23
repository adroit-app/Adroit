//! The tabs bar: the worktree in the middle by name, and a tab for each tab
//! of the content, named after what its focused window shows.

use std::collections::HashSet;

use adroit_core::completion::Completions;
use adroit_core::msg::Msg;
use adroit_core::tree as tree_view;
use adroit_model::AgentPermissions;
use dioxus::prelude::*;

use super::short;
use crate::bridge::AppCtx;
use crate::components::{SessionMark, TerminalMark, glyph, runs_of, session_mark};
use crate::content::{Tab, thing_name};
use crate::icons::{Icon, IconView};
use crate::nav;
use crate::rename::{Place as RenamePlace, SessionName};
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
        let host = app.host.read();
        let work = host.work.as_ref();
        let t = tree_view::listed(work);
        let terms = tree_view::terminals(work);
        let view = app.content.read();
        view.windows
            .tabs()
            .iter()
            .map(|tab| {
                let shows = tab.focused().shows.clone();
                let (state, mark) = match &shows {
                    Tab::Session(id) => nav::find(t, id).map_or((None, None), |(_, s)| {
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
                    Tab::Terminal(_) | Tab::Viewer(_) => (None, None),
                };
                let name = thing_name(t, terms, &shows);
                (shows, name, state, mark, tab.windows().len())
            })
            .collect::<Vec<_>>()
    });
    let current = app.content.read().described().cloned();
    let shown_at = app.content.read().windows.current_index();
    let host = app.host.read();
    let terminals = tree_view::terminals(host.work.as_ref());
    rsx! {
        div { class: "tabs",
            // The worktree by name only: the header above holds its whole
            // path, and here it would crowd the tabs out.
            if let Some(p) = &current {
                span { class: "tabs-where", title: "{short(&p.worktree)}",
                    "{p.worktree.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| short(&p.worktree))}"
                }
            }
            for (at , (tab , name , state , mark , count)) in tabs().into_iter().enumerate() {
                {
                    let is_active = at == shown_at;
                    let shell = match &tab {
                        Tab::Terminal(id) => Some(runs_of(terminals, *id)),
                        _ => None,
                    };
                    let class = match (is_active, shell.is_some()) {
                        (true, true) => "tab active shell",
                        (true, false) => "tab active",
                        (false, true) => "tab shell",
                        (false, false) => "tab",
                    };
                    let glyph_class = state.as_ref().map(glyph);
                    let target = Target::Tab(tab.clone());
                    let closing = Target::Tab(tab.clone());
                    let has_draft = drafted.read().contains(&tab);
                    let number = at + 1;
                    let windows = if count == 1 { "1 window".to_string() } else { format!("{count} windows") };
                    rsx! {
                        span {
                            key: "{at}",
                            class: "{class}",
                            onclick: move |_| app.dispatch(Msg::TabShow(at)),
                            oncontextmenu: move |e: Event<MouseData>| {
                                e.prevent_default();
                                let at = e.client_coordinates();
                                app.dispatch(Msg::MenuOpen { x: at.x, y: at.y, target: target.clone() });
                            },
                            span { class: "tab-number", "{number}" }
                            if let Some((g, icon, tip)) = glyph_class {
                                span { class: "{g}", title: "{tip}", IconView { icon, size: 13 } }
                            }
                            if let Some(runs) = shell { TerminalMark { runs } }
                            if matches!(tab, Tab::Draft { .. }) { span { class: "place-mark", IconView { icon: Icon::MessagePlus, size: 13 } } }
                            if let (Tab::Session(id), true) = (&tab, is_active) {
                                SessionName {
                                    id: id.clone(), name: name.clone(), place: RenamePlace::Tab,
                                }
                            } else { "{name}" }
                            SessionMark { mark }
                            span { class: "tab-count", title: "{windows}", "{count}" }
                            if has_draft {
                                span { class: "draft-mark", title: "Something is written for it in the prompt", "✎" }
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
