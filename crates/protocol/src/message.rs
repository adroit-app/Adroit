use serde::{Deserialize, Serialize};

use crate::{Sight, Version, WorkPatch, WorkSnapshot};

/// What the window asks of the work host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// The first thing a window says: the versions it speaks.
    Hello { versions: Vec<Version> },
    /// Send the whole work again, as after losing track of the patches.
    Snapshot,
    /// Keep read what the window now shows, in place of what it showed.
    Watch(Box<Sight>),
    /// Read again now what the window shows, as after the window changed it.
    ReadAgain,
}

/// What the work host tells a window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// The host works with the window in `version`.
    Welcome { version: Version },
    /// The host shares no version with the window; it speaks these.
    Refused { versions: Vec<Version> },
    /// The whole work, replacing what the window had.
    Snapshot(WorkSnapshot),
    /// A change to the work, in the order the host made it.
    Patch(WorkPatch),
}
