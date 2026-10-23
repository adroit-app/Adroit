//! What the main window hands the windows it opens beside it, the Settings
//! and image windows, which are drawn apart from it: the configuration as
//! the work host last sent it, and the colours they are drawn in.

use std::sync::LazyLock;

use adroit_core::config::Config;
use adroit_term::Palette;
use dioxus::prelude::*;
use tokio::sync::watch;

use crate::panes::palette_of;

static CONFIG: LazyLock<watch::Sender<Config>> =
    LazyLock::new(|| watch::Sender::new(Config::default()));

/// Hand `config` on, as the main window now works by it.
pub(crate) fn publish(config: &Config) {
    CONFIG.send_if_modified(|was| {
        let changed = was != config;
        if changed {
            *was = config.clone();
        }
        changed
    });
}

/// The configuration as the main window works by it, read again each time
/// it changes.
pub(crate) fn use_config() -> Signal<Config> {
    let mut now = use_signal(|| CONFIG.borrow().clone());
    use_hook(move || {
        spawn(async move {
            let mut changed = CONFIG.subscribe();
            while changed.changed().await.is_ok() {
                let config = changed.borrow_and_update().clone();
                now.set(config);
            }
        });
    });
    now
}

/// The window's colours as `config`'s theme has them, as a stylesheet's
/// root.
pub(crate) fn root_style(config: &Config) -> String {
    let palette = config
        .files()
        .configured
        .as_ref()
        .map_or_else(Palette::default, |c| palette_of(&c.theme.colours));
    format!(
        ":root{{{}}}",
        crate::themes::css(&crate::themes::window(&palette))
    )
}
