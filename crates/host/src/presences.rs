//! The work host's own window on each session and shell it starts, kept
//! while the session runs, so its program is told the theme whenever it
//! asks, whether a window shows it then or not.

use std::sync::LazyLock;

use adroit_term::{Palette, Presence};
use tokio::sync::Mutex;

type Kept = Vec<(Option<String>, Presence)>;
static KEPT: LazyLock<Mutex<Kept>> = LazyLock::new(Mutex::default);

/// Keep `presence` until its session ends.
pub(crate) async fn keep(presence: Presence) {
    let mut kept = KEPT.lock().await;
    kept.retain(|(_, p)| !p.ended());
    kept.push((
        crate::environment::current().map(|c| c.id.clone()),
        presence,
    ));
}

/// Tell every session kept `palette` from now on.
pub(crate) async fn retheme(palette: &Palette) {
    let mut kept = KEPT.lock().await;
    kept.retain(|(_, p)| !p.ended());
    let environment = crate::environment::current().map(|c| c.id.clone());
    for (_, presence) in kept.iter().filter(|(owned, _)| *owned == environment) {
        // A session ending meanwhile has no theme to take.
        let _ = presence.set_palette(palette).await;
    }
}
