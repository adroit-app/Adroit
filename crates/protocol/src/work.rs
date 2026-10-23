use serde::{Deserialize, Serialize};

use crate::{
    DetailsChange, DetailsWork, PastChange, PastWork, PickChange, PickWork, PlaceChange, PlaceWork,
    PromptChange, PromptWork, TreeChange, TreeWork,
};

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
    pub details: DetailsWork,
    pub tree: TreeWork,
    pub past: PastWork,
    pub pick: PickWork,
    pub place: PlaceWork,
    pub prompt: PromptWork,
}

/// A change to the work since the patch numbered one less.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkPatch {
    pub seq: Seq,
    pub changes: Vec<Change>,
}

/// One change a patch makes to the work, by the part of the work it is in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Details(DetailsChange),
    Tree(TreeChange),
    Past(PastChange),
    Pick(PickChange),
    Place(PlaceChange),
    Prompt(PromptChange),
}

impl WorkSnapshot {
    /// The work with `change` made; whether that changed it.
    pub fn apply(&mut self, change: Change) -> bool {
        match change {
            Change::Details(change) => self.details.apply(change),
            Change::Tree(change) => self.tree.apply(change),
            Change::Past(change) => self.past.apply(change),
            Change::Pick(change) => self.pick.apply(change),
            Change::Place(change) => self.place.apply(change),
            Change::Prompt(change) => self.prompt.apply(change),
        }
    }
}

impl From<DetailsChange> for Change {
    fn from(change: DetailsChange) -> Self {
        Change::Details(change)
    }
}

impl From<TreeChange> for Change {
    fn from(change: TreeChange) -> Self {
        Change::Tree(change)
    }
}

impl From<PastChange> for Change {
    fn from(change: PastChange) -> Self {
        Change::Past(change)
    }
}

impl From<PlaceChange> for Change {
    fn from(change: PlaceChange) -> Self {
        Change::Place(change)
    }
}

impl From<PromptChange> for Change {
    fn from(change: PromptChange) -> Self {
        Change::Prompt(change)
    }
}

impl From<PickChange> for Change {
    fn from(change: PickChange) -> Self {
        Change::Pick(change)
    }
}
