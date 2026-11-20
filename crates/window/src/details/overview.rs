//! What the session shown tells of itself: its head and the change it is
//! on, the rows a component lists grouped under their headings, and the row
//! opened from them.

use std::path::Path;

use adroit_core::msg::Msg;
use adroit_model::{Process, Session};
use dioxus::prelude::*;

use super::message::{MessageView, PromptView};
use super::process::ProcessView;
use super::subagent::SubagentView;
use super::timeline::TaskView;
use crate::bridge::AppCtx;
use crate::components::{CopyValue, Fact, Facts, ListRow, Section};
use crate::details::{
    Component, Entry, Group, change_line, entry_listing, link_sections, opened_agent, opened_note,
    opened_process, opened_prompt, opened_task, overview_facts, shown_session, stats, working_copy,
};
use crate::icons::{Icon, IconView};

/// A row of the overview a menu is asked for: where, which row, and
/// whether it is a file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LinkMenu {
    pub x: f64,
    pub y: f64,
    pub at: usize,
    pub file: bool,
}

/// The session's name, state, model and times.
#[component]
pub(super) fn SessionHead(live_input_ms: Option<u64>, now_ms: u64) -> Element {
    let app = use_context::<AppCtx>();
    let details = app.details.read().clone();
    let environment = app.shown_environment();
    let hosts = app.hosts.read();
    let work = hosts.work(&environment);
    let Some(s) = shown_session(&details, work).cloned() else {
        return rsx! {};
    };
    let stats = stats(&details, work).unwrap_or_default();
    let facts = overview_facts(&s, &stats, s.last_input_ms.max(live_input_ms), now_ms);
    rsx! {
        Section { label: "{s.name}",
            Facts {
                for fact in facts {
                    Fact { label: "{fact.label}",
                        if let Some(tone) = fact.tone {
                            span { class: "details-facts-value",
                                span { class: "state-badge {tone}", "{fact.value}" }
                                if let Some(note) = &fact.note {
                                    span { class: "details-facts-note", "{note}" }
                                }
                                CopyValue { text: fact.value.clone() }
                            }
                        } else {
                            span { class: "details-facts-value", title: "{fact.value}",
                                "{fact.value}"
                                CopyValue { text: fact.value.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The change the worktree is on, which shows the tab of its changes.
#[component]
pub(super) fn ChangeLine() -> Element {
    let app = use_context::<AppCtx>();
    let details = app.details.read().clone();
    let environment = app.shown_environment();
    let hosts = app.hosts.read();
    let Some(change) =
        working_copy(&details, hosts.work(&environment)).and_then(|v| change_line(&v))
    else {
        return rsx! {};
    };
    rsx! {
        div { class: "details-block details-links",
            ListRow {
                kind: "details-link details-change",
                title: "{change.subject}",
                onclick: move |_| app.dispatch(Msg::DetailsShowComponent(Component::Changes)),
                IconView { icon: Icon::GitCommit, size: 13 }
                span { class: "commit-mark", "{change.short}" }
                span { class: "list-row-name", "{change.subject}" }
                span { class: "details-link-tag", "{change.files}" }
            }
        }
    }
}

/// The row selected of the panel's list, if any.
pub(super) fn selected_row(app: &AppCtx) -> Option<usize> {
    let details = app.details.read();
    let environment = app.shown_environment();
    let hosts = app.hosts.read();
    let listing = entry_listing(&details, hosts.work(&environment))?;
    details.drill.selected(&listing)
}

/// The subagent, message, process or task opened from a component's rows.
#[component]
pub(super) fn OpenedRow(back_keys: String, on_kill: EventHandler<(Session, Process)>) -> Element {
    let app = use_context::<AppCtx>();
    let details = app.details.read().clone();
    let environment = app.shown_environment();
    let hosts = app.hosts.read();
    let work = hosts.work(&environment);
    let session = shown_session(&details, work).cloned();
    let now_ms = adroit_model::time::now_ms();
    rsx! {
        if let Some(detail) = opened_task(&details, work) {
            TaskView {
                detail,
                now_ms,
                back_keys: back_keys.clone(),
                on_back: move |_| app.dispatch(Msg::DetailsBack),
            }
        }
        if let Some(agent) = opened_agent(&details, work) {
            SubagentView {
                agent,
                now_ms,
                back_keys: back_keys.clone(),
                on_back: move |_| app.dispatch(Msg::DetailsBack),
            }
        }
        if let Some(prompt) = opened_prompt(&details, work) {
            PromptView {
                prompt,
                now_ms,
                back_keys: back_keys.clone(),
                on_back: move |_| app.dispatch(Msg::DetailsBack),
            }
        }
        if let Some(note) = opened_note(&details, work) {
            MessageView {
                note,
                now_ms,
                back_keys: back_keys.clone(),
                on_back: move |_| app.dispatch(Msg::DetailsBack),
            }
        }
        if let Some(process) = opened_process(&details, work) {
            ProcessView {
                key: "{process.key}",
                process,
                on_back: move |_| app.dispatch(Msg::DetailsBack),
                on_kill: move |process| {
                    if let Some(s) = session.clone() {
                        on_kill.call((s, process));
                    }
                },
            }
        }
    }
}

/// `rows`, the first at `base` of the panel's list, grouped under their
/// headings; paths are told from `paths_from`.
#[component]
pub(super) fn GroupedRows(
    rows: Vec<Entry>,
    /// What all its rows come to, which the line that folds them says.
    #[props(default)]
    said: Option<String>,
    base: usize,
    paths_from: std::path::PathBuf,
    now_ms: u64,
    on_kill: EventHandler<(Session, Process)>,
    on_menu: EventHandler<LinkMenu>,
) -> Element {
    let app = use_context::<AppCtx>();
    let selected = selected_row(&app);
    let session = {
        let details = app.details.read();
        let environment = app.shown_environment();
        let hosts = app.hosts.read();
        shown_session(&details, hosts.work(&environment)).cloned()
    };
    let sections = link_sections(&rows, Path::new(&paths_from), &crate::paths::home(), now_ms);
    let killable = |at: usize| match rows.get(at) {
        Some(Entry::Process(p)) if !p.zombie => session.clone().map(|s| (s, p.clone())),
        _ => None,
    };
    // A component alone in its tab lists its rows without a heading; they
    // are drawn as one section all the same.
    let headless = !matches!(rows.first(), Some(Entry::Heading { .. }));
    rsx! {
        for section in sections.into_iter() {
            {
            // A fact is a value to read and copy, drawn as the head's facts are.
            let facts = section.group == Group::About;
            let at = base + section.at;
            rsx! {
            div { key: "{section.group.title()}", class: "details-block details-links",
                if !headless {
                    ListRow {
                        kind: "details-link-heading",
                        selected: selected == Some(at),
                        onclick: move |_| app.dispatch(Msg::DetailsClick(at)),
                        IconView { icon: if section.folded { Icon::ChevronRight } else { Icon::ChevronDown }, size: 12 }
                        match said.clone().filter(|s| !s.is_empty()) {
                            Some(said) => rsx! { span { class: "list-row-name details-summary", "{said}" } },
                            None => rsx! {
                                span { class: "details-label", "{section.group.title()}" }
                                if section.folded {
                                    span { class: "details-link-tag", "{section.count}" }
                                }
                            },
                        }
                    }
                }
                for r in section.rows.into_iter() {
                    {
                        let (local, file) = (r.at, r.file);
                        let at = base + local;
                        let kill = killable(local);
                        let kind = match (facts, r.thumb.is_some()) {
                            (true, _) => "details-link details-fact",
                            (false, true) => "details-link details-image",
                            (false, false) => "details-link",
                        };
                        rsx! {
                            ListRow { key: "{r.key}",
                                kind,
                                selected: selected == Some(at),
                                title: "{r.target}",
                                onclick: move |_| app.dispatch(Msg::DetailsClick(at)),
                                oncontextmenu: move |e: Event<MouseData>| {
                                    e.prevent_default();
                                    if !facts {
                                        let p = e.client_coordinates();
                                        on_menu.call(LinkMenu { x: p.x, y: p.y, at, file });
                                    }
                                },
                                if facts {
                                    span { class: "details-facts-key", "{r.label}" }
                                    span { class: "list-row-name details-facts-value",
                                        "{r.tag}"
                                        CopyValue { text: r.tag.clone() }
                                    }
                                } else if let Some(thumb) = &r.thumb {
                                    img { class: "details-thumb", src: "{thumb}", alt: "" }
                                } else if r.tone.is_empty() {
                                    IconView { icon: r.icon, size: 13 }
                                } else {
                                    span { class: "{r.tone}", IconView { icon: r.icon, size: 13 } }
                                }
                                if !facts {
                                    span { class: "list-row-name", "{r.label}" }
                                }
                                if let Some(b) = &r.badge {
                                    span { class: "commit-mark details-link-badge", "{b}" }
                                }
                                if !facts && !r.tag.is_empty() {
                                    span { class: "details-link-tag", "{r.tag}" }
                                }
                                if let Some(kill) = kill {
                                    button {
                                        class: "process-kill",
                                        r#type: "button",
                                        title: "Kill process…",
                                        aria_label: "Kill {r.label}",
                                        onmousedown: move |e: Event<MouseData>| { e.stop_propagation(); e.prevent_default(); },
                                        onclick: move |e: Event<MouseData>| {
                                            e.stop_propagation();
                                            on_kill.call(kill.clone());
                                        },
                                        IconView { icon: Icon::X, size: 12 }
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some((more, hidden)) = section.more {
                    {
                        let at = base + more;
                        rsx! {
                            ListRow { key: "more-{at}",
                                kind: "details-link details-more",
                                selected: selected == Some(at),
                                onclick: move |_| app.dispatch(Msg::DetailsClick(at)),
                                IconView { icon: Icon::Dots, size: 13 }
                                span { class: "list-row-name", "Show all" }
                                span { class: "details-link-tag", "{hidden} more" }
                            }
                        }
                    }
                }
            }
            }
            }
        }
    }
}
