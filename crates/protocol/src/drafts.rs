//! What is written in a window's prompt for each session or draft, which the
//! work host keeps from one run to the next: each window sends what it
//! changed, so windows writing for different sessions keep each other's.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// What is written in the prompts, and the images attached, by what it is
/// written for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drafts {
    pub text: BTreeMap<String, String>,
    pub images: BTreeMap<String, Vec<PathBuf>>,
}

/// What a window changed of the drafts: each one written for anew, or
/// `None` where nothing is written for it any longer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftChanges {
    pub text: BTreeMap<String, Option<String>>,
    pub images: BTreeMap<String, Option<Vec<PathBuf>>>,
}

impl DraftChanges {
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.images.is_empty()
    }
}

impl Drafts {
    /// What changed from `before` to these.
    pub fn since(&self, before: &Drafts) -> DraftChanges {
        DraftChanges {
            text: changed(&before.text, &self.text),
            images: changed(&before.images, &self.images),
        }
    }

    /// These with `changes` made, leaving what they do not name as it is.
    pub fn apply(&mut self, changes: DraftChanges) {
        made(&mut self.text, changes.text);
        made(&mut self.images, changes.images);
    }
}

fn changed<T: Clone + PartialEq>(
    before: &BTreeMap<String, T>,
    now: &BTreeMap<String, T>,
) -> BTreeMap<String, Option<T>> {
    let gone = before
        .keys()
        .filter(|k| !now.contains_key(*k))
        .map(|k| (k.clone(), None));
    let new = now
        .iter()
        .filter(|(k, v)| before.get(*k) != Some(*v))
        .map(|(k, v)| (k.clone(), Some(v.clone())));
    gone.chain(new).collect()
}

fn made<T>(kept: &mut BTreeMap<String, T>, changes: BTreeMap<String, Option<T>>) {
    for (key, value) in changes {
        match value {
            Some(value) => kept.insert(key, value),
            None => kept.remove(&key),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drafts(text: &[(&str, &str)]) -> Drafts {
        Drafts {
            text: text
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            images: BTreeMap::new(),
        }
    }

    #[test]
    fn the_drafts_asked_and_kept_cross_the_wire_as_they_are() {
        use crate::{Answer, Ask, decode, encode};
        let changes = drafts(&[("session:a", "fix")]).since(&Drafts::default());
        let ask = Ask::KeepDrafts(Box::new(changes));
        assert_eq!(decode::<Ask>(encode(&ask)).unwrap(), ask);
        let answer = Answer::Drafts(Box::new(drafts(&[("draft:/w", "new")])));
        assert_eq!(decode::<Answer>(encode(&answer)).unwrap(), answer);
    }

    #[test]
    fn a_window_sends_only_what_it_changed() {
        let before = drafts(&[("session:a", "fix"), ("session:b", "test")]);
        let now = drafts(&[("session:a", "fix it"), ("session:c", "new")]);
        let changes = now.since(&before);
        assert_eq!(
            changes.text,
            BTreeMap::from([
                ("session:a".into(), Some("fix it".into())),
                ("session:b".into(), None),
                ("session:c".into(), Some("new".into())),
            ])
        );
        assert!(now.since(&now).is_empty());
    }

    #[test]
    fn what_one_window_changed_leaves_what_another_wrote_for_another_session() {
        let mut kept = drafts(&[("session:a", "from the first"), ("session:b", "old")]);
        let seen_by_second = drafts(&[("session:b", "old")]);
        let written_by_second = drafts(&[("session:b", "from the second")]);
        kept.apply(written_by_second.since(&seen_by_second));
        assert_eq!(
            kept,
            drafts(&[
                ("session:a", "from the first"),
                ("session:b", "from the second")
            ])
        );
    }
}
