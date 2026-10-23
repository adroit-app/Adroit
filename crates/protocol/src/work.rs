use serde::{Deserialize, Serialize};

/// Where an event stands in the order the host made its changes: each patch
/// is one past the one before it, and a snapshot carries the number of the
/// last change it holds.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Seq(pub u64);

impl Seq {
    /// The number of the change after this one.
    pub fn next(self) -> Seq {
        Seq(self.0 + 1)
    }
}

/// Everything the host holds of the work, as of `seq`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkSnapshot {
    pub seq: Seq,
}

/// A change to the work since the patch numbered one less.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkPatch {
    pub seq: Seq,
    pub changes: Vec<Change>,
}

/// One change a patch makes to the work. Each fact of the work the host
/// takes over adds its variants here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Change {}
