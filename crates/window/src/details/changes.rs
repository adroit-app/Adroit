//! What the changes and the log read of the worktree shown: the rows opened
//! as crumbs, the revision's description, and the files a change touches or
//! one file's diff.

use adroit_core::msg::Msg;
use adroit_model::repository::DiffKind;
use dioxus::prelude::*;

use super::log::LogRows;
use crate::bridge::AppCtx;
use crate::components::{Breadcrumbs, ListRow};
use crate::details::{Drill, Listing, View};

fn diff_class(kind: DiffKind) -> &'static str {
    match kind {
        DiffKind::File => "diff-file",
        DiffKind::Hunk => "diff-hunk",
        DiffKind::Added => "diff-add",
        DiffKind::Removed => "diff-del",
        DiffKind::Context => "diff-ctx",
    }
}

/// What a worktree is: none shown, outside git and jj, or a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Worktree {
    None,
    OutsideGit,
    Repository,
}

/// The spot of the changes or the log the panel is at, `view` what was read
/// for it.
#[component]
pub(super) fn Repository(
    drill: Drill,
    view: Option<View>,
    worktree: Worktree,
    back_keys: String,
) -> Element {
    let app = use_context::<AppCtx>();
    let crumbs = drill.crumbs();
    let revision = view.as_ref().and_then(|v| v.revision.clone());
    let listing = view.map(|v| v.listing);
    let selected = listing.as_ref().and_then(|l| drill.selected(l));
    rsx! {
        div { class: "list-rows details-git",
            if !crumbs.is_empty() {
                Breadcrumbs {
                    title: "Back to the list ({back_keys})",
                    crumbs,
                    onback: move |_| app.dispatch(Msg::DetailsBack),
                }
            }
            if let Some(revision) = &revision {
                div { class: "revision-description",
                    div { class: "revision-heading",
                        span { class: "details-label", "Description" }
                        span { class: "commit-hash", title: "{revision.id}", "{revision.short}" }
                    }
                    if revision.description.is_empty() {
                        div { class: "commit-none", "(no description)" }
                    } else {
                        pre { "{revision.description}" }
                    }
                }
            }
            match (worktree, listing) {
                (Worktree::None, _) => rsx! { ListRow { kind: "dim", "Open a worktree to see it here" } },
                (Worktree::OutsideGit, _) => rsx! { ListRow { kind: "dim", "Not a git or jj repository" } },
                (_, None) => rsx! { ListRow { kind: "dim", "Reading…" } },
                (_, Some(Listing::Error(e))) => rsx! { div { class: "git-error", "{e}" } },
                (_, Some(Listing::Files(files))) if files.is_empty() => rsx! { ListRow { kind: "dim", "No changes" } },
                (_, Some(Listing::Files(files))) => rsx! {
                    for (i , f) in files.iter().enumerate() {
                        ListRow { key: "{f.path}",
                            kind: "mono",
                            selected: selected == Some(i),
                            onclick: move |_| app.dispatch(Msg::DetailsOpenRow(i)),
                            span { class: "details-file-status", "{f.status}" }
                            span { class: "list-row-name", "{f.shown()}" }
                        }
                    }
                },
                (_, Some(Listing::Log(commits))) => rsx! { LogRows { commits, selected } },
                (_, Some(Listing::Diff(lines))) if lines.is_empty() => rsx! { ListRow { kind: "dim", "No changes" } },
                (_, Some(Listing::Diff(lines))) => rsx! {
                    div { class: "diff",
                        for (i , d) in lines.iter().enumerate() {
                            div { key: "{i}", class: "{diff_class(d.kind)}", "{d.text}" }
                        }
                    }
                },
                // The repository's sheets never read the overview's links.
                (_, Some(Listing::Links(_))) => rsx! {},
            }
        }
    }
}
