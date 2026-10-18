use felis_protocol::messages::{
    FKey, Key as FelisKey, KeyEvent, KeyEventKind, KeyLocation, KeyMods, NamedKey,
};

/// A key press as the webview reports it: `key` is the DOM `KeyboardEvent.key`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Key {
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl Key {
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_string(),
            ..Self::default()
        }
    }

    pub fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    pub fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    pub fn meta(mut self) -> Self {
        self.meta = true;
        self
    }

    /// The key as felis takes it, for the daemon to send as the program's
    /// modes ask; `None` for keys that never reach the program, Command
    /// shortcuts and bare modifiers.
    pub(crate) fn event(&self) -> Option<KeyEvent> {
        if self.meta {
            return None;
        }
        let single = self.key.chars().count() == 1;
        let key = match self.key.as_str() {
            " " => FelisKey::Named(NamedKey::Space),
            k if single => FelisKey::Character(k.to_string()),
            k => FelisKey::Named(named(k)?),
        };
        let mut mods = KeyMods::empty();
        // Typed text comes as the character alone, a capital carrying its Shift.
        let capital = single && self.key.chars().all(char::is_uppercase);
        for (on, m) in [
            (self.ctrl, KeyMods::CTRL),
            (self.alt, KeyMods::ALT),
            (self.shift || capital, KeyMods::SHIFT),
        ] {
            if on {
                mods |= m;
            }
        }
        Some(KeyEvent {
            key,
            text: (single && !self.ctrl).then(|| self.key.clone()),
            mods,
            kind: KeyEventKind::Press,
            location: KeyLocation::Standard,
        })
    }
}

/// The name the webview gives a key that is not a character, as felis
/// knows it.
fn named(key: &str) -> Option<NamedKey> {
    Some(match key {
        "Enter" => NamedKey::Enter,
        "Tab" => NamedKey::Tab,
        "Escape" => NamedKey::Escape,
        "Backspace" => NamedKey::Backspace,
        "Insert" => NamedKey::Insert,
        "Delete" => NamedKey::Delete,
        "Home" => NamedKey::Home,
        "End" => NamedKey::End,
        "PageUp" => NamedKey::PageUp,
        "PageDown" => NamedKey::PageDown,
        "ArrowUp" => NamedKey::ArrowUp,
        "ArrowDown" => NamedKey::ArrowDown,
        "ArrowLeft" => NamedKey::ArrowLeft,
        "ArrowRight" => NamedKey::ArrowRight,
        f => NamedKey::F(FKey::new(f.strip_prefix('F')?.parse().ok()?)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_letter_goes_as_its_character_and_its_text() {
        let e = Key::new("a").event().unwrap();
        assert_eq!(e.key, FelisKey::Character("a".into()));
        assert_eq!(e.text.as_deref(), Some("a"));
        assert_eq!(e.mods, KeyMods::empty());
    }

    #[test]
    fn a_capital_carries_its_shift() {
        assert_eq!(Key::new("A").event().unwrap().mods, KeyMods::SHIFT);
    }

    #[test]
    fn ctrl_with_a_letter_has_no_text_of_its_own() {
        let e = Key::new("c").ctrl().event().unwrap();
        assert_eq!(e.mods, KeyMods::CTRL);
        assert_eq!(e.text, None);
    }

    #[test]
    fn space_and_named_keys_go_by_their_names() {
        let space = Key::new(" ").event().unwrap();
        assert_eq!(space.key, FelisKey::Named(NamedKey::Space));
        assert_eq!(
            Key::new("ArrowUp").event().unwrap().key,
            FelisKey::Named(NamedKey::ArrowUp)
        );
        assert_eq!(
            Key::new("F5").event().unwrap().key,
            FelisKey::Named(NamedKey::F(FKey::new(5).unwrap()))
        );
    }

    #[test]
    fn command_shortcuts_and_bare_modifiers_never_reach_the_program() {
        assert_eq!(Key::new("c").meta().event(), None);
        assert_eq!(Key::new("Shift").event(), None);
        assert_eq!(Key::new("Control").event(), None);
    }
}
