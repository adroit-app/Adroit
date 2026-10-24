//! The prompts an agent kept as sent in a project, read each time a window
//! steps back or forth through them, as they grow with every prompt sent,
//! and told that window.

use adroit_protocol::{HistoryRead, PromptChange};

use tokio::sync::mpsc::UnboundedReceiver;

use super::Watched;
use crate::work::HistoryAsk;

pub(super) async fn watch(watched: Watched, mut asks: UnboundedReceiver<HistoryAsk>) {
    while let Some(ask) = asks.recv().await {
        let project = ask.project.clone();
        // An agent's history can run to megabytes: not on a runtime thread.
        let sent = tokio::task::spawn_blocking(move || {
            adroit_agents::adapter(ask.agent).history(
                &crate::programs::context(ask.agent),
                &crate::paths::home(),
                &project,
            )
        })
        .await
        .unwrap_or_default();
        watched.record_for(
            ask.window,
            [PromptChange::History {
                read: HistoryRead {
                    asked: ask.asked,
                    project: ask.project,
                    sent,
                },
            }],
        );
    }
}
