//! What the picker going to a session holds while it is open: its list, the
//! directory tree beside it, and what each host offered to list.

use std::collections::BTreeMap;
use std::path::PathBuf;

use adroit_model::EnvironmentId;
use adroit_protocol::Offers;

use super::Choice;
use super::filer::{Filer, Row};
use super::list::ListState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    List,
    Tree,
}

/// What choosing a row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Purpose {
    /// Go to the session, or start one in the directory.
    #[default]
    Go,
    /// Add the directory to the tree; only directories are offered.
    Add,
}

/// What the picker holds while it is open.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GoPicker {
    pub list: ListState,
    pub mode: Mode,
    pub purpose: Purpose,
    pub filer: Filer,
    /// What was typed, where the cursor was and the row it was on when the
    /// rows were last drawn.
    pub(crate) held: Option<(ListState, Choice)>,
    /// The directories the tree listed when the picker opened, by the
    /// environment they are in.
    pub running: BTreeMap<EnvironmentId, Vec<PathBuf>>,
    /// What each environment's host offered, as the list ranks it; none
    /// from a host until it is listed.
    pub offered: BTreeMap<EnvironmentId, Offers>,
    /// The directory opened before it was listed, which the cursor moves
    /// into once it is.
    pub(crate) entering: Option<Row>,
    /// The environment the directory tree browses, and the scratch row
    /// starts in.
    pub environment: EnvironmentId,
    /// The home directory of that environment's host, where the tree starts.
    pub home: PathBuf,
}

impl GoPicker {
    pub fn new(filer: Filer) -> Self {
        Self {
            filer,
            ..Self::default()
        }
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            Mode::List => Mode::Tree,
            Mode::Tree => Mode::List,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picker_opens_on_the_list_and_toggles_to_the_tree_and_back() {
        let mut p = GoPicker::new(Filer::new(["/w"]));
        assert_eq!(p.mode, Mode::List);
        p.toggle_mode();
        assert_eq!(p.mode, Mode::Tree);
        p.toggle_mode();
        assert_eq!(p.mode, Mode::List);
    }
}
