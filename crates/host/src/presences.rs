//! The work host's own window on each session and shell it starts, kept
//! while the session runs, so its program is told the theme whenever it
//! asks, whether a window shows it then or not.

use std::sync::LazyLock;

use adroit_term::{Palette, Presence};
use tokio::sync::Mutex;

static KEPT: LazyLock<Mutex<Vec<Presence>>> = LazyLock::new(Mutex::default);

/// Keep `presence` until its session ends.
pub(crate) async fn keep(presence: Presence) {
    let mut kept = KEPT.lock().await;
    kept.retain(|p| !p.ended());
    kept.push(presence);
}

/// Tell every session kept `palette` from now on.
pub(crate) async fn retheme(palette: &Palette) {
    let mut kept = KEPT.lock().await;
    kept.retain(|p| !p.ended());
    for presence in kept.iter() {
        // A session ending meanwhile has no theme to take.
        let _ = presence.set_palette(palette).await;
    }
}
