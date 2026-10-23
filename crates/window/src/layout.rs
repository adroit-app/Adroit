//! The side panels' layout, kept across runs of the window.

use std::path::PathBuf;

pub use adroit_core::layout::*;

use adroit_model::paths::state_dir;

/// A layout kept in the state folder from one run to the next.
pub trait Kept: Sized {
    /// The layout kept from the last run, or as designed.
    fn load() -> Self;
    /// Keep the layout for the next run; failing that, it is only lost.
    fn save(&self);
}

fn path() -> PathBuf {
    state_dir().join("adroit").join("layout.json")
}

impl Kept for Layout {
    fn load() -> Self {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let path = path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(self) {
            let _ = std::fs::write(path, text);
        }
    }
}
