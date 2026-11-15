use super::*;
use crate::keymap::Chord;

fn key(chord: &str) -> Key {
    Chord::parse(chord).unwrap().key()
}

fn contexts(groups: &[Group]) -> Vec<Context> {
    groups.iter().map(|g| g.context).collect()
}

fn alone(key: &str) -> Keys {
    Keys {
        lead: Vec::new(),
        ends: vec![key.into()],
    }
}

fn after(lead: &[&str], ends: &[&str]) -> Keys {
    Keys {
        lead: lead.iter().map(|k| k.to_string()).collect(),
        ends: ends.iter().map(|k| k.to_string()).collect(),
    }
}

/// How `Mod+n` is written on this platform.
fn with_mod(n: &str) -> String {
    Chord::parse(&format!("Mod+{n}")).unwrap().to_string()
}

fn row<'a>(groups: &'a [Group], ctx: Context, label: &str) -> Option<&'a Row> {
    groups
        .iter()
        .find(|g| g.context == ctx)?
        .rows
        .iter()
        .find(|r| r.label == label)
}

#[test]
fn the_areas_the_keys_are_in_come_first_in_their_order() {
    let groups = groups(&Keymap::defaults(), &[Context::Viewer, Context::Global]);
    assert_eq!(contexts(&groups)[..2], [Context::Viewer, Context::Global]);
}

#[test]
fn every_other_area_follows_in_the_order_the_file_names_them() {
    let groups = groups(&Keymap::defaults(), &[Context::Viewer]);
    assert_eq!(
        contexts(&groups),
        [
            Context::Viewer,
            Context::Global,
            Context::Tree,
            Context::Filer,
            Context::Details,
            Context::Console,
            Context::Prompt,
            Context::Draft,
            Context::Lightbox,
            Context::Overlay,
            Context::Confirm,
            Context::Agent(adroit_model::Agent::Claude),
        ]
    );
}

#[test]
fn only_the_areas_the_keys_are_in_are_marked_here() {
    let groups = groups(&Keymap::defaults(), &[Context::Tree, Context::Global]);
    let here: Vec<Context> = groups
        .iter()
        .filter(|g| g.here)
        .map(|g| g.context)
        .collect();
    assert_eq!(here, [Context::Tree, Context::Global]);
}

#[test]
fn an_area_that_binds_nothing_is_left_out() {
    let map = Keymap::with_user(
        "[draft]\n\"Ctrl+g a\" = \"none\"\n\"Ctrl+g A\" = \"none\"\n\"Ctrl+g p\" = \"none\"\n\"Ctrl+g P\" = \"none\"\n",
    )
        .unwrap();
    assert!(!contexts(&groups(&map, &[])).contains(&Context::Draft));
}

#[test]
fn keys_bound_to_one_action_in_an_area_share_its_row() {
    let groups = groups(&Keymap::defaults(), &[]);
    let down = row(&groups, Context::Tree, &Action::CursorDown.label()).unwrap();
    assert_eq!(down.keys, [alone("j"), alone("ArrowDown")]);
}

#[test]
fn a_row_says_what_its_action_does_with_its_icon() {
    let groups = groups(&Keymap::defaults(), &[]);
    let settings = row(&groups, Context::Global, "Settings…").unwrap();
    assert_eq!(settings.icon, Some(Icon::Settings));
}

#[test]
fn a_row_says_whether_its_last_key_repeats_it() {
    let groups = groups(&Keymap::defaults(), &[]);
    let next = row(&groups, Context::Global, &Action::TabNext.label()).unwrap();
    assert!(next.repeats);
    let split = row(&groups, Context::Global, &Action::WindowSplit.label()).unwrap();
    assert!(!split.repeats);
    let numbered = format!("{}–9", Action::TabGo(1).label());
    assert!(!row(&groups, Context::Global, &numbered).unwrap().repeats);
}

#[test]
fn every_action_taking_a_tab_s_number_folds_its_numbers_into_a_row_of_its_own() {
    let groups = groups(&Keymap::defaults(), &[]);
    let into = format!("{}–9", Action::TabMerge(1).label());
    let into = row(&groups, Context::Global, &into).unwrap();
    assert_eq!(into.keys.len(), 1, "{:?}", into.keys);
    let go = format!("{}–9", Action::TabGo(1).label());
    assert!(row(&groups, Context::Global, &go).is_some());
    assert!(row(&groups, Context::Global, &Action::TabMerge(3).label()).is_none());
}

#[test]
fn the_rows_keep_the_order_the_file_binds_them_in() {
    let groups = groups(&Keymap::defaults(), &[]);
    let tree = &groups
        .iter()
        .find(|g| g.context == Context::Tree)
        .unwrap()
        .rows;
    assert_eq!(tree[0].label, Action::CursorDown.label());
    assert_eq!(tree[1].label, Action::CursorUp.label());
}

#[test]
fn the_user_s_bindings_are_listed_in_place_of_the_defaults() {
    let map = Keymap::with_user("[tree]\n\"j\" = \"none\"\n\"J\" = \"cursor.down\"\n").unwrap();
    let groups = groups(&map, &[]);
    let down = row(&groups, Context::Tree, &Action::CursorDown.label()).unwrap();
    assert_eq!(down.keys, [alone("ArrowDown"), alone("J")]);
}

#[test]
fn a_sequence_is_written_as_the_file_writes_it() {
    let groups = groups(&Keymap::defaults(), &[]);
    let first = row(&groups, Context::Tree, &Action::CursorFirst.label()).unwrap();
    assert_eq!(first.keys, [after(&["g"], &["g"])]);
}

#[test]
fn keys_that_start_alike_write_what_they_share_once() {
    let groups = groups(&Keymap::defaults(), &[]);
    let left = row(&groups, Context::Global, &Action::WindowLeft.label()).unwrap();
    assert_eq!(left.keys, [after(&["Ctrl+q"], &["h", "ArrowLeft"])]);
}

#[test]
fn keys_that_start_differently_stand_apart_in_the_order_of_the_file() {
    let groups = groups(&Keymap::defaults(), &[]);
    let next = row(&groups, Context::Global, &Action::TabNext.label()).unwrap();
    assert_eq!(
        next.keys,
        [
            alone("Ctrl+Tab"),
            alone("Ctrl+PageDown"),
            alone("Cmd+}"),
            after(&["Ctrl+q", "g"], &["t"]),
        ]
    );
}

#[test]
fn a_numbered_action_bound_over_a_run_of_numbers_takes_one_row() {
    let groups = groups(&Keymap::defaults(), &[]);
    let go = row(&groups, Context::Global, "Go to tab 1–9").unwrap();
    assert_eq!(
        go.keys,
        [
            after(&["Ctrl+q"], &["1–9"]),
            alone(&format!("{}–9", with_mod("1"))),
        ]
    );
    assert!(row(&groups, Context::Global, &Action::TabGo(1).label()).is_none());
}

#[test]
fn the_folded_row_stands_where_its_first_number_is_bound() {
    let groups = groups(&Keymap::defaults(), &[]);
    let global = &groups
        .iter()
        .find(|g| g.context == Context::Global)
        .unwrap()
        .rows;
    let at = |label: &str| global.iter().position(|r| r.label == label).unwrap();
    assert_eq!(at("Go to tab 1–9"), at(&Action::TabPrev.label()) + 1);
}

#[test]
fn a_gap_in_the_numbers_splits_the_run() {
    let map = Keymap::with_user("[global]\n\"Ctrl+q 5\" = \"none\"\n").unwrap();
    let groups = groups(&map, &[]);
    let go = row(&groups, Context::Global, "Go to tab 1–9").unwrap();
    assert_eq!(go.keys[0], after(&["Ctrl+q"], &["1–4", "6–9"]));
}

#[test]
fn a_number_bound_on_its_own_keeps_a_row_of_its_own() {
    let map = Keymap::with_user("[global]\n\"Alt+x\" = \"tab.go 3\"\n").unwrap();
    let groups = groups(&map, &[]);
    let three = row(&groups, Context::Global, &Action::TabGo(3).label()).unwrap();
    assert_eq!(three.keys, [alone("Alt+x")]);
    assert!(row(&groups, Context::Global, "Go to tab 1–9").is_some());
}

fn opens(map: &Keymap, contexts: &[Context], chord: &str) -> bool {
    map.resolve(contexts, &[], &key(chord)) == Step::Run(Action::KeysHelp)
}

#[test]
fn f1_opens_the_keys_help_wherever_the_keys_are() {
    let map = Keymap::defaults();
    // A console passing its keys on reads only `global`; the prompt in
    // Neovim's insert mode only `prompt`.
    for contexts in [
        &[Context::Global][..],
        &[Context::Prompt],
        &[Context::Tree, Context::Global],
        &[Context::Details, Context::Tree, Context::Global],
        &[Context::Console, Context::Global],
        &[Context::Viewer, Context::Global],
        &[Context::Draft, Context::Prompt],
    ] {
        assert!(opens(&map, contexts, "F1"), "{contexts:?}");
    }
}

#[test]
fn a_question_mark_is_left_to_the_area_it_is_pressed_in() {
    let map = Keymap::defaults();
    // A live console's reader searches backward on it.
    for contexts in [
        &[Context::Global][..],
        &[Context::Prompt],
        &[Context::Tree, Context::Global],
        &[Context::Details, Context::Tree, Context::Global],
        &[Context::Console, Context::Global],
        &[Context::Viewer, Context::Global],
        &[Context::Draft, Context::Prompt],
    ] {
        assert!(!opens(&map, contexts, "?"), "{contexts:?}");
    }
}

#[test]
fn escape_q_and_f1_close_it() {
    let map = Keymap::defaults();
    let here = [Context::Tree, Context::Global];
    for chord in ["Escape", "q", "F1"] {
        assert_eq!(press(&map, &here, &key(chord)), Press::Close, "{chord}");
    }
}

#[test]
fn a_key_the_user_opens_it_with_closes_it() {
    let map = Keymap::with_user("[global]\n\"Mod+/\" = \"keys.help\"\n").unwrap();
    let here = [Context::Console, Context::Global];
    assert_eq!(press(&map, &here, &key("Mod+/")), Press::Close);
}

#[test]
fn j_k_and_the_arrows_scroll_it() {
    let map = Keymap::defaults();
    assert_eq!(press(&map, &[], &key("j")), Press::Scroll(1));
    assert_eq!(press(&map, &[], &key("ArrowDown")), Press::Scroll(1));
    assert_eq!(press(&map, &[], &key("k")), Press::Scroll(-1));
    assert_eq!(press(&map, &[], &key("ArrowUp")), Press::Scroll(-1));
    assert_eq!(press(&map, &[], &key("Ctrl+d")), Press::Scroll(10));
    assert_eq!(press(&map, &[], &key("Ctrl+u")), Press::Scroll(-10));
}

#[test]
fn any_other_key_does_nothing_while_it_is_shown() {
    let map = Keymap::defaults();
    let here = [Context::Tree, Context::Global];
    assert_eq!(press(&map, &here, &key("x")), Press::Nothing);
    assert_eq!(press(&map, &here, &key("Ctrl+q")), Press::Nothing);
}
