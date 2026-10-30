//! Connection state stays visible while the work shown may be stale.

use crate::bridge::AppCtx;
use crate::link::RemotePhase;
use dioxus::prelude::*;
use std::time::Duration;

#[component]
pub(super) fn RemoteConnection() -> Element {
    let app = use_context::<AppCtx>();
    let mut status = use_signal(move || app.remote_status());
    let mut now = use_signal(adroit_model::time::now_ms);
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let next = app.remote_status();
            if *status.peek() != next {
                status.set(next);
            }
            now.set(adroit_model::time::now_ms());
        }
    });
    let Some(status) = status() else {
        return rsx! {};
    };
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
    let age = status
        .last_work_ms
        .map(|at| now().saturating_sub(at) / 1000);
    rsx! {
        div { class: "remote-connection", role: "status",
            span { "{status.host} · {phase}" }
            if status.phase != RemotePhase::Connected {
                if let Some(age) = age { span { " · Work last received {age}s ago" } }
            }
            if let Some(problem) = status.problem { span { " · {problem}" } }
            if status.retryable && status.phase == RemotePhase::Error {
                button { onclick: move |_| app.retry_remote(), "Retry" }
            }
        }
        if !status.pending.is_empty() || !status.drafts.is_empty() {
            div { class: "remote-pending",
                for record in status.pending.iter() {
                    PendingChange { record: record.clone() }
                }
                for record in status.drafts.iter() {
                    PendingChange { record: record.clone(), saved: true }
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
                        let removing = app.remote_draft(saved_stamp.clone(), true, true);
                        spawn(async move { if let Err(text) = removing.await { app.status(Some(text)); } });
                    }, "Delete saved copy" }
                }
                if !confirmed {
                    button { onclick: move |_| app.query_remote(query_stamp.clone()), "Check outcome" }
                }
                if record.intent.prompt.is_none() {
                    button { onclick: move |_| {
                        let resolving = app.resolve_remote(stamp.clone(), !confirmed);
                        spawn(async move { if let Err(text) = resolving.await { app.status(Some(text)); } });
                    }, if confirmed { "Done" } else { "I checked the work" } }
                } else {
                    if prompt_result != Some(false) {
                        button { onclick: move |_| {
                            let (ui, record) = (ui.clone(), prompt_record.clone());
                            spawn(async move { if let Err(text) = ui.handle_remote_prompt(record, true, !confirmed, false).await { app.status(Some(text)); } });
                        }, if prompt_result == Some(true) { "Clear sent draft" } else { "I checked: it was sent" } }
                    }
                    if prompt_result != Some(true) {
                        button { onclick: move |_| {
                            let (ui, record) = (kept_ui.clone(), kept_record.clone());
                            spawn(async move { if let Err(text) = ui.handle_remote_prompt(record, false, !confirmed, true).await { app.status(Some(text)); } });
                        }, if prompt_result == Some(false) { "Restore draft" } else { "I checked: restore unsent draft" } }
                    }
                }
            }
        }
    }
}
