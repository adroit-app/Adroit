//! The sheets of the session shown: the overview's head, how it stands and
//! the change it is on, and on each sheet its rows grouped under headings;
//! or the row opened from them.

use std::path::PathBuf;

use adroit_core::msg::Msg;
use adroit_model::{Process, Session};
use dioxus::prelude::*;

use super::message::MessageView;
use super::process::ProcessView;
use super::subagent::SubagentView;
use crate::bridge::AppCtx;
use crate::components::{Fact, Facts, ListRow, Section};
use crate::details::{
    Details, Entry, Group, Listing, Sheet, change_line, entry_listing, link_sections, opened_agent,
    opened_note, opened_process, overview_change, overview_facts, shown_session, stats,
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

#[component]
pub(super) fn Overview(
    /// Where the shown session's paths are told from.
    worktree: Option<PathBuf>,
    /// When the shown session last took input through its console.
    live_input_ms: Option<u64>,
    /// Now, as the facts that tell how long ago are told.
    now_ms: u64,
    back_keys: String,
    on_kill: EventHandler<(Session, Process)>,
    on_menu: EventHandler<LinkMenu>,
) -> Element {
    let app = use_context::<AppCtx>();
    let details: Details = app.details.read().clone();
    let host = app.host.read();
    let work = host.work.as_ref();
    let session = shown_session(&details, work).cloned();
    let head = details.drill.sheet() == Sheet::Overview;
    let facts = session.as_ref().filter(|_| head).map(|s| {
        let stats = stats(&details, work).unwrap_or_default();
        overview_facts(s, &stats, s.last_input_ms.max(live_input_ms), now_ms)
    });
    let change = overview_change(&details, work)
        .filter(|_| head)
        .and_then(|v| change_line(&v));
    let listing = entry_listing(&details, work);
    let selected = listing.as_ref().and_then(|l| details.drill.selected(l));
    let agent = opened_agent(&details, work);
    let note = opened_note(&details, work);
    let process = opened_process(&details, work);
    let entries = match &listing {
        // A row opened stands in for the rows.
        Some(Listing::Entries(entries))
            if details.drill.agent().is_none()
                && details.drill.message().is_none()
                && details.drill.process().is_none() =>
        {
            entries.as_slice()
        }
        _ => &[],
    };
    let paths_from = worktree
        .clone()
        .or_else(|| session.as_ref().map(|s| s.cwd.clone()))
        .unwrap_or_default();
    let sections = link_sections(entries, &paths_from, &crate::paths::home(), now_ms);
    let killable = |at: usize| match &listing {
        Some(Listing::Entries(entries)) => match entries.get(at) {
            Some(Entry::Process(p)) if !p.zombie => session.clone().map(|s| (s, p.clone())),
            _ => None,
        },
        _ => None,
    };
    rsx! {
        if let (Some(s), Some(facts)) = (&session, facts) {
            Section { label: "{s.name}",
                Facts {
                    for fact in facts {
                        Fact { label: "{fact.label}",
                            if let Some(tone) = fact.tone {
                                span { class: "details-facts-value",
                                    span { class: "state-badge {tone}", "{fact.value}" }
                                }
                            } else {
                                span { class: "details-facts-value", title: "{fact.value}", "{fact.value}" }
                            }
                        }
                    }
                }
            }
        }
        if let Some(change) = change {
            div { class: "details-block details-links",
                ListRow {
                    kind: "details-link details-change",
                    title: "{change.subject}",
                    onclick: move |_| app.dispatch(Msg::DetailsSheet(Sheet::Changes)),
                    IconView { icon: Icon::GitCommit, size: 13 }
                    span { class: "commit-mark", "{change.short}" }
                    span { class: "list-row-name", "{change.subject}" }
                    span { class: "details-link-tag", "{change.files}" }
                }
            }
        }
        if let Some(agent) = agent {
            SubagentView {
                agent,
                now_ms: adroit_model::time::now_ms(),
                back_keys: back_keys.clone(),
                on_back: move |_| app.dispatch(Msg::DetailsBack),
            }
        }
        if let Some(note) = note {
            MessageView {
                note,
                now_ms: adroit_model::time::now_ms(),
                back_keys: back_keys.clone(),
                on_back: move |_| app.dispatch(Msg::DetailsBack),
            }
        }
        if let Some(process) = process {
            ProcessView {
                key: "{process.key}",
                process,
                on_back: move |_| app.dispatch(Msg::DetailsBack),
                on_kill: {
                    let session = session.clone();
                    move |process| {
                        if let Some(s) = session.clone() {
                            on_kill.call((s, process));
                        }
                    }
                },
            }
        }
        for section in sections.into_iter() {
            {
            // A fact is a value to read and copy, drawn as the head's facts are.
            let facts = section.group == Group::About;
            let at = section.at;
            rsx! {
            div { key: "{section.group.title()}", class: "details-block details-links",
                ListRow {
                    kind: "details-link-heading",
                    selected: selected == Some(at),
                    onclick: move |_| app.dispatch(Msg::DetailsClick(at)),
                    IconView { icon: if section.folded { Icon::ChevronRight } else { Icon::ChevronDown }, size: 12 }
                    span { class: "details-label", "{section.group.title()}" }
                    if section.folded {
                        span { class: "details-link-tag", "{section.count}" }
                    }
                }
                for r in section.rows.into_iter() {
                    {
                        let (at, file) = (r.at, r.file);
                        let kill = killable(at);
                        let kind = match (facts, r.thumb.is_some()) {
                            (true, _) => "details-link details-fact",
                            (false, true) => "details-link details-image",
                            (false, false) => "details-link",
                        };
                        rsx! {
                            ListRow { key: "{r.target}",
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
                                    span { class: "list-row-name details-facts-value", "{r.tag}" }
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
                if let Some((at, hidden)) = section.more {
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
