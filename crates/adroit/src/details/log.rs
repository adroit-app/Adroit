//! The log: the history of the worktree shown, drawn with its graph.

use adroit_core::graph;
use adroit_model::repository::Commit;
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use adroit_core::msg::Msg;

#[component]
pub(super) fn LogRows(commits: Vec<Commit>, selected: Option<usize>) -> Element {
    let app = use_context::<AppCtx>();
    let rows = graph::layout(
        commits
            .iter()
            .map(|c| (c.node.as_str(), c.parents.as_slice())),
    );
    let lanes = rows.iter().map(graph::Row::width).max().unwrap_or(1);
    rsx! {
        for (i , (c , row)) in commits.iter().zip(&rows).enumerate() {
            div { key: "{c.id}",
                class: if selected == Some(i) { "commit selected" } else { "commit" },
                onclick: move |_| app.dispatch(Msg::DetailsOpenRow(i)),
                div {
                    class: "commit-graph",
                    dangerous_inner_html: graph::svg(row, lanes, graph::Marks { current: c.current, empty: c.empty, conflict: c.conflict }),
                }
                div { class: "commit-text",
                span { class: "commit-hash", "{c.short}" }
                span { class: "commit-subject",
                    if c.current {
                        span { class: "commit-mark current", "@" }
                    }
                    if c.conflict {
                        span { class: "commit-mark conflict", "conflict" }
                    }
                    if c.empty {
                        span { class: "commit-mark", "empty" }
                    }
                    for b in c.bookmarks.iter() {
                        span { class: "commit-mark bookmark", "{b}" }
                    }
                    if c.subject.is_empty() {
                        span { class: "commit-none", "(no description)" }
                    } else {
                        "{c.subject}"
                    }
                }
                span { class: "commit-when", "{c.when}" }
                }
            }
        }
    }
}
