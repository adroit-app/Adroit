//! Sessions and terminals started with their program held back until the
//! window that shows them has told them the theme's colours. Each is let go
//! once: when the window says it attached, or after a while without a word,
//! as when the window closed meanwhile.

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

/// How long a program is held for a window that does not say it attached.
const AT_MOST: Duration = Duration::from_secs(10);

/// The programs held now, by their session's id.
#[derive(Debug, Default)]
struct Held(HashSet<u128>);

impl Held {
    fn hold(&mut self, id: u128) {
        self.0.insert(id);
    }

    /// Whether `id` was held, and is to be let go now.
    fn take(&mut self, id: u128) -> bool {
        self.0.remove(&id)
    }
}

static HELD: LazyLock<Mutex<Held>> = LazyLock::new(Mutex::default);

fn take(id: u128) -> bool {
    HELD.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take(id)
}

/// Hold the program of the session `id`, which was started held, until it
/// is let go; it is let go anyway after a while.
pub(crate) fn hold(id: u128) {
    HELD.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .hold(id);
    tokio::spawn(async move {
        tokio::time::sleep(AT_MOST).await;
        release(id).await;
    });
}

/// Let the program of the session `id` go, if it is still held.
pub(crate) async fn release(id: u128) {
    if take(id) {
        // A session ended meanwhile has nothing to let go.
        let _ = crate::daemon::daemon().release(id).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_program_is_let_go_once_and_one_never_held_not_at_all() {
        let mut held = Held::default();
        held.hold(7);
        assert!(held.take(7));
        assert!(!held.take(7));
        assert!(!held.take(8));
    }
}
