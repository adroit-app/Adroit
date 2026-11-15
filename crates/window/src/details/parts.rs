//! Each component of the details panel, drawn where the tab or the foot
//! places it.

use std::path::PathBuf;

use adroit_core::msg::Msg;
use adroit_model::{Agent, Process, Session};
use dioxus::prelude::*;

use super::changes::Worktree;
use super::log::LogRows;
use super::overview::{ChangeLine, GroupedRows, LinkMenu, SessionHead, selected_row};
use super::timeline::{ProgressCard, TimelineRows};
use super::usage::{AgentUsage, MachineUsage};
use crate::bridge::AppCtx;
use crate::components::ListRow;
use crate::connection::{EnvironmentAgents, EnvironmentCard};
use crate::details::{
    Component, Entry, Listing, Part, branch_pull, machine_gauges, shown_session, usage,
    working_copy,
};
use adroit_core::details::GaugeKind;
use adroit_core::tree::nav::Item;

/// The components of a run of `runs`: the machine's meters or the agent's
/// gauges drawn together, or else the one component.
#[component]
pub(super) fn RunView(
    run: Vec<Part>,
    worktree: Option<PathBuf>,
    kind: Worktree,
    live_input_ms: Option<u64>,
    agent: Agent,
    now_ms: u64,
    on_kill: EventHandler<(Session, Process)>,
    on_menu: EventHandler<LinkMenu>,
) -> Element {
    let app = use_context::<AppCtx>();
    let components: Vec<Component> = run.iter().map(|p| p.placed.component).collect();
    if components.iter().all(|c| c.meter().is_some()) {
        let system = components
            .iter()
            .filter_map(|c| c.meter())
            .flat_map(|meter| machine_gauges(&app, &app.shown_environment(), &meter))
            .collect::<Vec<_>>();
        return rsx! { MachineUsage { system } };
    }
    let kinds: Vec<GaugeKind> = components.iter().filter_map(|c| c.gauge()).collect();
    if kinds.len() == components.len() {
        let details = app.details.read().clone();
        let hosts = app.hosts.read();
        let work = hosts.work(&app.shown_environment());
        let all = usage(&details, work, now_ms);
        let gauges = kinds
            .iter()
            .flat_map(|k| all.iter().filter(move |g| g.kind == *k).cloned())
            .collect::<Vec<_>>();
        let usage_url = work
            .and_then(|w| w.agents.of(agent))
            .and_then(|facts| facts.usage_url.clone());
        return rsx! { AgentUsage { gauges, agent, usage_url } };
    }
    rsx! {
        for part in run {
            PartView {
                key: "{part.placed.component.name()}-{part.at}",
                part,
                worktree: worktree.clone(),
                kind,
                live_input_ms,
                agent,
                now_ms,
                on_kill,
                on_menu,
            }
        }
    }
}

#[component]
pub(super) fn PartView(
    part: Part,
    /// The worktree the details describe, while a window shows something.
    worktree: Option<PathBuf>,
    kind: Worktree,
    live_input_ms: Option<u64>,
    agent: Agent,
    now_ms: u64,
    on_kill: EventHandler<(Session, Process)>,
    on_menu: EventHandler<LinkMenu>,
) -> Element {
    let app = use_context::<AppCtx>();
    let selected = selected_row(&app);
    let base = part.at;
    let rows = part.rows.clone();
    // The environment a row of the tree is of, or else the one shown.
    let selected_item = app.details.read().shown.selected.clone();
    let environment = match &selected_item {
        Some(Item::Environment(e) | Item::GitHubItem(e, ..)) => e.clone(),
        _ => app.shown_environment(),
    };
    match part.placed.component {
        Component::Session => rsx! { SessionHead { live_input_ms, now_ms } },
        Component::Change => rsx! { ChangeLine {} },
        Component::Progress => match rows.first() {
            Some(Entry::Progress(progress)) => rsx! {
                ProgressCard { progress: progress.clone(), at: base, selected: selected == Some(base), now_ms }
            },
            _ => rsx! {},
        },
        Component::Timeline => {
            let has_session = {
                let details = app.details.read();
                let hosts = app.hosts.read();
                shown_session(&details, hosts.work(&app.shown_environment())).is_some()
            };
            let rows: Vec<(usize, Entry)> = rows
                .into_iter()
                .enumerate()
                .map(|(i, e)| (base + i, e))
                .collect();
            rsx! {
                if !rows.is_empty() {
                    TimelineRows { rows, selected, now_ms }
                } else if has_session {
                    div { class: "details-block timeline-empty",
                        "No timeline yet. A session's agent records the plan and progress of its work here as it goes."
                    }
                }
            }
        }
        Component::Changes => rsx! { Changed { rows, base, selected, worktree, kind } },
        Component::Log => {
            let commits = rows
                .iter()
                .filter_map(|e| match e {
                    Entry::Commit(c) => Some(c.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let first = base
                + rows
                    .iter()
                    .position(|e| matches!(e, Entry::Commit(_)))
                    .unwrap_or(0);
            let heading = rows
                .first()
                .filter(|e| matches!(e, Entry::Heading { .. }))
                .cloned();
            rsx! {
                if let Some(heading) = heading {
                    GroupedRows { rows: vec![heading], base, paths_from: PathBuf::new(), now_ms, on_kill, on_menu }
                }
                match (kind, commits.is_empty()) {
                    (Worktree::None, _) => rsx! { ListRow { kind: "dim", "Open a worktree to see it here" } },
                    (Worktree::OutsideGit, _) => rsx! { ListRow { kind: "dim", "Not a git or jj repository" } },
                    (_, true) => rsx! {},
                    (_, false) => rsx! {
                        div { class: "list-rows details-git",
                            LogRows { commits, selected: selected.and_then(|s| s.checked_sub(first)), base: Some(first) }
                        }
                    },
                }
            }
        }
        Component::Environment => rsx! { EnvironmentCard { environment } },
        Component::Profiles => rsx! { EnvironmentAgents { environment } },
        Component::Github => match selected_item {
            Some(Item::GitHubItem(environment, root, listing, number)) => rsx! {
                super::GitHubCard { environment, root, listing, number }
            },
            _ => {
                let pull = {
                    let details = app.details.read();
                    let hosts = app.hosts.read();
                    branch_pull(&details, hosts.work(&environment))
                };
                match pull {
                    Some((root, number)) => rsx! {
                        super::GitHubCard { environment, root, listing: adroit_model::github::Listing::Pulls, number }
                    },
                    None => rsx! {
                        div { class: "details-block timeline-empty", "No pull request is open for this branch." }
                    },
                }
            }
        },
        _ => {
            let paths_from = worktree
                .clone()
                .or_else(|| {
                    let details = app.details.read();
                    let hosts = app.hosts.read();
                    shown_session(&details, hosts.work(&app.shown_environment()))
                        .map(|s| s.cwd.clone())
                })
                .unwrap_or_default();
            rsx! { GroupedRows { rows, base, paths_from, now_ms, on_kill, on_menu } }
        }
    }
}

/// The current change's description and the files it changes.
#[component]
fn Changed(
    rows: Vec<Entry>,
    base: usize,
    selected: Option<usize>,
    worktree: Option<PathBuf>,
    kind: Worktree,
) -> Element {
    let app = use_context::<AppCtx>();
    let view = {
        let details = app.details.read();
        let hosts = app.hosts.read();
        working_copy(&details, hosts.work(&app.shown_environment()))
            .filter(|v| worktree.as_ref() == Some(&v.worktree))
    };
    let revision = view.as_ref().and_then(|v| v.revision.clone());
    let listing = view.map(|v| v.listing);
    rsx! {
        div { class: "list-rows details-git",
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
            match (kind, listing) {
                (Worktree::None, _) => rsx! { ListRow { kind: "dim", "Open a worktree to see it here" } },
                (Worktree::OutsideGit, _) => rsx! { ListRow { kind: "dim", "Not a git or jj repository" } },
                (_, None) => rsx! { ListRow { kind: "dim", "Reading…" } },
                (_, Some(Listing::Error(e))) => rsx! { div { class: "git-error", "{e}" } },
                (_, Some(Listing::Files(files))) if files.is_empty() => rsx! { ListRow { kind: "dim", "No changes" } },
                _ => rsx! {
                    for (i , entry) in rows.into_iter().enumerate() {
                        {
                            let at = base + i;
                            match entry {
                                Entry::Changed(f) => rsx! {
                                    ListRow { key: "{f.path}",
                                        kind: "mono",
                                        selected: selected == Some(at),
                                        onclick: move |_| app.dispatch(Msg::DetailsClick(at)),
                                        span { class: "details-file-status", "{f.status}" }
                                        span { class: "list-row-name", "{f.shown()}" }
                                    }
                                },
                                Entry::Heading { folded, count, .. } => rsx! {
                                    ListRow { key: "heading-{at}",
                                        kind: "details-link-heading",
                                        selected: selected == Some(at),
                                        onclick: move |_| app.dispatch(Msg::DetailsClick(at)),
                                        crate::icons::IconView { icon: if folded { crate::icons::Icon::ChevronRight } else { crate::icons::Icon::ChevronDown }, size: 12 }
                                        span { class: "details-label", "Changes" }
                                        if folded {
                                            span { class: "details-link-tag", "{count}" }
                                        }
                                    }
                                },
                                Entry::More { hidden, .. } => rsx! {
                                    ListRow { key: "more-{at}",
                                        kind: "details-link details-more",
                                        selected: selected == Some(at),
                                        onclick: move |_| app.dispatch(Msg::DetailsClick(at)),
                                        crate::icons::IconView { icon: crate::icons::Icon::Dots, size: 13 }
                                        span { class: "list-row-name", "Show all" }
                                        span { class: "details-link-tag", "{hidden} more" }
                                    }
                                },
                                _ => rsx! {},
                            }
                        }
                    }
                },
            }
        }
    }
}
