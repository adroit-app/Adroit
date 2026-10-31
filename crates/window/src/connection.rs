//! How the window stands with each environment it shows: what the
//! environment's heading says of it, and the card the details show while
//! the heading is selected. The work shown may be stale while a link is
//! down, so how it stands stays in sight.

use adroit_connect::Target;
use adroit_connect::environments::Catalog;
use adroit_core::host::Link;
use adroit_model::EnvironmentId;
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::icons::{Icon, IconView};
use crate::link::{RemotePhase, RemoteStatus};

/// Where an environment works.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// On this computer.
    Local,
    /// On a work machine.
    Remote,
}

impl Kind {
    pub(crate) fn icon(self) -> Icon {
        match self {
            Kind::Local => Icon::DeviceDesktop,
            Kind::Remote => Icon::Server,
        }
    }
}

/// The dot before an environment's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
    /// The window and the environment's host work together.
    Connected,
    /// The host is being reached.
    Reaching,
    /// The host cannot be worked with.
    Stopped,
}

impl Mark {
    pub(crate) fn class(self) -> &'static str {
        match self {
            Mark::Connected => "ok",
            Mark::Reaching => "busy",
            Mark::Stopped => "fail",
        }
    }
}

/// What an environment's heading says beside its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Note {
    Connecting,
    Reconnecting,
    /// The work machine cannot be reached; its work was last received this
    /// many milliseconds ago, if ever.
    Offline {
        age_ms: Option<u64>,
    },
    ReadOnly,
    /// A change sent there has an outcome the user has to look at.
    SendUnconfirmed,
    /// The host speaks no version this window speaks.
    Incompatible,
}

impl Note {
    pub(crate) fn text(self) -> String {
        match self {
            Note::Connecting => "Connecting".into(),
            Note::Reconnecting => "Reconnecting".into(),
            Note::Offline { age_ms: None } => "Offline".into(),
            Note::Offline {
                age_ms: Some(age_ms),
            } => format!("Offline · {}", short_age(age_ms)),
            Note::ReadOnly => "Read only".into(),
            Note::SendUnconfirmed => "Send unconfirmed".into(),
            Note::Incompatible => "Incompatible".into(),
        }
    }

    pub(crate) fn class(self) -> &'static str {
        match self {
            Note::Connecting | Note::Reconnecting => "busy",
            Note::Offline { .. } | Note::Incompatible => "fail",
            Note::ReadOnly => "quiet",
            Note::SendUnconfirmed => "wait",
        }
    }

    pub(crate) fn icon(self) -> Option<Icon> {
        (self == Note::ReadOnly).then_some(Icon::Lock)
    }
}

/// How an environment's connection stands, as its heading and its card
/// say it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Standing {
    pub(crate) kind: Kind,
    pub(crate) mark: Mark,
    /// Nothing while it is connected and nothing waits on the user.
    pub(crate) note: Option<Note>,
}

impl Standing {
    /// How it stands, said in a few words.
    pub(crate) fn words(self) -> &'static str {
        match self.note {
            None => "Connected",
            Some(Note::ReadOnly) => "Connected, read only",
            Some(Note::SendUnconfirmed) => "Connected; a change sent there needs you",
            Some(Note::Connecting) => "Connecting",
            Some(Note::Reconnecting) => "Reconnecting",
            Some(Note::Offline { .. }) => "Connection stopped",
            Some(Note::Incompatible) => "The host speaks no version this window speaks",
        }
    }
}

/// How the window stands with an environment whose host link is `link`,
/// and whose work machine link is `remote` for one on a work machine, at
/// `now_ms`.
pub(crate) fn standing(link: &Link, remote: Option<&RemoteStatus>, now_ms: u64) -> Standing {
    let refused = matches!(link, Link::Refused { .. });
    let Some(remote) = remote else {
        let (mark, note) = match link {
            Link::Opening => (Mark::Reaching, Some(Note::Connecting)),
            Link::Ready { .. } => (Mark::Connected, None),
            Link::Refused { .. } => (Mark::Stopped, Some(Note::Incompatible)),
        };
        return Standing {
            kind: Kind::Local,
            mark,
            note,
        };
    };
    let (mark, note) = match remote.phase {
        RemotePhase::Connecting => (Mark::Reaching, Some(Note::Connecting)),
        RemotePhase::Reconnecting => (Mark::Reaching, Some(Note::Reconnecting)),
        RemotePhase::Error => {
            let age_ms = remote.last_work_ms.map(|at| now_ms.saturating_sub(at));
            (Mark::Stopped, Some(Note::Offline { age_ms }))
        }
        RemotePhase::Connected if refused => (Mark::Stopped, Some(Note::Incompatible)),
        RemotePhase::Connected if !remote.pending.is_empty() => {
            (Mark::Connected, Some(Note::SendUnconfirmed))
        }
        RemotePhase::Connected if !remote.writable => (Mark::Connected, Some(Note::ReadOnly)),
        RemotePhase::Connected => (Mark::Connected, None),
    };
    Standing {
        kind: Kind::Remote,
        mark,
        note,
    }
}

/// `ms` in the largest whole unit, as short as a heading has room for.
fn short_age(ms: u64) -> String {
    let s = ms / 1000;
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m", s / 60),
        3600..86_400 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86_400),
    }
}

/// How the window stands with `environment` now.
pub(crate) fn standing_of(app: &AppCtx, environment: &EnvironmentId) -> Standing {
    let hosts = app.hosts.read();
    let link = hosts
        .get(environment)
        .map(|h| h.link.clone())
        .unwrap_or_default();
    let remotes = app.remotes.read();
    standing(&link, remotes.get(environment), *app.clock.read())
}

/// The dot that marks how an environment's connection stands.
#[component]
pub(crate) fn StandingDot(mark: Mark) -> Element {
    rsx! {
        span { class: "standing-dot {mark.class()}", "aria-hidden": "true" }
    }
}

/// What an environment's heading says beside its name.
#[component]
pub(crate) fn StandingPill(note: Note) -> Element {
    rsx! {
        span { class: "standing-pill {note.class()}",
            if let Some(icon) = note.icon() {
                IconView { icon, size: 11 }
            }
            "{note.text()}"
        }
    }
}

/// The details of `environment` while its heading is selected: where it
/// is, how its connection stands and what went wrong, a way to try again at
/// once, and the drafts and changes waiting for it.
#[component]
pub(crate) fn EnvironmentCard(environment: EnvironmentId) -> Element {
    let app = use_context::<AppCtx>();
    let name = app.environment_name(&environment);
    let standing = standing_of(&app, &environment);
    let remote = app.remotes.read().get(&environment).cloned();
    let now = *app.clock.read();
    let retry = environment.clone();
    let pair = environment.clone();
    let last_work = remote
        .as_ref()
        .and_then(|r| r.last_work_ms)
        .map(|at| adroit_model::time::ago(now.saturating_sub(at)));
    let problem = remote.as_ref().and_then(|r| r.problem.clone());
    let retryable = remote.as_ref().is_some_and(|r| {
        r.retryable && matches!(r.phase, RemotePhase::Reconnecting | RemotePhase::Error)
    });
    let (pending, drafts) = remote
        .as_ref()
        .map(|r| (r.pending.clone(), r.drafts.clone()))
        .unwrap_or_default();
    rsx! {
        div { class: "environment-card",
            div { class: "environment-card-head",
                div { class: "environment-card-title",
                    StandingDot { mark: standing.mark }
                    IconView { icon: standing.kind.icon(), size: 14 }
                    span { class: "environment-card-name", "{name}" }
                }
                if let Some(remote) = &remote {
                    div { class: "environment-card-address", "{remote.host}" }
                }
            }
            div { class: "details-block",
                if let Some(problem) = problem {
                    div { class: "environment-card-problem", role: "status", "{problem}" }
                }
                if retryable || remote.is_some() {
                    div { class: "environment-card-actions",
                        if retryable {
                            button { class: "button primary", onclick: move |_| app.retry_remote(&retry),
                                IconView { icon: Icon::Refresh, size: 13 }
                                "Retry now"
                            }
                        }
                        if remote.is_some() {
                            button {
                                class: "button",
                                title: "Pair with this work machine again, as when it no longer accepts this computer",
                                onclick: move |_| match work_machine(&pair) {
                                    Some(address) => app.pair_again(address, app.environment_name(&pair)),
                                    None => app.notice("this environment's work machine is no longer listed".into()),
                                },
                                "Pair again"
                            }
                        }
                    }
                }
                div { class: "details-facts",
                    span { class: "details-facts-key", "Status" }
                    span { class: "details-facts-value", "{standing.words()}" }
                    if let Some(last_work) = last_work {
                        span { class: "details-facts-key", "Last work" }
                        span { class: "details-facts-value", "{last_work}" }
                    }
                }
            }
            if !pending.is_empty() || !drafts.is_empty() {
                div { class: "details-block",
                    span { class: "details-label", "Waiting for it" }
                    for record in pending.iter() {
                        PendingChange { environment: environment.clone(), record: record.clone() }
                    }
                    for record in drafts.iter() {
                        PendingChange { environment: environment.clone(), record: record.clone(), saved: true }
                    }
                }
            }
        }
    }
}

fn label(request: &adroit_protocol::Request) -> String {
    use adroit_protocol::{Ask, Request};
    match request {
        Request::Ask { ask, .. } => match ask.as_ref() {
            Ask::StartSession(start) => format!("Start session: {}", start.name),
            Ask::SubmitPrompt { .. } => "Send prompt".into(),
            Ask::StartTerminal(start) => format!("Open terminal: {}", start.name),
            Ask::KeepDrafts(_) => "Save drafts".into(),
            Ask::ConsumeDraft { .. } => "Finish a sent draft".into(),
            Ask::KeepImage { .. } => "Upload an image".into(),
            Ask::SaveSettings { .. } | Ask::SaveKeys { .. } => "Save settings".into(),
            Ask::Run { .. } => "Run a command on the work machine".into(),
            _ => "Change the remote work".into(),
        },
        Request::Stop { .. } | Request::Kill { .. } => "Stop a remote program".into(),
        Request::Rename { name, .. } => format!("Rename session: {name}"),
        Request::EndTerminal { .. } => "Close a terminal".into(),
        Request::Release { .. } => "Release a held program".into(),
        _ => "Change the remote work".into(),
    }
}

#[component]
fn PendingChange(
    environment: EnvironmentId,
    record: adroit_connect::journal::Record,
    #[props(default)] saved: bool,
) -> Element {
    let app = use_context::<AppCtx>();
    let ui = use_context::<crate::shell::Ui>();
    let title = label(&record.intent.request);
    let confirmed = saved
        || matches!(
            record.outcome,
            Some(
                adroit_protocol::MutationState::Result { .. }
                    | adroit_protocol::MutationState::Rejected { .. }
            )
        );
    let prompt_result = if saved {
        Some(false)
    } else {
        crate::prompts::remote::confirmed(&record.outcome)
    };
    let saved_stamp = record.intent.stamp.clone();
    let prompt_record = record.clone();
    let kept_record = record.clone();
    let kept_ui = ui.clone();
    let stamp = record.intent.stamp.clone();
    let query_stamp = stamp.clone();
    let (draft_in, query_in, resolve_in, sent_in, kept_in) = (
        environment.clone(),
        environment.clone(),
        environment.clone(),
        environment.clone(),
        environment,
    );
    rsx! {
        div { class: "pending-change",
            strong { "{title}" }
            p { if saved { "A durable copy of this unsent draft is retained. Restoring it does not send it." } else if confirmed { "The outcome was confirmed. It has not been marked as handled." } else { "The outcome is unconfirmed and may still finish. This change has not been sent again." } }
            if let Some(prompt) = &record.intent.prompt {
                textarea { readonly: true, value: "{prompt.text}", aria_label: "Retained prompt" }
            }
            div { class: "dialog-actions",
                if saved {
                    button { onclick: move |_| {
                        let removing = app.remote_draft(&draft_in, saved_stamp.clone(), true, true);
                        spawn(async move { if let Err(text) = removing.await { app.status(Some(text)); } });
                    }, "Delete saved copy" }
                }
                if !confirmed {
                    button { onclick: move |_| app.query_remote(&query_in, query_stamp.clone()), "Check outcome" }
                }
                if record.intent.prompt.is_none() {
                    button { onclick: move |_| {
                        let resolving = app.resolve_remote(&resolve_in, stamp.clone(), !confirmed);
                        spawn(async move { if let Err(text) = resolving.await { app.status(Some(text)); } });
                    }, if confirmed { "Done" } else { "I checked the work" } }
                } else {
                    if prompt_result != Some(false) {
                        button { onclick: move |_| {
                            let (ui, record, sent_in) = (ui.clone(), prompt_record.clone(), sent_in.clone());
                            spawn(async move { if let Err(text) = ui.handle_remote_prompt(&sent_in, record, true, !confirmed, false).await { app.status(Some(text)); } });
                        }, if prompt_result == Some(true) { "Clear sent draft" } else { "I checked: it was sent" } }
                    }
                    if prompt_result != Some(true) {
                        button { onclick: move |_| {
                            let (ui, record, kept_in) = (kept_ui.clone(), kept_record.clone(), kept_in.clone());
                            spawn(async move { if let Err(text) = ui.handle_remote_prompt(&kept_in, record, false, !confirmed, true).await { app.status(Some(text)); } });
                        }, if prompt_result == Some(false) { "Restore draft" } else { "I checked: restore unsent draft" } }
                    }
                }
            }
        }
    }
}

/// The address of the work machine `environment` works on, as the catalog
/// keeps it now.
fn work_machine(environment: &EnvironmentId) -> Option<String> {
    address_in(&Catalog::load(&Catalog::path()).ok()?, environment)
}

/// The address of the work machine `environment` works on in `catalog`,
/// none for one on this computer or one not kept.
fn address_in(catalog: &Catalog, environment: &EnvironmentId) -> Option<String> {
    let kept = catalog
        .environments
        .iter()
        .find(|e| EnvironmentId::new(&e.id) == *environment)?;
    match &kept.target {
        Target::Native { address, .. } => Some(address.clone()),
        Target::Local => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use adroit_connect::environments::Environment;

    use super::*;

    #[test]
    fn only_an_environment_kept_on_a_work_machine_has_one_to_pair_with_again() {
        let machine = Environment::new(
            "Box",
            Target::Native {
                address: "box.local:47130".into(),
                server: "ab".repeat(32),
            },
        );
        let here = Environment::new("Here", Target::Local);
        let catalog = Catalog {
            environments: vec![machine.clone(), here.clone()],
            ..Catalog::default()
        };
        let address = |e: &Environment| address_in(&catalog, &EnvironmentId::new(&e.id));
        assert_eq!(address(&machine).as_deref(), Some("box.local:47130"));
        assert_eq!(address(&here), None);
        assert_eq!(address_in(&catalog, &EnvironmentId::new("gone")), None);
    }

    fn remote(edit: impl FnOnce(&mut RemoteStatus)) -> RemoteStatus {
        let mut status = RemoteStatus::connected("box");
        edit(&mut status);
        status
    }

    /// A change sent to the work machine whose outcome is not known.
    fn sent() -> adroit_connect::journal::Record {
        use adroit_connect::journal::{Intent, Record};
        use adroit_protocol::{MutationStamp, Request};
        Record {
            intent: Intent {
                stamp: MutationStamp {
                    instance: "i".into(),
                    work: "w".into(),
                    terminal: "t".into(),
                    epoch: 1,
                    id: "1".into(),
                },
                request: Request::Snapshot,
                prompt: None,
            },
            outcome: None,
        }
    }

    fn ready() -> Link {
        Link::Ready {
            version: adroit_protocol::SPOKEN[0],
        }
    }

    fn refused() -> Link {
        Link::Refused {
            versions: Vec::new(),
        }
    }

    #[test]
    fn an_environment_on_this_computer_stands_as_its_host_link_does() {
        // (host link, mark, note)
        let cases = [
            (Link::Opening, Mark::Reaching, Some(Note::Connecting)),
            (ready(), Mark::Connected, None),
            (refused(), Mark::Stopped, Some(Note::Incompatible)),
        ];
        for (link, mark, note) in cases {
            let got = standing(&link, None, 0);
            assert_eq!(
                got,
                Standing {
                    kind: Kind::Local,
                    mark,
                    note
                },
                "{link:?}"
            );
        }
    }

    #[test]
    fn an_environment_on_a_work_machine_is_noted_only_while_something_needs_saying() {
        let pending = Arc::new(vec![sent()]);
        // (how the work machine's link stands, mark, note)
        let cases = [
            (remote(|_| {}), Mark::Connected, None),
            (
                remote(|s| s.phase = RemotePhase::Connecting),
                Mark::Reaching,
                Some(Note::Connecting),
            ),
            (
                remote(|s| s.phase = RemotePhase::Reconnecting),
                Mark::Reaching,
                Some(Note::Reconnecting),
            ),
            (
                remote(|s| s.writable = false),
                Mark::Connected,
                Some(Note::ReadOnly),
            ),
            (
                remote(|s| {
                    s.writable = false;
                    s.pending = pending.clone();
                }),
                Mark::Connected,
                Some(Note::SendUnconfirmed),
            ),
            (
                remote(|s| s.phase = RemotePhase::Error),
                Mark::Stopped,
                Some(Note::Offline { age_ms: None }),
            ),
            (
                remote(|s| {
                    s.phase = RemotePhase::Error;
                    s.last_work_ms = Some(4_000);
                }),
                Mark::Stopped,
                Some(Note::Offline {
                    age_ms: Some(12_000),
                }),
            ),
        ];
        for (status, mark, note) in cases {
            let got = standing(&ready(), Some(&status), 16_000);
            assert_eq!(
                got,
                Standing {
                    kind: Kind::Remote,
                    mark,
                    note
                },
                "{status:?}"
            );
        }
    }

    #[test]
    fn a_work_machine_connected_whose_host_speaks_no_version_of_the_window_s_is_incompatible() {
        let got = standing(&refused(), Some(&remote(|_| {})), 0);
        assert_eq!(got.mark, Mark::Stopped);
        assert_eq!(got.note, Some(Note::Incompatible));
    }

    #[test]
    fn how_long_offline_is_said_in_its_largest_whole_unit() {
        // (age in ms, the note)
        let cases = [
            (None, "Offline"),
            (Some(12_000), "Offline · 12s"),
            (Some(5 * 60_000 + 1), "Offline · 5m"),
            (Some(2 * 3_600_000), "Offline · 2h"),
            (Some(3 * 86_400_000), "Offline · 3d"),
        ];
        for (age_ms, want) in cases {
            assert_eq!(Note::Offline { age_ms }.text(), want, "{age_ms:?}");
        }
    }

    #[test]
    fn only_a_read_only_note_carries_a_lock() {
        assert_eq!(Note::ReadOnly.icon(), Some(Icon::Lock));
        for note in [
            Note::Connecting,
            Note::Reconnecting,
            Note::Offline { age_ms: None },
            Note::SendUnconfirmed,
            Note::Incompatible,
        ] {
            assert_eq!(note.icon(), None, "{note:?}");
        }
    }

    #[test]
    fn the_card_says_in_words_how_the_connection_stands() {
        let words = |note| {
            Standing {
                kind: Kind::Remote,
                mark: Mark::Connected,
                note,
            }
            .words()
        };
        assert_eq!(words(None), "Connected");
        assert_eq!(words(Some(Note::ReadOnly)), "Connected, read only");
        assert_eq!(
            words(Some(Note::Offline { age_ms: Some(1) })),
            "Connection stopped"
        );
    }
}
