//! A session's name, or an environment's, being edited where it is shown.

use adroit_model::{EnvironmentId, Session};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Sidebar,
    Tab,
    /// An environment's heading in the tree, which names the environment
    /// itself.
    Heading,
}

/// A session or an environment being renamed, and the name typed for it so
/// far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rename {
    /// The environment the session runs in, or the one renamed.
    pub environment: EnvironmentId,
    /// The session's id; empty for an environment.
    pub id: String,
    /// What it is called now.
    pub was: String,
    pub text: String,
    pub place: Place,
}

impl Rename {
    /// Start renaming `session` of `environment`, with its name to change.
    pub fn of(environment: &EnvironmentId, session: &Session, place: Place) -> Self {
        Self {
            environment: environment.clone(),
            id: session.id.clone(),
            was: session.name.clone(),
            text: session.name.clone(),
            place,
        }
    }

    /// Start renaming `environment`, called `name` now, on its heading.
    pub fn of_environment(environment: &EnvironmentId, name: &str) -> Self {
        Self {
            environment: environment.clone(),
            id: String::new(),
            was: name.to_string(),
            text: name.to_string(),
            place: Place::Heading,
        }
    }

    pub fn is_for(&self, environment: &EnvironmentId, id: &str, place: Place) -> bool {
        self.environment == *environment && self.id == id && self.place == place
    }

    pub fn take(
        editing: &mut Option<Self>,
        environment: &EnvironmentId,
        id: &str,
        place: Place,
    ) -> Option<Self> {
        if editing
            .as_ref()
            .is_some_and(|r| r.is_for(environment, id, place))
        {
            editing.take()
        } else {
            None
        }
    }

    /// The new name typed. `None` when there is none.
    pub fn name(&self) -> Option<String> {
        let name = self.text.trim();
        (!name.is_empty() && name != self.was).then(|| name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_model::testing;

    fn session(name: &str) -> Session {
        testing::session("a").name(name).build()
    }

    fn here() -> EnvironmentId {
        EnvironmentId::new("here")
    }

    #[test]
    fn an_environment_is_renamed_on_its_heading_from_the_name_it_has() {
        let mut r = Rename::of_environment(&here(), "Work box");
        assert!(r.is_for(&here(), "", Place::Heading));
        assert!(!r.is_for(&here(), "", Place::Sidebar));
        assert_eq!(r.name(), None);
        r.text = " GPU box ".into();
        assert_eq!(r.name().as_deref(), Some("GPU box"));
    }

    #[test]
    fn only_the_name_and_surface_that_started_editing_hold_the_input() {
        let rename = Rename::of(&here(), &session("api"), Place::Tab);
        assert!(rename.is_for(&here(), "a", Place::Tab));
        assert!(!rename.is_for(&EnvironmentId::new("there"), "a", Place::Tab));
        assert!(!rename.is_for(&here(), "a", Place::Sidebar));
        assert!(!rename.is_for(&here(), "other", Place::Tab));
    }

    #[test]
    fn finishing_an_old_field_cannot_submit_or_cancel_a_new_one() {
        let mut editing = Some(Rename::of(&here(), &session("api"), Place::Tab));
        assert!(Rename::take(&mut editing, &here(), "a", Place::Sidebar).is_none());
        assert!(editing.is_some());
        assert!(Rename::take(&mut editing, &here(), "other", Place::Tab).is_none());
        assert!(Rename::take(&mut editing, &here(), "a", Place::Tab).is_some());
        assert!(Rename::take(&mut editing, &here(), "a", Place::Tab).is_none());
    }

    #[test]
    fn it_starts_from_the_name_shown_and_takes_the_one_typed() {
        let mut r = Rename::of(&here(), &session("api-fix"), Place::Sidebar);
        assert_eq!(r.text, "api-fix");
        r.text = " ログイン修正 ".into();
        assert_eq!(r.name().as_deref(), Some("ログイン修正"));
    }

    #[test]
    fn an_empty_or_unchanged_name_is_no_new_name() {
        let mut r = Rename::of(&here(), &session("api"), Place::Sidebar);
        assert_eq!(r.name(), None);
        r.text = " api ".into();
        assert_eq!(r.name(), None);
        r.text = "  ".into();
        assert_eq!(r.name(), None);
    }
}
