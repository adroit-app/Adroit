//! The configuration, read again on a beat and at once when the Settings
//! window saves it, whatever the window shows, with what the host makes of
//! the settings.

use std::time::Duration;

use adroit_protocol::ConfigChange;

use super::Watched;
use crate::settings;

/// How often the files are looked at for changes made by hand or by another
/// program.
const EVERY: Duration = Duration::from_secs(1);

pub(super) async fn watch(watched: Watched) {
    let mut saved = settings::CHANGED.subscribe();
    let config = watched.host().work().snapshot().config;
    let (mut last, mut configured) = (config.settings, config.configured);
    loop {
        tokio::select! {
            _ = tokio::time::sleep(EVERY) => {},
            _ = saved.recv() => {},
        };
        let Ok(config) = crate::environment::spawn_blocking(crate::config::files).await else {
            continue;
        };
        // A notification can belong to another environment.
        if config.settings != last
            && let Some(file) = &config.settings
        {
            settings::take_in(file);
        }
        last = config.settings.clone();
        // The theme's file is read only for a theme newly named.
        let made = configured.clone();
        let Ok(made) = crate::environment::spawn_blocking(move || {
            crate::config::configured(
                &settings::current(),
                crate::appearance::system(),
                made.as_ref(),
            )
        })
        .await
        else {
            continue;
        };
        let rethemed = configured
            .as_ref()
            .is_none_or(|last| last.theme != made.theme);
        if rethemed && let Ok(palette) = crate::themes::palette_of(&made.theme.colours) {
            crate::presences::retheme(&palette).await;
        }
        configured = Some(made.clone());
        let files = config
            .settings
            .map(|file| ConfigChange::Settings { file })
            .into_iter()
            .chain(config.keys.map(|file| ConfigChange::Keys { file }))
            .chain([ConfigChange::Configured { configured: made }]);
        watched.record(files);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::{Environment, run};
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn another_environments_save_does_not_hide_an_external_settings_edit() {
        let root = tempfile::tempdir().unwrap();
        let work = Environment::at("work", root.path()).unwrap();
        let personal = Environment::at("personal", root.path()).unwrap();
        let watched = Watched::new(
            Arc::new(Mutex::new(crate::work::Host::default())),
            Arc::new(|_, _| {}),
        );
        let task = crate::environment::spawn(run(Some(work.clone()), watch(watched)));
        tokio::task::yield_now().await;
        run(Some(work.clone()), async {
            std::fs::create_dir_all(crate::paths::config_dir()).unwrap();
            std::fs::write(settings::path(), "scratch_dir = '/external-work'\n").unwrap();
        })
        .await;
        run(Some(personal), async {
            settings::set(settings::current()).unwrap();
        })
        .await;
        let result = run(Some(work), async {
            tokio::time::timeout(Duration::from_millis(800), async {
                while settings::current().scratch_dir != "/external-work" {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
        })
        .await;
        task.abort();
        result.unwrap();
    }
}
