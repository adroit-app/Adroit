//! Keys as the window's script reports them.

use adroit_term::Key;
use serde::Deserialize;

/// What the window's script tells, one JSON object a message tagged by its
/// `kind`.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Message {
    /// A key pressed, by the name the browser gives it.
    Key {
        key: String,
        #[serde(default)]
        ctrl: bool,
        #[serde(default)]
        alt: bool,
        #[serde(default)]
        shift: bool,
        #[serde(default)]
        meta: bool,
    },
    /// Text typed, an input method's included.
    Text {
        text: String,
    },
    Paste {
        text: String,
    },
}

impl Message {
    /// The key a key message tells of.
    pub fn key(&self) -> Option<Key> {
        match self {
            Message::Key {
                key,
                ctrl,
                alt,
                shift,
                meta,
            } => Some(Key {
                key: key.clone(),
                ctrl: *ctrl,
                alt: *alt,
                shift: *shift,
                meta: *meta,
            }),
            _ => None,
        }
    }
}

/// Whether the key is a modifier pressed on its own, which only changes the
/// keys that come with it.
pub fn held_alone(key: &Key) -> bool {
    matches!(
        key.key.as_str(),
        "Shift" | "Control" | "Alt" | "Meta" | "CapsLock" | "Fn"
    )
}

/// Whether a text field types or edits with `key` itself, rather than the
/// key being a command: Enter, Escape, Tab, the up and down arrows and any
/// held Control or Option are commands.
pub fn edits_text(key: &Key) -> bool {
    !key.ctrl
        && !key.alt
        && !key.meta
        && !matches!(
            key.key.as_str(),
            "Enter" | "Escape" | "Tab" | "ArrowUp" | "ArrowDown"
        )
}

/// A key pressed in a text field, or [`None`] while an input method composes
/// with it.
pub fn of_field(e: &dioxus::prelude::KeyboardData) -> Option<Key> {
    use dioxus::prelude::{Modifiers, ModifiersInteraction};
    if e.is_composing() {
        return None;
    }
    let held = e.modifiers();
    Some(Key {
        key: e.key().to_string(),
        ctrl: held.contains(Modifiers::CONTROL),
        alt: held.contains(Modifiers::ALT),
        shift: held.contains(Modifiers::SHIFT),
        meta: held.contains(Modifiers::META),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_modifier_pressed_alone_is_no_key_of_its_own() {
        for name in ["Shift", "Control", "Alt", "Meta", "CapsLock", "Fn"] {
            assert!(held_alone(&Key::new(name)), "{name}");
        }
        assert!(!held_alone(&Key::new("g")));
        assert!(!held_alone(&Key::new("c").ctrl()));
    }

    #[test]
    fn a_text_field_edits_with_plain_keys_but_for_the_ones_that_leave_or_move() {
        for edits in ["a", "あ", "Backspace", "Delete", "ArrowLeft", "Home", " "] {
            assert!(edits_text(&Key::new(edits)), "{edits}");
        }
        assert!(edits_text(&Key::new("A").shift()));
        for not in ["Enter", "Escape", "Tab", "ArrowUp", "ArrowDown"] {
            assert!(!edits_text(&Key::new(not)), "{not}");
        }
        assert!(!edits_text(&Key::new("n").ctrl()));
        assert!(!edits_text(&Key::new("f").alt()));
    }

    fn read(json: &str) -> Message {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_key_message_names_the_key_and_the_modifiers_held() {
        let m = read(r#"{"kind":"key","key":"Tab","ctrl":true,"shift":true}"#);
        assert_eq!(m.key(), Some(Key::new("Tab").ctrl().shift()));
        let m = read(r#"{"kind":"key","key":"|"}"#);
        assert_eq!(m.key(), Some(Key::new("|")));
    }

    #[test]
    fn text_and_pastes_carry_their_text() {
        assert_eq!(
            read(r#"{"kind":"text","text":"日本"}"#),
            Message::Text {
                text: "日本".into()
            }
        );
        assert_eq!(
            read(r#"{"kind":"paste","text":"a\nb"}"#),
            Message::Paste {
                text: "a\nb".into()
            }
        );
        assert_eq!(read(r#"{"kind":"text","text":"x"}"#).key(), None);
    }
}
