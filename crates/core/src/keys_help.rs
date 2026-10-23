//! What the keys help lists: every binding in force, area by area, the areas
//! the keys are in first, and what a key does while it is shown.

use adroit_model::Key;

use crate::actions::Action;
use crate::icons::Icon;
use crate::keymap::{Context, Keymap, Step};

/// One area's bindings.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub context: Context,
    /// Whether the keys are in the area now.
    pub here: bool,
    pub rows: Vec<Row>,
}

/// One action of an area, with every sequence bound to it there.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub keys: Vec<String>,
    pub label: String,
    pub icon: Option<Icon>,
}

/// The groups of `keymap`'s bindings, the areas of `here` first and in
/// their order, then every other area in the order the file names them. An
/// area that binds nothing is left out.
pub fn groups(keymap: &Keymap, here: &[Context]) -> Vec<Group> {
    let listed = keymap.listed();
    let rest = Context::ALL.into_iter().filter(|c| !here.contains(c));
    here.iter()
        .copied()
        .chain(rest)
        .filter_map(|ctx| {
            let mut rows: Vec<(Action, Row)> = Vec::new();
            for (area, keys, name) in &listed {
                let Ok(action) = name.parse::<Action>() else {
                    continue;
                };
                if *area != ctx.name() {
                    continue;
                }
                match rows.iter_mut().find(|(a, _)| *a == action) {
                    Some((_, row)) => row.keys.push(keys.clone()),
                    None => rows.push((
                        action.clone(),
                        Row {
                            keys: vec![keys.clone()],
                            label: action.label(),
                            icon: action.icon(),
                        },
                    )),
                }
            }
            (!rows.is_empty()).then(|| Group {
                context: ctx,
                here: here.contains(&ctx),
                rows: rows.into_iter().map(|(_, r)| r).collect(),
            })
        })
        .collect()
}

/// What a key does while the keys help is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Close,
    /// Scroll by this many lines, down when positive.
    Scroll(i32),
    Nothing,
}

/// What `key` does while the keys help is shown, opened from `contexts`.
pub fn press(keymap: &Keymap, contexts: &[Context], key: &Key) -> Press {
    let opens =
        |contexts: &[Context]| keymap.resolve(contexts, &[], key) == Step::Run(Action::KeysHelp);
    if opens(contexts) || opens(&[Context::Global]) {
        return Press::Close;
    }
    let plain = !key.ctrl && !key.alt && !key.meta;
    let ctrl = key.ctrl && !key.alt && !key.meta;
    match key.key.as_str() {
        "Escape" | "q" if plain => Press::Close,
        "j" | "ArrowDown" if plain => Press::Scroll(1),
        "k" | "ArrowUp" if plain => Press::Scroll(-1),
        "d" if ctrl => Press::Scroll(10),
        "u" if ctrl => Press::Scroll(-10),
        _ => Press::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Chord;

    fn key(chord: &str) -> Key {
        Chord::parse(chord).unwrap().key()
    }

    fn contexts(groups: &[Group]) -> Vec<Context> {
        groups.iter().map(|g| g.context).collect()
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
                Context::Details,
                Context::Console,
                Context::Prompt,
                Context::Draft,
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
        let map = Keymap::with_user("[draft]\n\"Ctrl+g a\" = \"none\"\n\"Ctrl+g A\" = \"none\"\n")
            .unwrap();
        assert!(!contexts(&groups(&map, &[])).contains(&Context::Draft));
    }

    #[test]
    fn keys_bound_to_one_action_in_an_area_share_its_row() {
        let groups = groups(&Keymap::defaults(), &[]);
        let down = row(&groups, Context::Tree, &Action::CursorDown.label()).unwrap();
        assert_eq!(down.keys, ["j", "ArrowDown"]);
    }

    #[test]
    fn a_row_says_what_its_action_does_with_its_icon() {
        let groups = groups(&Keymap::defaults(), &[]);
        let settings = row(&groups, Context::Global, "Settings…").unwrap();
        assert_eq!(settings.icon, Some(Icon::Settings));
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
        assert_eq!(down.keys, ["ArrowDown", "J"]);
    }

    #[test]
    fn a_sequence_is_written_as_the_file_writes_it() {
        let groups = groups(&Keymap::defaults(), &[]);
        let first = row(&groups, Context::Tree, &Action::CursorFirst.label()).unwrap();
        assert_eq!(first.keys, ["g g"]);
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
}
