//! The dialog that gives a session a new name.

use adroit_model::Session;

/// A session being renamed, and the name typed for it so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rename {
    pub id: String,
    /// What the session is called now.
    pub was: String,
    pub text: String,
}

impl Rename {
    /// Start renaming `session`, with its name to change.
    pub fn of(session: &Session) -> Self {
        Self {
            id: session.id.clone(),
            was: session.name.clone(),
            text: session.name.clone(),
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

    fn session(name: &str) -> Session {
        Session {
            id: "a".into(),
            name: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn it_starts_from_the_name_shown_and_takes_the_one_typed() {
        let mut r = Rename::of(&session("api-fix"));
        assert_eq!(r.text, "api-fix");
        r.text = " ログイン修正 ".into();
        assert_eq!(r.name().as_deref(), Some("ログイン修正"));
    }

    #[test]
    fn an_empty_or_unchanged_name_is_no_new_name() {
        let mut r = Rename::of(&session("api"));
        assert_eq!(r.name(), None);
        r.text = " api ".into();
        assert_eq!(r.name(), None);
        r.text = "  ".into();
        assert_eq!(r.name(), None);
    }
}
