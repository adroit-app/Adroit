//! The details of a pull request or an issue while its row is selected.

use std::path::PathBuf;

use adroit_core::actions::Action;
use adroit_core::content::Area;
use adroit_core::msg::Msg;
use adroit_model::EnvironmentId;
use adroit_model::github::{Item, Listing, Outcome, Review, Thread};
use adroit_model::time::{ago, parse_utc};
use adroit_protocol::{Answer, Ask};
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::components::{CopyValue, ListRow, PlacedButton};
use crate::icons::{Icon, IconView};
use crate::nav;
use crate::shell::Ui;
use crate::viewers::label_colour;

/// What is known of the pull request or issue `number` of `listing` in the
/// repository at `root`, as last read: who opened it and when, how it
/// stands, its labels and, of a pull request, its branches, size, review
/// and checks.
#[component]
pub(crate) fn GitHubCard(
    environment: EnvironmentId,
    root: PathBuf,
    listing: Listing,
    number: u64,
) -> Element {
    let app = use_context::<AppCtx>();
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
                    span { class: "details-facts-value",
                        {
                            let (word, tone) = state(&item);
                            rsx! { span { class: "state-badge {tone}", "{word}" } }
                        }
                    }
                    span { class: "details-facts-key", "Opened by" }
                    span { class: "details-facts-value",
                        "@{item.author}"
                        if let Some(opened) = &opened { ", {opened}" }
                        CopyValue { text: item.author.clone() }
                    }
                    if let Some(updated) = updated {
                        span { class: "details-facts-key", "Updated" }
                        span { class: "details-facts-value", "{updated}" CopyValue { text: updated.clone() } }
                    }
                    if let Some(pull) = &item.pull {
                        span { class: "details-facts-key", "Branch" }
                        span { class: "details-facts-value github-card-mono",
                            "{pull.head} → {pull.base}"
                            CopyValue { text: pull.head.clone() }
                        }
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
                                {
                                    let (word, tone) = review(pull.review);
                                    rsx! { span { class: "state-badge {tone}", "{word}" } }
                                }
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
                                        IconView { icon: Icon::Progress, size: 12 }
                                        "{pull.checks.pending}"
                                    }
                                }
                                for failing in pull.checks.failing.iter() {
                                    {
                                        let checks = format!("{}/checks", item.url.trim_end_matches('/'));
                                        rsx! {
                                            button {
                                                class: "details-facts-note github-check-failed github-check-link",
                                                r#type: "button",
                                                title: "Show the checks on GitHub",
                                                onclick: move |_| crate::bridge::open([checks.clone()]),
                                                "✗ {failing}"
                                            }
                                        }
                                    }
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
                        span { class: "details-facts-value", "{logins(&item.assignees)}" CopyValue { text: logins(&item.assignees) } }
                    }
                    if let Some(milestone) = &item.milestone {
                        span { class: "details-facts-key", "Milestone" }
                        span { class: "details-facts-value", "{milestone}" CopyValue { text: milestone.clone() } }
                    }
                }
            }
        }
    }
}

/// What a button of a pull request or issue does.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Press {
    Show,
    Open,
    Start,
}

/// A button for the pull request or issue `number` of the repository at
/// `root`: one that shows it, opens it on GitHub or starts a session on it.
#[component]
pub(crate) fn GitHubButton(
    press: Press,
    placed: adroit_core::details::Placed,
    environment: EnvironmentId,
    root: PathBuf,
    listing: Listing,
    number: u64,
) -> Element {
    let app = use_context::<AppCtx>();
    let ui = use_context::<Ui>();
    let url = {
        let hosts = app.hosts.read();
        adroit_core::tree::github(hosts.work(&environment), &root)
            .and_then(|r| r.item(listing, number))
            .map(|i| i.url.clone())
    };
    let row = nav::Item::GitHubItem(environment, root, listing, number);
    let disabled = press == Press::Open && url.is_none();
    rsx! {
        PlacedButton {
            placed,
            disabled,
            onclick: move |_| match press {
                Press::Show => app.dispatch(Msg::OpenRow { item: row.clone(), how: None, take_keys: true }),
                Press::Open => {
                    if let Some(url) = url.clone() {
                        crate::bridge::open([url]);
                    }
                }
                Press::Start => {
                    app.dispatch(Msg::Focus(Area::Sidebar));
                    app.dispatch(Msg::TreeSelect(Some(row.clone())));
                    ui.perform(Action::SessionNew, None);
                }
            },
        }
    }
}

/// The thread of the pull request or issue `number` of the repository at
/// `root`, asked of the work host as the component that shows it appears
/// and again now and then: none until it answers, and what it last said
/// while a later ask fails.
fn use_thread(
    environment: EnvironmentId,
    root: PathBuf,
    listing: Listing,
    number: u64,
) -> Signal<Option<Result<Thread, String>>> {
    let app = use_context::<AppCtx>();
    let mut read = use_signal(|| None::<Result<Thread, String>>);
    // What the thread read is of: one that answers for another pull request
    // or issue than the one now asked of is dropped.
    let mut of = use_signal(|| None::<(EnvironmentId, PathBuf, Listing, u64)>);
    let mut reads = use_signal(|| 0_u64);
    use_hook(move || {
        spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                reads += 1;
            }
        })
    });
    use_effect(use_reactive(
        (&environment, &root, &listing, &number),
        move |(environment, root, listing, number)| {
            let _ = reads();
            let key = (environment.clone(), root.clone(), listing, number);
            if of.peek().as_ref() != Some(&key) {
                of.set(Some(key.clone()));
                read.set(None);
            }
            let asked = app.ask_in(
                &environment,
                Ask::GitHubThread {
                    repository: root,
                    listing,
                    number,
                },
            );
            spawn(async move {
                let answer = match asked.await {
                    Answer::Thread { thread } => Ok(*thread),
                    Answer::Failed { text } => Err(text),
                    other => Err(format!("the work host answered {other:?}")),
                };
                if of.peek().as_ref() != Some(&key) {
                    return;
                }
                let keeps = answer.is_err() && matches!(*read.peek(), Some(Ok(_)));
                if !keeps {
                    read.set(Some(answer));
                }
            });
        },
    ));
    read
}

/// What was said on the pull request or issue, each post in brief: who,
/// what they did and when, and its first lines.
#[component]
pub(crate) fn GitHubConversation(
    environment: EnvironmentId,
    root: PathBuf,
    listing: Listing,
    number: u64,
) -> Element {
    let app = use_context::<AppCtx>();
    let read = use_thread(environment, root, listing, number);
    let now = *app.clock.read();
    rsx! {
        match &*read.read() {
            None => rsx! { div { class: "details-block timeline-empty", "Reading…" } },
            Some(Err(e)) => rsx! { div { class: "details-block git-error", "{e}" } },
            Some(Ok(thread)) => rsx! {
                div { class: "details-block github-posts",
                    for (i , post) in std::iter::once(&thread.body).chain(&thread.said).enumerate() {
                        div { key: "{i}", class: "github-post",
                            div { class: "github-post-head",
                                span { class: "github-post-author", "@{post.author}" }
                                span { class: "github-post-did", "{post.did.words()}" }
                                if let Some(at) = parse_utc(&post.at) {
                                    span { class: "details-link-tag", "{ago(now.saturating_sub(at))}" }
                                }
                            }
                            if !post.body.trim().is_empty() {
                                div { class: "github-post-body", "{post.body.trim()}" }
                            }
                        }
                    }
                }
            },
        }
    }
}

/// The files the pull request changes, each with the lines it adds and
/// takes out.
#[component]
pub(crate) fn GitHubFiles(
    environment: EnvironmentId,
    root: PathBuf,
    listing: Listing,
    number: u64,
) -> Element {
    let read = use_thread(environment, root, listing, number);
    rsx! {
        match &*read.read() {
            None => rsx! { div { class: "details-block timeline-empty", "Reading…" } },
            Some(Err(e)) => rsx! { div { class: "details-block git-error", "{e}" } },
            Some(Ok(thread)) if thread.item.pull.is_none() => rsx! {},
            Some(Ok(thread)) => rsx! {
                div { class: "list-rows details-git",
                    for file in thread.files.iter() {
                        ListRow { key: "{file.path}", kind: "mono", title: "{file.path}",
                            span { class: "details-file-status", "{status_letter(&file.status)}" }
                            span { class: "list-row-name", "{file.path}" }
                            span { class: "details-link-tag",
                                span { class: "github-card-added", "+{file.additions}" }
                                " "
                                span { class: "github-card-removed", "−{file.deletions}" }
                            }
                        }
                    }
                }
            },
        }
    }
}

/// The checks on the pull request, those that failed first, each opening
/// where it tells more.
#[component]
pub(crate) fn GitHubChecks(
    environment: EnvironmentId,
    root: PathBuf,
    listing: Listing,
    number: u64,
) -> Element {
    let read = use_thread(environment, root, listing, number);
    rsx! {
        match &*read.read() {
            None => rsx! { div { class: "details-block timeline-empty", "Reading…" } },
            Some(Err(e)) => rsx! { div { class: "details-block git-error", "{e}" } },
            Some(Ok(thread)) if thread.item.pull.is_none() => rsx! {},
            Some(Ok(thread)) => rsx! {
                div { class: "details-block details-links",
                    for (i , check) in thread.checks.iter().cloned().enumerate() {
                        ListRow { key: "{i}-{check.name}",
                            kind: "details-link",
                            title: "{check.said}",
                            onclick: move |_| {
                                if !check.link.is_empty() {
                                    crate::bridge::open([check.link.clone()]);
                                }
                            },
                            {
                                let (tone, icon) = outcome_glyph(check.outcome);
                                rsx! { span { class: "state-glyph {tone}", IconView { icon, size: 13 } } }
                            }
                            span { class: if check.outcome == Outcome::Failed { "list-row-name github-check-failed" } else { "list-row-name" }, "{check.name}" }
                            if !check.by.is_empty() {
                                span { class: "details-link-tag", "{check.by}" }
                            }
                        }
                    }
                }
            },
        }
    }
}

fn status_letter(status: &str) -> &'static str {
    match status {
        "added" => "A",
        "removed" => "D",
        "renamed" => "R",
        _ => "M",
    }
}

fn outcome_glyph(outcome: Outcome) -> (&'static str, Icon) {
    match outcome {
        Outcome::Failed => ("fail", Icon::X),
        Outcome::Pending => ("run", Icon::Progress),
        Outcome::Passed => ("done", Icon::Check),
        Outcome::Skipped => ("idle", Icon::SquareMinus),
    }
}

/// How `item` stands, in a word, with the tone of its badge.
fn state(item: &Item) -> (&'static str, &'static str) {
    match (&item.pull, item.draft) {
        (Some(_), true) => ("Draft", "idle"),
        _ => ("Open", "run"),
    }
}

fn review(review: Option<Review>) -> (&'static str, &'static str) {
    match review {
        Some(Review::Approved) => ("Approved", "done"),
        Some(Review::ChangesRequested) => ("Changes requested", "fail"),
        Some(Review::Required) => ("Review required", "wait"),
        None => ("Awaiting review", "idle"),
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
