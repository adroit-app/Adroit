//! The link to each environment a window shows, each its own: one link
//! losing its host leaves the others as they are.

use std::collections::BTreeMap;

use adroit_model::EnvironmentId;

/// The link to the window's own environment, and to each it shows beside it.
#[derive(Clone)]
pub(crate) struct Links<L> {
    own: EnvironmentId,
    each: BTreeMap<EnvironmentId, L>,
}

impl<L> Links<L> {
    /// The links of a window opened in `own`, reached through `link`.
    pub(crate) fn new(own: EnvironmentId, link: L) -> Self {
        Self {
            each: BTreeMap::from([(own.clone(), link)]),
            own,
        }
    }

    /// The window's own environment.
    pub(crate) fn own_environment(&self) -> &EnvironmentId {
        &self.own
    }

    /// The link to the window's own environment.
    pub(crate) fn own(&self) -> &L {
        &self.each[&self.own]
    }

    /// The link to `environment`, none for one the window does not show.
    pub(crate) fn get(&self, environment: &EnvironmentId) -> Option<&L> {
        self.each.get(environment)
    }

    /// The link to `environment`, or the window's own for one it does not
    /// show.
    pub(crate) fn of(&self, environment: &EnvironmentId) -> &L {
        self.get(environment).unwrap_or_else(|| self.own())
    }

    pub(crate) fn shows(&self, environment: &EnvironmentId) -> bool {
        self.each.contains_key(environment)
    }

    /// The link `open` makes to `environment`, kept, when the window does
    /// not show it yet; none when it does, and `open` is not called.
    pub(crate) fn add(
        &mut self,
        environment: EnvironmentId,
        open: impl FnOnce() -> L,
    ) -> Option<&L> {
        if self.shows(&environment) {
            return None;
        }
        Some(self.each.entry(environment).or_insert_with(open))
    }

    /// The link to `environment` dropped, one shown beside the window's
    /// own: the window's own link stays.
    pub(crate) fn remove(&mut self, environment: &EnvironmentId) -> Option<L> {
        if *environment == self.own {
            return None;
        }
        self.each.remove(environment)
    }

    /// Every environment shown with its link, in the order of their ids.
    pub(crate) fn each(&self) -> impl Iterator<Item = (&EnvironmentId, &L)> {
        self.each.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_beside_the_own_one_is_dropped_and_the_own_one_never_is() {
        let (own, there) = (EnvironmentId::new("own"), EnvironmentId::new("there"));
        let mut links = Links::new(own.clone(), "own link");
        links.add(there.clone(), || "there link");
        assert_eq!(links.remove(&own), None);
        assert_eq!(links.remove(&there), Some("there link"));
        assert!(!links.shows(&there));
        assert!(links.shows(&own));
        assert_eq!(links.own_environment(), &own);
    }

    #[test]
    fn an_environment_already_shown_is_never_linked_twice() {
        let mut links = Links::new(EnvironmentId::new("own"), "own link");
        assert_eq!(
            links.add(EnvironmentId::new("there"), || "first link"),
            Some(&"first link")
        );
        let again = links.add(EnvironmentId::new("there"), || {
            panic!("a second link was opened")
        });
        assert_eq!(again, None);
        assert_eq!(
            links.add(EnvironmentId::new("own"), || panic!("own linked twice")),
            None
        );
        assert_eq!(links.get(&EnvironmentId::new("there")), Some(&"first link"));
    }

    #[test]
    fn an_environment_not_shown_is_asked_through_the_window_s_own_link() {
        let mut links = Links::new(EnvironmentId::new("own"), "own link");
        links.add(EnvironmentId::new("there"), || "there link");
        assert_eq!(*links.of(&EnvironmentId::new("there")), "there link");
        assert_eq!(*links.of(&EnvironmentId::new("elsewhere")), "own link");
        assert_eq!(links.get(&EnvironmentId::new("elsewhere")), None);
    }
}
