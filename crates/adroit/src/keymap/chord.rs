//! One key with its modifiers, as a file of bindings writes it and as the
//! window reports it.

use std::fmt;

use adroit_term::Key;

use super::Error;

/// The keys that are not one character, by the names the window reports them.
const NAMED: &[&str] = &[
    "Enter",
    "Escape",
    "Tab",
    "Backspace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
];

/// Keys that stay with the menu with Command held: copy, paste and the rest
/// of editing, quitting, hiding and minimizing.
pub const MENU_KEYS: &[&str] = &["c", "v", "x", "a", "z", "q", "h", "m"];

fn menu_key(key: &str) -> bool {
    MENU_KEYS.contains(&key.to_lowercase().as_str())
}

/// Whether `Mod` is Command, as on macOS, rather than Ctrl with Shift, which
/// terminal applications elsewhere take for themselves.
const MOD_IS_CMD: bool = cfg!(target_os = "macos");

/// One key with the modifiers held for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    key: String,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
}

impl Chord {
    pub fn parse(text: &str) -> Result<Self, Error> {
        Self::parse_on(text, MOD_IS_CMD)
    }

    /// `text` read with `Mod` as Command, or as Ctrl with Shift unless
    /// `mod_is_cmd`.
    fn parse_on(text: &str, mod_is_cmd: bool) -> Result<Self, Error> {
        let bad = || Error(format!("{text:?} is not a key"));
        let (mods, name) = match text.rsplit_once('+') {
            Some((mods, name)) => (mods, name),
            None => ("", text),
        };
        let mut c = Chord {
            key: String::new(),
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
        };
        let mut held_mod = false;
        for m in mods.split('+').filter(|_| !mods.is_empty()) {
            match m {
                "Mod" if mod_is_cmd => {
                    held_mod = true;
                    c.meta = true;
                }
                "Mod" => {
                    held_mod = true;
                    c.ctrl = true;
                    c.shift = true;
                }
                "Ctrl" => c.ctrl = true,
                "Alt" => c.alt = true,
                "Shift" => c.shift = true,
                "Cmd" => c.meta = true,
                _ => return Err(bad()),
            }
        }
        c.key = match name {
            "Space" => " ".into(),
            "Plus" => "+".into(),
            n if n.chars().count() == 1 => n.into(),
            n if NAMED.contains(&n) => n.into(),
            n if n
                .strip_prefix('F')
                .is_some_and(|d| d.parse::<u8>().is_ok_and(|d| (1..=12).contains(&d))) =>
            {
                n.into()
            }
            _ => return Err(bad()),
        };
        // A character carries its own case; Shift with a letter is its
        // capital.
        if c.printable() && c.shift {
            if c.key.chars().all(char::is_alphabetic) {
                c.key = c.key.to_uppercase();
                c.shift = false;
            } else if !c.ctrl {
                // Shift with another character types a third one, written as
                // itself: Shift and 1 is !. Held with Ctrl as well, as `Mod`
                // is off macOS, Shift stays for the key to need.
                return Err(bad());
            }
        }
        if (c.meta || held_mod) && menu_key(&c.key) {
            return Err(Error(format!("{text:?} stays with the menu")));
        }
        Ok(c)
    }

    /// A key pressed in the window, by the name it reports and its
    /// modifiers, written as the file writes it; `None` for a modifier on
    /// its own and for Command with a key the menu keeps.
    pub fn pressed(key: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> Option<String> {
        if (meta && menu_key(key))
            || matches!(
                key,
                "Control" | "Alt" | "Shift" | "Meta" | "CapsLock" | "Dead" | "Process"
            )
        {
            return None;
        }
        let name = match key {
            " " => "Space".to_string(),
            "+" => "Plus".to_string(),
            k => k.to_string(),
        };
        // A character carries its own case, so Shift is already in it.
        let character = key.chars().count() == 1 && key != " ";
        let mut text = String::new();
        for (on, m) in [
            (meta, "Cmd+"),
            (ctrl, "Ctrl+"),
            (alt, "Alt+"),
            (shift && !character, "Shift+"),
        ] {
            if on {
                text.push_str(m);
            }
        }
        text.push_str(&name);
        Chord::parse(&text).ok().map(|c| c.to_string())
    }

    /// Whether `key` is held with `Mod`.
    pub fn held_mod(key: &Key) -> bool {
        Self::held_mod_on(key, MOD_IS_CMD)
    }

    fn held_mod_on(key: &Key, mod_is_cmd: bool) -> bool {
        if mod_is_cmd {
            key.meta
        } else {
            key.ctrl && key.shift
        }
    }

    fn printable(&self) -> bool {
        self.key.chars().count() == 1 && self.key != " "
    }

    pub fn matches(&self, key: &Key) -> bool {
        key.key == self.key
            && key.ctrl == self.ctrl
            && key.alt == self.alt
            && key.meta == self.meta
            && ((self.printable() && !self.shift) || key.shift == self.shift)
    }

    /// The key this chord stands for, as the window would report it.
    pub fn key(&self) -> Key {
        let shifted = self.printable() && self.key.chars().any(char::is_uppercase);
        Key {
            key: self.key.clone(),
            ctrl: self.ctrl,
            alt: self.alt,
            shift: self.shift || shifted,
            meta: self.meta,
        }
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.meta, "Cmd+"),
            (self.ctrl, "Ctrl+"),
            (self.alt, "Alt+"),
            (self.shift, "Shift+"),
        ] {
            if on {
                f.write_str(name)?;
            }
        }
        match self.key.as_str() {
            " " => f.write_str("Space"),
            "+" => f.write_str("Plus"),
            k => f.write_str(k),
        }
    }
}

pub(super) fn parse_sequence(text: &str) -> Result<Vec<Chord>, Error> {
    let chords: Vec<Chord> = text
        .split(' ')
        .filter(|t| !t.is_empty())
        .map(Chord::parse)
        .collect::<Result<_, _>>()?;
    if chords.is_empty() {
        return Err(Error(format!("{text:?} is not a key")));
    }
    Ok(chords)
}

pub(super) fn show_sequence(chords: &[Chord]) -> String {
    chords
        .iter()
        .map(Chord::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chord_names_its_modifiers_before_its_key() {
        let c = Chord::parse("Ctrl+Shift+Tab").unwrap();
        assert_eq!(c.key(), Key::new("Tab").ctrl().shift());
        assert_eq!(
            Chord::parse("Ctrl+Alt+x").unwrap().key(),
            Key::new("x").ctrl().alt()
        );
    }

    #[test]
    fn a_pressed_key_is_written_as_the_file_writes_it() {
        let c = |k: &str, ctrl, alt, shift, meta| Chord::pressed(k, ctrl, alt, shift, meta);
        assert_eq!(c("w", true, false, false, false), Some("Ctrl+w".into()));
        // A character carries its case, and Shift is in it.
        assert_eq!(c("G", false, false, true, false), Some("G".into()));
        assert_eq!(
            c("Tab", false, false, true, false),
            Some("Shift+Tab".into())
        );
        assert_eq!(c(" ", false, false, false, false), Some("Space".into()));
        assert_eq!(c("+", false, false, true, false), Some("Plus".into()));
        // A modifier alone is no key yet; Command with what the menu uses
        // stays with the menu.
        assert_eq!(c("Control", true, false, false, false), None);
        assert_eq!(c("j", false, false, false, true), Some("Cmd+j".into()));
        assert_eq!(c("c", false, false, false, true), None);
        assert_eq!(c("w", false, false, false, true), Some("Cmd+w".into()));
    }

    #[test]
    fn command_keys_the_menu_uses_cannot_be_bound() {
        for taken in ["Cmd+c", "Cmd+v", "Cmd+q", "Cmd+Z", "Cmd+h", "Cmd+m"] {
            assert!(Chord::parse(taken).is_err(), "{taken}");
        }
        for free in [
            "Cmd+w", "Cmd+W", "Cmd+b", "Cmd+0", "Cmd+,", "Cmd+Plus", "Cmd+-",
        ] {
            assert!(Chord::parse(free).is_ok(), "{free}");
        }
        let chord = Chord::parse("Cmd+N").unwrap();
        assert!(chord.matches(&Key::new("N").shift().meta()));
        assert!(!chord.matches(&Key::new("N").shift()));
        assert_eq!(chord.to_string(), "Cmd+N");
    }

    #[test]
    fn mod_is_command_on_macos_and_ctrl_with_shift_elsewhere() {
        let mac = |t: &str| Chord::parse_on(t, true);
        let other = |t: &str| Chord::parse_on(t, false);
        assert_eq!(mac("Mod+p"), mac("Cmd+p"));
        assert_eq!(other("Mod+p").unwrap().key(), Key::new("P").ctrl().shift());
        assert_eq!(other("Mod+p"), other("Ctrl+P"));
        // Another character is typed with Shift held, which it then needs.
        let bracket = other("Mod+]").unwrap();
        assert!(bracket.matches(&Key::new("]").ctrl().shift()));
        assert!(!bracket.matches(&Key::new("]").ctrl()));
        assert_eq!(bracket.to_string(), "Ctrl+Shift+]");
        assert_eq!(other("Ctrl+Shift+]"), Ok(bracket));
        assert!(Chord::held_mod_on(&Key::new("p").meta(), true));
        assert!(!Chord::held_mod_on(&Key::new("p").meta(), false));
        assert!(Chord::held_mod_on(&Key::new("P").ctrl().shift(), false));
        assert!(!Chord::held_mod_on(&Key::new("p").ctrl(), false));
        // The menu's keys stay with it, whichever keys Mod is.
        assert!(mac("Mod+c").is_err());
        assert!(other("Mod+c").is_err());
    }

    #[test]
    fn shift_with_a_character_other_than_a_letter_is_refused() {
        // Shift and 1 types !, which is written as itself.
        assert!(Chord::parse("Shift+1").is_err());
        assert_eq!(Chord::parse("Shift+g").unwrap(), Chord::parse("G").unwrap());
    }

    #[test]
    fn a_letter_s_case_is_part_of_it() {
        assert!(Chord::parse("G").unwrap().matches(&Key::new("G").shift()));
        assert!(!Chord::parse("G").unwrap().matches(&Key::new("g")));
        assert!(!Chord::parse("g").unwrap().matches(&Key::new("G").shift()));
    }

    #[test]
    fn a_named_key_needs_its_modifiers_exactly() {
        assert!(
            Chord::parse("Shift+Tab")
                .unwrap()
                .matches(&Key::new("Tab").shift())
        );
        assert!(
            !Chord::parse("Tab")
                .unwrap()
                .matches(&Key::new("Tab").shift())
        );
        assert!(!Chord::parse("Ctrl+w").unwrap().matches(&Key::new("w")));
    }

    #[test]
    fn space_and_plus_have_names_of_their_own() {
        assert_eq!(Chord::parse("Space").unwrap().key(), Key::new(" "));
        assert_eq!(
            Chord::parse("Ctrl+Plus").unwrap().key(),
            Key::new("+").ctrl()
        );
    }

    #[test]
    fn a_chord_that_names_nothing_is_refused() {
        for bad in ["", "Ctrl+", "Hyper+x", "Ctrl++"] {
            assert!(Chord::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_chord_reads_back_as_written() {
        for s in ["Ctrl+w", "G", "Shift+Tab", "Space", "Ctrl+Alt+x"] {
            assert_eq!(Chord::parse(s).unwrap().to_string(), s);
        }
    }
}
