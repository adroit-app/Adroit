//! The prompts an agent kept as sent in a project, read each time the window
//! steps back or forth through them, as they grow with every prompt sent.

use adroit_protocol::{HistoryRead, PromptChange};

use super::Watched;

pub(super) async fn watch(mut watched: Watched) {
    while let Some(ask) = watched.asked_history().await {
        let project = ask.project.clone();
        // An agent's history can run to megabytes: not on a runtime thread.
        let sent = tokio::task::spawn_blocking(move || {
            adroit_agents::adapter(ask.agent).history(&project)
        })
        .await
        .unwrap_or_default();
        watched.record([PromptChange::History {
            read: HistoryRead {
                asked: ask.asked,
                project: ask.project,
                sent,
            },
        }]);
    }
}
