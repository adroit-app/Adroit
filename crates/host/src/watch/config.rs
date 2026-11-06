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
    // Apart from `configured`: the work host starts with the theme already
    // in force there, which no file has been written for yet.
    let mut written = None;
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
        if written.as_ref() != Some(&made.theme)
            && let Ok(palette) = crate::themes::palette_of(&made.theme.colours)
        {
            // A Neovim that cannot be told the new theme keeps drawing the one
            // it has, as a console whose palette cannot be set does.
            let wrote = crate::environment::spawn_blocking(move || {
                crate::neovim::write_theme(&crate::neovim::theme_file(), &palette)
            })
            .await;
            if matches!(wrote, Ok(Ok(()))) {
                written = Some(made.theme.clone());
            }
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
    use std::path::Path;
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

    #[tokio::test]
    async fn the_theme_file_holds_the_theme_in_force_and_follows_a_new_one() {
        let root = tempfile::tempdir().unwrap();
        let work = Environment::at("work", root.path()).unwrap();
        let theme = |name: &str| {
            let dark = settings::Settings {
                appearance: settings::Appearance::Dark,
                dark_theme: name.into(),
                ..settings::Settings::default()
            };
            run(
                Some(work.clone()),
                async move { settings::set(dark).unwrap() },
            )
        };
        let written = |name: &str| {
            let palette = crate::themes::palette(name, Path::new("/none")).unwrap();
            let work = work.clone();
            async move {
                run(Some(work), async {
                    tokio::time::timeout(Duration::from_secs(3), async {
                        let file = crate::neovim::theme_file();
                        while std::fs::read_to_string(&file).ok() != Some(palette.to_ghostty()) {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    })
                    .await
                })
                .await
            }
        };
        theme("Nightfox").await;
        // Seeded as the work host seeds it, with the theme already in force.
        let host = run(Some(work.clone()), async {
            crate::work::Host::new(crate::work::Work::configured(crate::config::read()))
        })
        .await;
        let watched = Watched::new(Arc::new(Mutex::new(host)), Arc::new(|_, _| {}));
        let task = crate::environment::spawn(run(Some(work.clone()), watch(watched)));
        let first = written("Nightfox").await;
        theme("Nordfox").await;
        let next = written("Nordfox").await;
        task.abort();
        first.unwrap();
        next.unwrap();
    }
}
