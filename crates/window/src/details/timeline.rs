//! The timeline as the panel draws it: the trees with their tasks, and a
//! task opened with its history.

use adroit_core::details::{Bar, Entry, TaskDetail, TaskLine, TreeCard, status_label, status_tone};
use adroit_core::msg::Msg;
use adroit_model::time::ago;
use adroit_model::timeline::{Status, percent};
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::components::{Breadcrumbs, Fact, Facts, ListRow};

/// The mark a status is listed with.
fn mark(status: Status) -> &'static str {
    match status {
        Status::Pending => "○",
        Status::InProgress => "◐",
        Status::Completed => "●",
        Status::Blocked => "✕",
    }
}

/// A bar split by the status of the work, drawn in each status's tone.
#[component]
fn StackBar(bar: Bar, #[props(default)] kind: String) -> Element {
    rsx! {
        div { class: "timeline-bar {kind}",
            for (status , width) in bar.parts() {
                if width > 0.0 {
                    div { class: "timeline-bar-part {status_tone(status)}", style: "width:{width}%" }
                }
            }
        }
    }
}

#[component]
fn Chip(status: Status) -> Element {
    rsx! {
        span { class: "state-badge {status_tone(status)}", "{status_label(status)}" }
    }
}

/// The row standing for the trees not shown, which shows them all.
#[component]
fn MoreRow(hidden: usize, at: usize, selected: bool) -> Element {
    let app = use_context::<AppCtx>();
    rsx! {
        ListRow {
            kind: "details-link details-more",
            selected,
            onclick: move |_| app.dispatch(Msg::DetailsClick(at)),
            crate::icons::IconView { icon: crate::icons::Icon::Dots, size: 13 }
            span { class: "list-row-name", "Show all" }
            span { class: "details-link-tag", "{hidden} more" }
        }
    }
}

/// The timeline sheet's rows, each with the row it is in the entries.
#[component]
pub(super) fn TimelineRows(
    rows: Vec<(usize, Entry)>,
    selected: Option<usize>,
    now_ms: u64,
) -> Element {
    rsx! {
        div { class: "details-block details-links timeline",
            for (at , entry) in rows.into_iter() {
                match entry {
                    Entry::Tree(card) => rsx! {
                        TreeRow { key: "{card.key}", card, at, selected: selected == Some(at), now_ms }
                    },
                    Entry::Task(line) => rsx! {
                        TaskRow { key: "{line.id}", line, at, selected: selected == Some(at) }
                    },
                    Entry::More { hidden, .. } => rsx! {
                        MoreRow { key: "more-{at}", hidden, at, selected: selected == Some(at) }
                    },
                    _ => rsx! {},
                }
            }
        }
    }
}

#[component]
fn TreeRow(card: TreeCard, at: usize, selected: bool, now_ms: u64) -> Element {
    let app = use_context::<AppCtx>();
    rsx! {
        ListRow {
            kind: "timeline-tree",
            selected,
            title: if card.folded {
                "Show its tasks"
            } else {
                "Hide its tasks"
            },
            onclick: move |_| app.dispatch(Msg::DetailsClick(at)),
            div { class: "timeline-tree-head",
                crate::icons::IconView {
                    icon: if card.folded { crate::icons::Icon::ChevronRight } else { crate::icons::Icon::ChevronDown },
                    size: 12,
                }
                span { class: "list-row-name timeline-tree-title", "{card.title}" }
                Chip { status: card.status }
            }
            div { class: "timeline-tree-bar",
                StackBar { bar: card.bar }
                span { class: "timeline-share", "{card.bar.percent()}" }
            }
            span { class: "details-link-tag timeline-tree-tag", "{card.tag(now_ms)}" }
        }
    }
}

#[component]
fn TaskRow(line: TaskLine, at: usize, selected: bool) -> Element {
    let app = use_context::<AppCtx>();
    let indent = 18 + 14 * line.depth;
    rsx! {
        ListRow {
            kind: "details-link timeline-task",
            selected,
            title: "{line.title}",
            style: "padding-left:{indent}px",
            onclick: move |_| app.dispatch(Msg::DetailsClick(at)),
            span { class: "timeline-mark {status_tone(line.status)}", title: "{status_label(line.status)}", "{mark(line.status)}" }
            span { class: "list-row-name", "{line.title}" }
            StackBar { bar: line.bar, kind: "small" }
            span { class: "timeline-share", "{line.bar.percent()}" }
        }
    }
}

/// A task opened: what it is for, what came of it, and its history, told
/// at `now_ms`; `on_back` goes back to its tree, as `back_keys` do.
#[component]
pub(super) fn TaskView(
    detail: TaskDetail,
    now_ms: u64,
    back_keys: String,
    on_back: EventHandler<()>,
) -> Element {
    let task = detail.task;
    rsx! {
        Breadcrumbs {
            title: "Back to the timeline ({back_keys})",
            crumbs: vec![detail.tree.clone(), task.title.clone()],
            onback: move |_| on_back.call(()),
        }
        div { class: "details-block",
            div { class: "timeline-task-head",
                span { class: "details-label timeline-task-title", "{task.title}" }
                Chip { status: task.status }
            }
            if let Some(description) = &task.description {
                div { class: "message-text", "{description}" }
            }
            if let Some(note) = &task.note {
                div { class: "timeline-note", "{note}" }
            }
            div { class: "timeline-tree-bar",
                StackBar { bar: detail.bar }
                span { class: "timeline-share", "{detail.bar.percent()}" }
            }
            Facts {
                Fact { label: "Completed", span { class: "details-facts-value", "{detail.bar.percent()}" } }
                Fact { label: "Weight", span { class: "details-facts-value", "{task.weight}" } }
                Fact { label: "Leaf tasks", span { class: "details-facts-value", "{detail.leaves_done} / {detail.leaves}" } }
            }
        }
        div { class: "details-block",
            span { class: "details-label", "History" }
            for (n , r) in detail.history.into_iter().enumerate() {
                div { key: "{n}", class: "timeline-revision",
                    div { class: "timeline-revision-head",
                        span { class: "timeline-mark {status_tone(r.to)}", "{mark(r.to)}" }
                        span { class: "timeline-revision-status", "{status_label(r.to)}" }
                        span { class: "details-link-tag", "{ago(now_ms.saturating_sub(r.at_ms))}" }
                    }
                    div { class: "timeline-revision-share",
                        if r.from.is_none() {
                            "Added · weight {r.weight}"
                        } else {
                            "{percent(r.before)} → {percent(r.after)} · weight {r.weight}"
                        }
                    }
                    if let Some(description) = &r.description {
                        div { class: "timeline-revision-text", "{description}" }
                    }
                    if let Some(note) = &r.note {
                        div { class: "timeline-revision-text timeline-note", "{note}" }
                    }
                }
            }
        }
    }
}
