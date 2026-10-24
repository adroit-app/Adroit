//! What the window's tests start from: sessions as the window's tests name
//! them, the host's first listing of them, and the messages that show a tab.
//!
//! The work itself is built with `adroit_model::testing` and
//! `adroit_protocol::testing`; this adds only what the window's tests share
//! on top.

use adroit_model::Session;
pub use adroit_model::testing::{SessionBuilder, shell, worktree};
pub use adroit_protocol::testing::{
    ListedBuilder, PatchBuilder, REPO, SnapshotBuilder, listed, patch, snapshot,
};

use crate::content::Tab;
use crate::msg::Msg;
use crate::tree::nav::Place;

/// A session adroit runs in a worktree of its own under [`REPO`], named
/// unlike its id so a test sees which of the two the window shows.
pub fn session(id: &str) -> SessionBuilder {
    adroit_model::testing::session(id)
        .name(format!("{id}-name"))
        .cwd(format!("{REPO}/{id}"))
}

/// The host's first word, listing `sessions` and no shells.
pub fn listing<S: Into<Session>>(sessions: impl IntoIterator<Item = S>) -> SnapshotBuilder {
    snapshot().listed(listed().sessions(sessions))
}

/// Show `thing` in the focused window, wherever the tree stands.
pub fn show(thing: Tab) -> Msg {
    Msg::Show {
        thing,
        place: None,
        how: None,
    }
}

/// Show `thing` with the tree's cursor at the worktree `worktree`.
pub fn show_in(thing: Tab, worktree: &str) -> Msg {
    Msg::Show {
        thing,
        place: Some(Place {
            worktree: worktree.into(),
        }),
        how: None,
    }
}

pub fn session_tab(id: &str) -> Tab {
    Tab::Session(id.into())
}
