//! The conversations the agents kept, which the host lists while a window
//! shows the list of them.

use std::path::PathBuf;

use adroit_model::Past;
use serde::{Deserialize, Serialize};

/// Which conversations a window lists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PastSight {
    /// Those that ran in this directory; every directory's when none.
    pub dir: Option<PathBuf>,
}

/// What the host listed of the conversations the agents kept.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PastWork {
    /// The conversations listed, the most recently written first, and what
    /// they were listed for; none until they are listed.
    pub listed: Option<(PastSight, Vec<Past>)>,
}

/// One change to what the host listed of the conversations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PastChange {
    Listed {
        sight: PastSight,
        conversations: Vec<Past>,
    },
    /// The host no longer lists them.
    Left,
}

impl PastWork {
    /// The conversations with `change` made; whether that changed them.
    pub fn apply(&mut self, change: PastChange) -> bool {
        let now = match change {
            PastChange::Listed {
                sight,
                conversations,
            } => Some((sight, conversations)),
            PastChange::Left => None,
        };
        if self.listed == now {
            return false;
        }
        self.listed = now;
        true
    }

    /// The conversations listed for `sight`; none while they are not.
    pub fn of(&self, sight: &PastSight) -> Option<&[Past]> {
        self.listed
            .as_ref()
            .filter(|(of, _)| of == sight)
            .map(|(_, pasts)| pasts.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use adroit_model::Agent;

    use super::*;

    fn past(id: &str) -> Past {
        Past {
            agent: Agent::Codex,
            id: id.into(),
            cwd: "/r".into(),
            title: "fix".into(),
            at_ms: 1,
        }
    }

    #[test]
    fn a_listing_replaces_the_one_before_and_leaving_drops_it() {
        let mut work = PastWork::default();
        let here = PastSight {
            dir: Some("/r".into()),
        };
        let listed = |conversations| PastChange::Listed {
            sight: here.clone(),
            conversations,
        };
        assert!(work.apply(listed(vec![past("a")])));
        assert!(!work.apply(listed(vec![past("a")])));
        assert!(work.apply(listed(vec![past("b")])));
        assert_eq!(work.of(&here).map(<[Past]>::len), Some(1));
        assert_eq!(work.of(&PastSight::default()), None);
        assert!(work.apply(PastChange::Left));
        assert!(!work.apply(PastChange::Left));
        assert_eq!(work.of(&here), None);
    }
}
