//! The directories the picker offers to start a session in, which the host
//! lists while a window shows the picker.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// What the picker shows that the host keeps read while it is open.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickSight {
    /// The directories whose entries the picker's directory tree lists.
    pub listed: BTreeSet<PathBuf>,
}

/// The directories offered, by where each was found, in the order each
/// source names them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offers {
    /// Those an agent has been used in.
    pub recent: Vec<PathBuf>,
    /// The repositories `ghq list` names.
    pub ghq: Vec<PathBuf>,
    /// The folders in the roots the settings list.
    pub listed: Vec<PathBuf>,
    /// Where the directory tree starts besides the home directory: ghq's
    /// roots, then those the settings list.
    pub roots: Vec<PathBuf>,
}

/// What the host listed for the picker.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PickWork {
    /// None until they are listed, and once the picker closes.
    pub offers: Option<Offers>,
}

/// One change to what the host listed for the picker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PickChange {
    Offers {
        offers: Offers,
    },
    /// The host no longer lists them.
    Left,
}

impl PickWork {
    /// What was listed with `change` made; whether that changed it.
    pub fn apply(&mut self, change: PickChange) -> bool {
        let now = match change {
            PickChange::Offers { offers } => Some(offers),
            PickChange::Left => None,
        };
        if self.offers == now {
            return false;
        }
        self.offers = now;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_replace_those_before_and_leaving_drops_them() {
        let mut work = PickWork::default();
        let offers = |dir: &str| PickChange::Offers {
            offers: Offers {
                recent: vec![dir.into()],
                ..Default::default()
            },
        };
        assert!(work.apply(offers("/a")));
        assert!(!work.apply(offers("/a")));
        assert!(work.apply(offers("/b")));
        assert_eq!(
            work.offers.as_ref().map(|o| o.recent.clone()),
            Some(vec!["/b".into()])
        );
        assert!(work.apply(PickChange::Left));
        assert!(!work.apply(PickChange::Left));
        assert_eq!(work.offers, None);
    }
}
