use super::*;
use crate::picker::filer::Filer;
use crate::picker::resume::Resume;
use crate::picker::{GoPicker, Purpose};

fn shown(hints: &[Hint]) -> Vec<(&str, &str)> {
    hints.iter().map(|h| (h.keys.as_str(), h.does)).collect()
}

fn go(purpose: Purpose, mode: Mode) -> Picking {
    let mut picker = GoPicker {
        purpose,
        ..GoPicker::new(Filer::new(["/home/me"]))
    };
    if mode == Mode::Tree {
        picker.toggle_mode();
    }
    Picking::Go(Box::new(picker))
}

#[test]
fn the_picker_going_to_a_session_hints_at_the_keys_bound_over_the_window_and_its_own() {
    let hints = picker(&Keymap::defaults(), &go(Purpose::Go, Mode::List));
    assert_eq!(
        shown(&hints),
        [
            ("Ctrl+n/p", "move"),
            ("Enter", "choose"),
            ("Ctrl+s", "shell"),
            ("Ctrl+e", "Neovim"),
            ("Ctrl+t", "add to tree"),
            ("Ctrl+f", "file tree"),
            ("Escape", "close"),
        ]
    );
}

#[test]
fn the_picker_adding_to_the_tree_adds_what_is_chosen_and_offers_no_adding_beside() {
    let hints = picker(&Keymap::defaults(), &go(Purpose::Add, Mode::List));
    assert!(shown(&hints).contains(&("Enter", "add")));
    assert!(!hints.iter().any(|h| h.does == "add to tree"));
}

#[test]
fn the_picker_s_file_tree_hints_at_the_filer_s_keys() {
    let hints = picker(&Keymap::defaults(), &go(Purpose::Go, Mode::Tree));
    let shown = shown(&hints);
    for hint in [
        ("j", "down"),
        ("k", "up"),
        ("l", "open"),
        ("h", "close"),
        ("!", "hidden"),
        ("Enter", "choose"),
        ("Ctrl+f", "list"),
        ("Escape", "close"),
    ] {
        assert!(shown.contains(&hint), "{hint:?} in {shown:?}");
    }
}

#[test]
fn the_list_of_past_conversations_hints_at_widening_it_to_every_directory() {
    let hints = picker(&Keymap::defaults(), &Picking::Resume(Resume::default()));
    assert_eq!(
        shown(&hints),
        [
            ("Ctrl+n/p", "move"),
            ("Enter", "resume"),
            ("Ctrl+a", "this directory or every one"),
            ("Escape", "close"),
        ]
    );
}

#[test]
fn the_keys_help_hints_at_scrolling_and_closing() {
    assert_eq!(
        shown(&keys_help(&Keymap::defaults())),
        [("Ctrl+n/p", "scroll"), ("Escape", "close")]
    );
}

#[test]
fn a_question_s_buttons_show_its_answering_keys() {
    assert_eq!(
        answers(&Keymap::defaults()),
        (Some("n".to_string()), Some("y".to_string()))
    );
}

#[test]
fn a_hint_shows_the_keys_rebound_in_place_of_adroit_s() {
    let text =
        "[overlay]\n\"Escape\" = \"none\"\n\"Ctrl+n\" = \"none\"\n\n[confirm]\n\"n\" = \"none\"\n";
    let keymap = Keymap::with_user(text).unwrap();
    assert_eq!(
        shown(&keys_help(&keymap)),
        [("Ctrl+j/p", "scroll"), ("Ctrl+c", "close")]
    );
    assert_eq!(answers(&keymap).0, Some("Ctrl+c".to_string()));
}

#[test]
fn keys_alike_but_their_last_character_share_what_comes_before_it() {
    assert_eq!(pair("Ctrl+n", "Ctrl+p"), "Ctrl+n/p");
    assert_eq!(pair("j", "k"), "j/k");
    assert_eq!(pair("ArrowDown", "ArrowUp"), "ArrowDown/ArrowUp");
    assert_eq!(pair("Ctrl+g", "Alt+p"), "Ctrl+g/Alt+p");
}
