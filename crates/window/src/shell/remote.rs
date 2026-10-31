//! Connection state stays visible while the work shown may be stale.

use crate::bridge::AppCtx;
use crate::link::{RemotePhase, RemoteStatus};
use adroit_model::EnvironmentId;
use dioxus::prelude::*;

/// How the window stands with each environment on a work machine, and the
/// changes sent there whose outcome needs the user. A window showing one
/// environment says how its link stands above everything; one showing
/// several says it on each environment's heading, and only the link that
/// stopped and the changes waiting are said here, named after their
/// environment.
#[component]
pub(super) fn RemoteConnection() -> Element {
    let app = use_context::<AppCtx>();
    let several = app.hosts.read().each().count() > 1;
    let remotes = app.remotes.read().clone();
    rsx! {
        for (environment, status) in remotes {
            RemoteEnvironment {
                key: "{environment}",
                name: several.then(|| app.environment_name(&environment)),
                environment,
                status,
            }
        }
    }
}

/// What a heading of an environment on a work machine adds to its name, as
/// `status` tells how its link stands at `now_ms`: nothing while it is
/// connected and nothing waits on the user.
pub(crate) fn heading_note(status: &RemoteStatus, now_ms: u64) -> Option<String> {
    let phase = match status.phase {
        RemotePhase::Connected if !status.pending.is_empty() => "Outcome needs attention",
        RemotePhase::Connected if status.writable => return None,
        RemotePhase::Connected => "Read only",
        RemotePhase::Connecting => "Connecting",
        RemotePhase::Reconnecting => "Reconnecting",
        RemotePhase::Error => "Connection stopped",
    };
    let age = status
        .last_work_ms
        .filter(|_| status.phase != RemotePhase::Connected)
        .map(|at| now_ms.saturating_sub(at) / 1000);
    Some(match age {
        Some(age) => format!("{phase} · work {age}s old"),
        None => phase.to_string(),
    })
}

#[component]
fn RemoteEnvironment(
    environment: EnvironmentId,
    status: RemoteStatus,
    /// The environment's name, while the window shows several.
    name: Option<String>,
) -> Element {
    let app = use_context::<AppCtx>();
    let now = app.clock.read();
    let phase = match status.phase {
        RemotePhase::Connecting => "Connecting",
        RemotePhase::Connected if !status.pending.is_empty() => {
            "Connected · outcome needs attention"
        }
        RemotePhase::Connected if status.writable => "Connected",
        RemotePhase::Connected => "Connected · read only",
        RemotePhase::Reconnecting => "Reconnecting",
        RemotePhase::Error => "Connection stopped",
    };
    let age = status.last_work_ms.map(|at| now.saturating_sub(at) / 1000);
    let said = name.is_none() || status.phase == RemotePhase::Error;
    let named = name.as_ref().map(|n| format!("{n} · ")).unwrap_or_default();
    let retry = environment.clone();
    rsx! {
        if said {
            div { class: "remote-connection", role: "status",
                span { "{named}{status.host} · {phase}" }
                if status.phase != RemotePhase::Connected {
                    if let Some(age) = age { span { " · Work last received {age}s ago" } }
                }
                if let Some(problem) = &status.problem { span { " · {problem}" } }
                if status.retryable && status.phase == RemotePhase::Error {
                    button { onclick: move |_| app.retry_remote(&retry), "Retry" }
                }
            }
        }
        if !status.pending.is_empty() || !status.drafts.is_empty() {
            div { class: "remote-pending",
                if let Some(name) = &name {
                    p { class: "remote-pending-environment", "{name}" }
                }
                for record in status.pending.iter() {
                    PendingChange { environment: environment.clone(), record: record.clone() }
                }
                for record in status.drafts.iter() {
                    PendingChange { environment: environment.clone(), record: record.clone(), saved: true }
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
    let ui = use_context::<super::Ui>();
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
        div { class: "remote-pending-change",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::RemoteStatus;

    #[test]
    fn an_environment_s_heading_tells_how_its_link_stands_only_while_it_needs_saying() {
        let connected = RemoteStatus::connected("box");
        assert_eq!(heading_note(&connected, 10_000), None);
        let mut reconnecting = RemoteStatus::connected("box");
        reconnecting.phase = RemotePhase::Reconnecting;
        reconnecting.last_work_ms = Some(4_000);
        assert_eq!(
            heading_note(&reconnecting, 16_000).as_deref(),
            Some("Reconnecting · work 12s old")
        );
        let mut read_only = RemoteStatus::connected("box");
        read_only.writable = false;
        assert_eq!(heading_note(&read_only, 0).as_deref(), Some("Read only"));
    }
}
