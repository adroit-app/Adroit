//! Which environment a piece of a window's work belongs to: the same session
//! id, shell or path in two environments is two different things.

use std::fmt;
use std::sync::Arc;

/// An environment as the window knows it: by the id of the environment its
/// binding names. Every row and tab carries one, so it is shared rather than
/// copied.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EnvironmentId(Arc<str>);

impl EnvironmentId {
    pub fn new(id: impl AsRef<str>) -> Self {
        Self(id.as_ref().into())
    }

    /// The environment of a window opened without a binding, which reaches
    /// its execution target directly. Its work is what the environment
    /// imported from that instance later keeps, so it has no id of its own
    /// and is written down as work was before environments.
    pub fn unbound() -> Self {
        Self::new("")
    }

    pub fn is_unbound(&self) -> bool {
        self.0.is_empty()
    }
}

impl Default for EnvironmentId {
    fn default() -> Self {
        Self::unbound()
    }
}

impl fmt::Display for EnvironmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
