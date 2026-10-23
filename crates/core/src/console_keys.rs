//! What reaches a session while the console passes every key on to it.

use adroit_model::Key;

/// The key the session gets for `key`: `None` for the `Escape` that stops
/// passing keys on, and `Escape` itself for `Ctrl-c`.
pub fn direct_key(key: &Key) -> Option<Key> {
    match key.key.as_str() {
        "Escape" => None,
        "c" if key.ctrl => Some(Key::new("Escape")),
        _ => Some(key.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passing_every_key_on_ends_with_escape_and_ctrl_c_still_sends_escape() {
        assert_eq!(direct_key(&Key::new("Escape")), None);
        assert_eq!(direct_key(&Key::new("c").ctrl()), Some(Key::new("Escape")));
        assert_eq!(
            direct_key(&Key::new("w").ctrl()),
            Some(Key::new("w").ctrl())
        );
        assert_eq!(direct_key(&Key::new("x")), Some(Key::new("x")));
    }
}
