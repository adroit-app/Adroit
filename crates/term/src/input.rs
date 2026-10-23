use felis_protocol::messages::{
    FKey, Key as FelisKey, KeyEvent, KeyEventKind, KeyLocation, KeyMods, NamedKey,
};

pub use adroit_model::Key;

/// What is sent to a session's program without a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// Text as if it were typed, byte for byte.
    Text(String),
    /// Text pasted, bracketed when the program asks for it.
    Paste(String),
    Key(Key),
}

/// A key press as felis takes it.
pub(crate) trait Event {
    /// The key as felis takes it, for the daemon to send as the program's
    /// modes ask; `None` for keys that never reach the program, Command
    /// shortcuts and bare modifiers.
    fn event(&self) -> Option<KeyEvent>;
}

impl Event for Key {
    fn event(&self) -> Option<KeyEvent> {
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
