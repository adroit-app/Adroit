use adroit_term::Key;

/// The key in Neovim's notation for `nvim_input`, or `None` for keys Neovim
/// never sees, such as Command shortcuts and bare modifiers.
pub fn notation(key: &Key) -> Option<String> {
    if key.meta {
        return None;
    }
    let mut chars = key.key.chars();
    let single = match (chars.next(), chars.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    };
    let name: String = match (single, key.key.as_str()) {
        (Some('<'), _) => "lt".into(),
        (Some(' '), _) => "Space".into(),
        (Some('\\'), _) if key.ctrl || key.alt => "Bslash".into(),
        (Some(c), _) => {
            // Shift is already in the character itself.
            if key.ctrl || key.alt {
                c.to_string()
            } else {
                return Some(c.to_string());
            }
        }
        (None, "Enter") => "CR".into(),
        (None, "Escape") => "Esc".into(),
        (None, "Backspace") => "BS".into(),
        (None, "Tab") => "Tab".into(),
        (None, "ArrowUp") => "Up".into(),
        (None, "ArrowDown") => "Down".into(),
        (None, "ArrowLeft") => "Left".into(),
        (None, "ArrowRight") => "Right".into(),
        (None, "Home") => "Home".into(),
        (None, "End") => "End".into(),
        (None, "PageUp") => "PageUp".into(),
        (None, "PageDown") => "PageDown".into(),
        (None, "Delete") => "Del".into(),
        (None, "Insert") => "Insert".into(),
        (None, f) if f.starts_with('F') && f[1..].parse::<u8>().is_ok() => f.into(),
        _ => return None,
    };
    let mut mods = String::new();
    if key.ctrl {
        mods.push_str("C-");
    }
    if key.alt {
        mods.push_str("M-");
    }
    if key.shift && single.is_none() {
        mods.push_str("S-");
    }
    if mods.is_empty() && name.chars().count() == 1 {
        return Some(name);
    }
    Some(format!("<{mods}{name}>"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(k: Key) -> Option<String> {
        notation(&k)
    }

    #[test]
    fn a_plain_character_is_itself() {
        assert_eq!(n(Key::new("a")).as_deref(), Some("a"));
        assert_eq!(n(Key::new("A").shift()).as_deref(), Some("A"));
        assert_eq!(n(Key::new("あ")).as_deref(), Some("あ"));
    }

    #[test]
    fn less_than_is_escaped_so_it_does_not_start_a_key_name() {
        assert_eq!(n(Key::new("<")).as_deref(), Some("<lt>"));
    }

    #[test]
    fn a_bare_space_is_named() {
        assert_eq!(n(Key::new(" ")).as_deref(), Some("<Space>"));
    }

    #[test]
    fn modifiers_wrap_the_key() {
        assert_eq!(n(Key::new("a").ctrl()).as_deref(), Some("<C-a>"));
        assert_eq!(n(Key::new("x").alt()).as_deref(), Some("<M-x>"));
        assert_eq!(n(Key::new("Enter").ctrl()).as_deref(), Some("<C-CR>"));
        assert_eq!(n(Key::new("Tab").shift()).as_deref(), Some("<S-Tab>"));
    }

    #[test]
    fn special_keys_use_their_vim_names() {
        assert_eq!(n(Key::new("Enter")).as_deref(), Some("<CR>"));
        assert_eq!(n(Key::new("Escape")).as_deref(), Some("<Esc>"));
        assert_eq!(n(Key::new("Backspace")).as_deref(), Some("<BS>"));
        assert_eq!(n(Key::new("ArrowLeft")).as_deref(), Some("<Left>"));
        assert_eq!(n(Key::new("Delete")).as_deref(), Some("<Del>"));
        assert_eq!(n(Key::new("F5")).as_deref(), Some("<F5>"));
    }

    #[test]
    fn command_shortcuts_and_bare_modifiers_are_not_sent() {
        assert_eq!(n(Key::new("v").meta()), None);
        assert_eq!(n(Key::new("Shift")), None);
    }
}
