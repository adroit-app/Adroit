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
use crate::connection::{
    EnvironmentAgents, EnvironmentButton, EnvironmentCard, EnvironmentPress, EnvironmentWaiting,
};
use crate::details::{
    Component, Entry, Listing, Part, branch_pull, machine_gauges, shown_session, usage,
    working_copy,
};
use adroit_core::tree::nav::Item;

/// The lines of a run of `runs`: the machine's meters or the agent's gauges
/// drawn together, or else the one line, its components beside each other.
#[component]
pub(super) fn RunView(
    run: Vec<Vec<Part>>,
    worktree: Option<PathBuf>,
    kind: Worktree,
    live_input_ms: Option<u64>,
    agent: Agent,
    now_ms: u64,
    on_kill: EventHandler<(Session, Process)>,
    on_menu: EventHandler<LinkMenu>,
) -> Element {
    let app = use_context::<AppCtx>();
    let all = |f: fn(Component) -> bool| run.iter().flatten().all(|p| f(p.placed.component));
    if all(|c| c.meter().is_some()) {
        let lines = run
            .iter()
            .map(|line| {
                line.iter()
                    .filter_map(|p| p.placed.component.meter())
                    .flat_map(|meter| machine_gauges(&app, &app.shown_environment(), &meter))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        return rsx! { MachineUsage { lines } };
    }
    if all(|c| c.gauge().is_some()) {
        let details = app.details.read().clone();
        let hosts = app.hosts.read();
        let work = hosts.work(&app.shown_environment());
        let gauges = usage(&details, work, now_ms);
        let lines = run
            .iter()
            .map(|line| {
                line.iter()
                    .filter_map(|p| p.placed.component.gauge())
                    .flat_map(|k| gauges.iter().filter(move |g| g.kind == k).cloned())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let usage_url = work
            .and_then(|w| w.agents.of(agent))
            .and_then(|facts| facts.usage_url.clone());
        return rsx! { AgentUsage { lines, agent, usage_url } };
    }
    rsx! {
        for line in run {
            if line.len() == 1 {
                PartView {
                    key: "{line[0].place}-{line[0].placed.component.name()}",
                    part: line[0].clone(),
                    worktree: worktree.clone(),
                    kind,
                    live_input_ms,
                    agent,
                    now_ms,
                    on_kill,
                    on_menu,
                }
            } else {
                // Only a line of several is wrapped, so the panel's rules on
                // what follows what still hold between lines of one.
                div {
                    key: "{line[0].place}-{line[0].placed.component.name()}",
                    class: "details-line beside",
                    style: "--beside:{line.len()}",
                    for part in line.iter().cloned() {
                        PartView {
                            key: "{part.place}-{part.placed.component.name()}",
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
        Component::Timeline => {
            if let Some(Entry::Progress(progress)) = rows.first() {
                return rsx! {
                    ProgressCard { progress: progress.clone(), at: base, selected: selected == Some(base), now_ms }
                };
            }
            // Folded, the timeline sums up the tree at work, and says nothing
            // while there is none.
            let has_session = {
                let details = app.details.read();
                let hosts = app.hosts.read();
                !adroit_core::details::is_folded(&details, adroit_core::details::Group::Timeline)
                    && shown_session(&details, hosts.work(&app.shown_environment())).is_some()
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
        Component::Changes => {
            rsx! { Changed { rows, said: part.said.clone(), base, selected, worktree, kind } }
        }
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
                    GroupedRows { rows: vec![heading], said: part.said.clone(), base, paths_from: PathBuf::new(), now_ms, on_kill, on_menu }
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
        // A meter or a gauge on a line with other components is drawn on
        // its own there, as its run would draw it.
        c if c.meter().is_some() => {
            let meter = c
                .meter()
                .unwrap_or(adroit_model::config::UsageSettings::NONE);
            let lines = vec![machine_gauges(&app, &app.shown_environment(), &meter)];
            rsx! { MachineUsage { lines } }
        }
        c if c.gauge().is_some() => {
            let details = app.details.read().clone();
            let hosts = app.hosts.read();
            let work = hosts.work(&app.shown_environment());
            let kind = c.gauge();
            let lines = vec![
                usage(&details, work, now_ms)
                    .into_iter()
                    .filter(|g| Some(g.kind) == kind)
                    .collect::<Vec<_>>(),
            ];
            let usage_url = work
                .and_then(|w| w.agents.of(agent))
                .and_then(|facts| facts.usage_url.clone());
            rsx! { AgentUsage { lines, agent, usage_url } }
        }
        Component::Environment => rsx! { EnvironmentCard { environment } },
        Component::EnvironmentRetry => {
            rsx! { EnvironmentButton { press: EnvironmentPress::Retry, placed: part.placed.clone(), environment } }
        }
        Component::EnvironmentPair => {
            rsx! { EnvironmentButton { press: EnvironmentPress::Pair, placed: part.placed.clone(), environment } }
        }
        Component::EnvironmentSettings => {
            rsx! { EnvironmentButton { press: EnvironmentPress::Settings, placed: part.placed.clone(), environment } }
        }
        Component::EnvironmentWaiting => rsx! { EnvironmentWaiting { environment } },
        Component::Profiles => rsx! { EnvironmentAgents { environment } },
        Component::Section => {
            let title = part.placed.title.clone().unwrap_or_default();
            rsx! { div { class: "details-section", span { class: "details-label", "{title}" } } }
        }
        c @ (Component::Github
        | Component::GithubShow
        | Component::GithubOpen
        | Component::GithubStart
        | Component::GithubConversation
        | Component::GithubFiles
        | Component::GithubChecks) => {
            use super::github::{
                GitHubButton, GitHubCard, GitHubChecks, GitHubConversation, GitHubFiles, Press,
            };
            use adroit_model::github::Listing as L;
            // The pull request or issue selected in the tree, or else the
            // pull request of the branch shown; placed among others, a branch
            // with none says nothing rather than that it has none.
            let target = match selected_item {
                Some(Item::GitHubItem(environment, root, listing, number)) => {
                    Some((environment, root, listing, number))
                }
                _ => {
                    let details = app.details.read();
                    let hosts = app.hosts.read();
                    branch_pull(&details, hosts.work(&environment))
                        .map(|(root, number)| (environment.clone(), root, L::Pulls, number))
                }
            };
            let Some((environment, root, listing, number)) = target else {
                return rsx! {};
            };
            match c {
                Component::GithubShow => {
                    rsx! { GitHubButton { press: Press::Show, placed: part.placed.clone(), environment, root, listing, number } }
                }
                Component::GithubOpen => {
                    rsx! { GitHubButton { press: Press::Open, placed: part.placed.clone(), environment, root, listing, number } }
                }
                Component::GithubStart => {
                    rsx! { GitHubButton { press: Press::Start, placed: part.placed.clone(), environment, root, listing, number } }
                }
                Component::GithubConversation => {
                    rsx! { GitHubConversation { environment, root, listing, number } }
                }
                Component::GithubFiles => {
                    rsx! { GitHubFiles { environment, root, listing, number } }
                }
                Component::GithubChecks => {
                    rsx! { GitHubChecks { environment, root, listing, number } }
                }
                _ => rsx! { GitHubCard { environment, root, listing, number } },
            }
        }
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
            rsx! { GroupedRows { rows, said: part.said.clone(), base, paths_from, now_ms, on_kill, on_menu } }
        }
    }
}

/// The current change's description and the files it changes.
#[component]
fn Changed(
    rows: Vec<Entry>,
    said: Option<String>,
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
                                        span { class: "list-row-name details-summary", {said.clone().unwrap_or_else(|| format!("{count} changed"))} }
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
