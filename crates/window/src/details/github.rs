//! The details of a pull request or an issue while its row is selected.

use std::path::PathBuf;

use adroit_core::actions::Action;
use adroit_core::content::Area;
use adroit_core::msg::Msg;
use adroit_model::EnvironmentId;
use adroit_model::github::{Item, Listing, Review};
use adroit_model::time::{ago, parse_utc};
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::icons::{Icon, IconView};
use crate::nav;
use crate::shell::Ui;
use crate::viewers::label_colour;

/// What is known of the pull request or issue `number` of `listing` in the
/// repository at `root`, as last read: who opened it and when, how it
/// stands, its labels and, of a pull request, its branches, size, review
/// and checks; with ways to show it, open it on GitHub and start a session
/// on it.
#[component]
pub(crate) fn GitHubCard(
    environment: EnvironmentId,
    root: PathBuf,
    listing: Listing,
    number: u64,
) -> Element {
    let app = use_context::<AppCtx>();
    let ui = use_context::<Ui>();
    let now = *app.clock.read();
    let (slug, item) = {
        let hosts = app.hosts.read();
        let repository = adroit_core::tree::github(hosts.work(&environment), &root);
        (
            repository.map(|r| r.slug.clone()).unwrap_or_default(),
            repository.and_then(|r| r.item(listing, number)).cloned(),
        )
    };
    let Some(item) = item else {
        return rsx! {};
    };
    let when = |at: &str| parse_utc(at).map(|ms| ago(now.saturating_sub(ms)));
    let opened = when(&item.created_at);
    let updated = when(&item.updated_at).filter(|_| item.updated_at != item.created_at);
    let row = nav::Item::GitHubItem(environment.clone(), root.clone(), listing, number);
    let show = row.clone();
    rsx! {
        div { class: "environment-card github-card",
            div { class: "environment-card-head",
                div { class: "environment-card-title",
                    span { class: "environment-card-name", "{item.title}" }
                }
                div { class: "environment-card-address", "{slug}#{number}" }
            }
            div { class: "details-block",
                div { class: "details-facts",
                    span { class: "details-facts-key", "State" }
                    span { class: "details-facts-value", "{state(&item)}" }
                    span { class: "details-facts-key", "Opened by" }
                    span { class: "details-facts-value",
                        "@{item.author}"
                        if let Some(opened) = &opened { ", {opened}" }
                    }
                    if let Some(updated) = updated {
                        span { class: "details-facts-key", "Updated" }
                        span { class: "details-facts-value", "{updated}" }
                    }
                    if let Some(pull) = &item.pull {
                        span { class: "details-facts-key", "Branch" }
                        span { class: "details-facts-value github-card-mono", "{pull.head} → {pull.base}" }
                        span { class: "details-facts-key", "Changes" }
                        span { class: "details-facts-value",
                            span { class: "github-card-added", "+{pull.additions}" }
                            " "
                            span { class: "github-card-removed", "−{pull.deletions}" }
                            " · {files(pull.changed_files)}"
                        }
                        if pull.review.is_some() || !pull.requested.is_empty() {
                            span { class: "details-facts-key", "Review" }
                            span { class: "details-facts-value",
                                "{review(pull.review)}"
                                if !pull.requested.is_empty() {
                                    span { class: "details-facts-note",
                                        "Requested: {logins(&pull.requested)}"
                                    }
                                }
                            }
                        }
                        if pull.checks.standing().is_some() {
                            span { class: "details-facts-key", "Checks" }
                            span { class: "details-facts-value",
                                span { class: "github-card-check state-glyph done",
                                    IconView { icon: Icon::Check, size: 12 }
                                    "{pull.checks.passed}"
                                }
                                if pull.checks.failed > 0 {
                                    span { class: "github-card-check state-glyph fail",
                                        IconView { icon: Icon::X, size: 12 }
                                        "{pull.checks.failed}"
                                    }
                                }
                                if pull.checks.pending > 0 {
                                    span { class: "github-card-check state-glyph run",
                                        IconView { icon: Icon::Loader, size: 12 }
                                        "{pull.checks.pending}"
                                    }
                                }
                                for failing in pull.checks.failing.iter() {
                                    span { class: "details-facts-note", "✗ {failing}" }
                                }
                            }
                        }
                    }
                    if !item.labels.is_empty() {
                        span { class: "details-facts-key", "Labels" }
                        span { class: "details-facts-value github-card-labels",
                            for label in item.labels.iter() {
                                span {
                                    class: "github-label",
                                    style: "--label: #{label_colour(&label.color)}",
                                    "{label.name}"
                                }
                            }
                        }
                    }
                    if !item.assignees.is_empty() {
                        span { class: "details-facts-key", "Assignees" }
                        span { class: "details-facts-value", "{logins(&item.assignees)}" }
                    }
                    if let Some(milestone) = &item.milestone {
                        span { class: "details-facts-key", "Milestone" }
                        span { class: "details-facts-value", "{milestone}" }
                    }
                }
            }
            div { class: "details-block",
                div { class: "environment-card-actions",
                    button {
                        class: "button primary",
                        onclick: move |_| app.dispatch(Msg::OpenRow { item: show.clone(), how: None, take_keys: true }),
                        IconView { icon: Icon::ArrowRight, size: 13 }
                        "Show"
                    }
                    button {
                        class: "button",
                        onclick: {
                            let ui = ui.clone();
                            move |_| ui.perform(Action::CursorBrowse, None)
                        },
                        IconView { icon: Icon::BrandGithub, size: 13 }
                        "Open on GitHub"
                    }
                    button {
                        class: "button",
                        onclick: move |_| {
                            app.dispatch(Msg::Focus(Area::Sidebar));
                            app.dispatch(Msg::TreeSelect(Some(row.clone())));
                            ui.perform(Action::SessionNew, None);
                        },
                        IconView { icon: Icon::MessagePlus, size: 13 }
                        "Start a session"
                    }
                }
            }
        }
    }
}

/// How `item` stands, in a word.
fn state(item: &Item) -> &'static str {
    match (&item.pull, item.draft) {
        (Some(_), true) => "Draft",
        _ => "Open",
    }
}

fn review(review: Option<Review>) -> &'static str {
    match review {
        Some(Review::Approved) => "Approved",
        Some(Review::ChangesRequested) => "Changes requested",
        Some(Review::Required) => "Review required",
        None => "Awaiting review",
    }
}

fn files(n: u64) -> String {
    match n {
        1 => "1 file".into(),
        n => format!("{n} files"),
    }
}

fn logins(names: &[String]) -> String {
    names
        .iter()
        .map(|n| format!("@{n}"))
        .collect::<Vec<_>>()
        .join(", ")
}
