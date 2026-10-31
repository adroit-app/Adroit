//! What the window's tests start from: sessions as the window's tests name
//! them, the host's first listing of them, and the messages that show a tab.
//!
//! The work itself is built with `adroit_model::testing` and
//! `adroit_protocol::testing`; this adds only what the window's tests share
//! on top.

pub use adroit_model::testing::{SessionBuilder, shell, worktree};
use adroit_model::{EnvironmentId, Session};
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
        place: Some(Place::at(&here(), worktree)),
        how: None,
    }
}

/// The environment a test's window is in: that of a window opened without
/// one.
pub fn here() -> EnvironmentId {
    EnvironmentId::unbound()
}

pub fn session_tab(id: &str) -> Tab {
    Tab::Session(here(), id.into())
}

/// An environment a test's window shows beside its own.
pub fn there() -> EnvironmentId {
    EnvironmentId::new("there")
}

/// The window shows `there()` too, called "There", whose host lists
/// `sessions`.
pub fn shows_there<S: Into<Session>>(
    run: crate::scenario::Scenario,
    sessions: impl IntoIterator<Item = S>,
) -> crate::scenario::Scenario {
    run.send(Msg::EnvironmentOpened {
        environment: there(),
        name: "There".into(),
    })
    .event_from(there(), listing(sessions))
}
