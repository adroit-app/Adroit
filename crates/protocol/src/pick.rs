//! The directories the picker offers to start a session in, which the host
//! lists while a window shows the picker.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::work::set;

/// What the picker shows that the host keeps read while it is open.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickSight {
    /// The directories whose entries the picker's directory tree lists.
    pub listed: BTreeSet<PathBuf>,
    /// The owners on GitHub whose repositories the picker offers to clone.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub owners: BTreeSet<String>,
    /// Whether the picker offers the owners `gh` is logged in as: the user
    /// and the organizations they belong to.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub accounts: bool,
}

/// The directories offered, by where each was found, in the order each
/// source names them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offers {
    /// Those an agent has been used in.
    pub recent: Vec<PathBuf>,
    /// The repositories `ghq list` names.
    pub ghq: Vec<PathBuf>,
    /// Whether ghq clones a repository typed in the picker here.
    #[serde(default)]
    pub clones: bool,
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
    /// The names of each owner's repositories on GitHub, by the owner asked
    /// for; none of an owner until they are listed, and none once the picker
    /// closes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub owned: BTreeMap<String, Vec<String>>,
    /// The user `gh` is logged in as, then the organizations they belong to;
    /// none until they are listed, and once the picker closes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accounts: Option<Vec<String>>,
}

/// One change to what the host listed for the picker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PickChange {
    Offers {
        offers: Offers,
    },
    /// The repositories of `owner` on GitHub.
    Owned {
        owner: String,
        repositories: Vec<String>,
    },
    /// The owners `gh` is logged in as.
    Accounts {
        owners: Vec<String>,
    },
    /// The host no longer lists them.
    Left,
}

impl PickWork {
    /// What was listed with `change` made; whether that changed it.
    pub fn apply(&mut self, change: PickChange) -> bool {
        match change {
            PickChange::Offers { offers } => set(&mut self.offers, Some(offers)),
            PickChange::Owned {
                owner,
                repositories,
            } => {
                let was = self.owned.insert(owner, repositories.clone());
                was.as_ref() != Some(&repositories)
            }
            PickChange::Accounts { owners } => set(&mut self.accounts, Some(owners)),
            PickChange::Left => {
                let owned = !self.owned.is_empty();
                self.owned.clear();
                let accounts = set(&mut self.accounts, None);
                set(&mut self.offers, None) | owned | accounts
            }
        }
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

    #[test]
    fn an_owners_repositories_are_kept_by_owner_until_the_picker_closes() {
        let mut work = PickWork::default();
        let owned = |owner: &str, names: &[&str]| PickChange::Owned {
            owner: owner.into(),
            repositories: names.iter().map(|n| n.to_string()).collect(),
        };
        assert!(work.apply(owned("o", &["a", "b"])));
        assert!(!work.apply(owned("o", &["a", "b"])));
        assert!(work.apply(owned("p", &["c"])));
        assert_eq!(
            work.owned.get("o").map(Vec::as_slice),
            Some(&["a".to_string(), "b".to_string()][..])
        );
        assert!(work.apply(PickChange::Left));
        assert!(work.owned.is_empty());
    }

    #[test]
    fn the_owners_gh_logs_in_as_are_kept_until_the_picker_closes() {
        let mut work = PickWork::default();
        let accounts = PickChange::Accounts {
            owners: vec!["me".into(), "org".into()],
        };
        assert!(work.apply(accounts.clone()));
        assert!(!work.apply(accounts));
        assert_eq!(work.accounts, Some(vec!["me".into(), "org".into()]));
        assert!(work.apply(PickChange::Left));
        assert_eq!(work.accounts, None);
    }
}
