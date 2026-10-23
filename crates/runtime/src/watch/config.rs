//! The configuration, read again on a beat and at once when the Settings
//! window saves it, whatever the window shows.

use std::time::Duration;

use adroit_protocol::ConfigChange;
use tokio::sync::broadcast::error::RecvError;

use super::Watched;
use crate::settings;

/// How often the files are looked at for changes made by hand or by another
/// program.
const EVERY: Duration = Duration::from_secs(1);

pub(super) async fn watch(watched: Watched) {
    let mut saved = settings::CHANGED.subscribe();
    let mut last = watched.host().work().snapshot().config.settings;
    loop {
        let by_window = tokio::select! {
            _ = tokio::time::sleep(EVERY) => false,
            told = saved.recv() => !matches!(told, Err(RecvError::Closed)),
        };
        let Ok(config) = tokio::task::spawn_blocking(crate::config::read).await else {
            continue;
        };
        // The Settings window took in what it saved itself.
        if !by_window
            && config.settings != last
            && let Some(file) = &config.settings
        {
            settings::take_in(file);
        }
        last = config.settings.clone();
        let files = config
            .settings
            .map(|file| ConfigChange::Settings { file })
            .into_iter()
            .chain(config.keys.map(|file| ConfigChange::Keys { file }));
        watched.record(files);
    }
}
