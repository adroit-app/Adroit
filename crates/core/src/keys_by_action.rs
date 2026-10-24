//! The key bindings told action by action, as the Settings window offers
//! them to someone who knows what they want done but not which areas of the
//! window it works in: each action with the keys that run it, and changes
//! made to every area it works in at once.

use crate::actions::{ACTIONS, Action};
use crate::keymap::{Chord, Context, Error, Keymap};
use crate::keys_edit;

/// The headings actions are listed under, each with the names it takes: a
/// name ending in `.` takes every action it begins, any other only itself,
/// before any prefix.
const SECTIONS: &[(&str, &[&str])] = &[
    (
        "Sessions",
        &[
            "session.",
            "waiting.",
            "workspace.",
            "shell.",
            "neovim.",
            "terminal.",
            "refresh",
            "filter",
            "worktrees.",
        ],
    ),
    ("Tree", &["cursor."]),
    ("Files", &["filer."]),
    ("Tabs", &["tab."]),
    (
        "Application",
        &[
            "window.open",
            "window.close",
            "settings.",
            "keys.",
            "zoom.",
            "tree.toggle",
            "details.toggle",
            "daemon.",
        ],
    ),
    ("Windows", &["window."]),
    ("Prompt", &["prompt."]),
    ("Console", &["console."]),
    ("Details", &["details."]),
    ("Scrolling", &["scroll."]),
    ("Viewer", &["viewer."]),
];

/// Actions listed under one heading.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub title: &'static str,
    pub actions: Vec<ActionKeys>,
}

/// One action and the keys that run it.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionKeys {
    /// The action as the file of bindings writes it, an argument included.
    pub action: String,
    /// What it does, in a few words.
    pub label: String,
    /// What it does, in full, as plain text.
    pub does: String,
    /// The areas it works in: where adroit's bindings or the file bind it,
    /// or everywhere when neither does.
    pub areas: Vec<Context>,
    pub keys: Vec<Bound>,
    /// Whether its keys are other than adroit's own.
    pub changed: bool,
}

/// Keys bound to an action, and the areas they are bound in.
#[derive(Debug, Clone, PartialEq)]
pub struct Bound {
    pub keys: String,
    pub areas: Vec<Context>,
}

impl ActionKeys {
    /// Whether `bound` runs the action in every area it works in: bound in
    /// Global, or in each of them.
    pub fn everywhere(&self, bound: &Bound) -> bool {
        bound.areas.contains(&Context::Global) || self.areas.iter().all(|a| bound.areas.contains(a))
    }
}

/// Every action, heading by heading, with the keys `text`, the user's file,
/// binds to it over adroit's own.
pub fn sections(text: &str) -> Result<Vec<Section>, Error> {
    let now = Keymap::with_user(text)?.listed();
    let defaults = Keymap::defaults().listed();
    let mut actions: Vec<String> = ACTIONS
        .iter()
        .filter(|s| !matches!(s.action, Action::None | Action::TabGo(_)))
        .map(|s| s.name.to_string())
        .collect();
    for (_, _, action) in defaults.iter().chain(&now) {
        if !actions.contains(action) {
            actions.push(action.clone());
        }
    }
    // Each action follows the row of ACTIONS it is one of, so `tab.go 2`
    // comes among the tabs and after `tab.go 1`.
    let rank = |action: &String| {
        let parsed: Option<Action> = action.parse().ok();
        parsed.map_or(usize::MAX, |a| {
            ACTIONS
                .iter()
                .position(|s| std::mem::discriminant(&s.action) == std::mem::discriminant(&a))
                .unwrap_or(usize::MAX)
        })
    };
    actions.sort_by_key(|a| rank(a));
    let mut out: Vec<Section> = SECTIONS
        .iter()
        .map(|(title, _)| Section {
            title,
            actions: Vec::new(),
        })
        .collect();
    for action in actions {
        let Ok(parsed) = action.parse::<Action>() else {
            continue;
        };
        let at = section_of(&action);
        out[at]
            .actions
            .push(action_keys(&action, &parsed, &now, &defaults));
    }
    Ok(out)
}

/// Which of [`SECTIONS`] lists `action`: the one naming it, or else the one
/// whose prefix begins it.
fn section_of(action: &str) -> usize {
    let name = action.split(' ').next().unwrap_or(action);
    SECTIONS
        .iter()
        .position(|(_, names)| names.contains(&name))
        .or_else(|| {
            SECTIONS.iter().position(|(_, names)| {
                names
                    .iter()
                    .any(|n| n.ends_with('.') && name.starts_with(n))
            })
        })
        .unwrap_or(SECTIONS.len() - 1)
}

type Listed = [(&'static str, String, String)];

fn action_keys(action: &str, parsed: &Action, now: &Listed, defaults: &Listed) -> ActionKeys {
    let of = |listed: &Listed| -> Vec<(Context, String)> {
        listed
            .iter()
            .filter(|(_, _, a)| a == action)
            .map(|(area, keys, _)| (area_named(area), keys.clone()))
            .collect()
    };
    let (bound, shipped) = (of(now), of(defaults));
    let mut areas: Vec<Context> = Context::ALL
        .into_iter()
        .filter(|c| bound.iter().chain(&shipped).any(|(a, _)| a == c))
        .collect();
    if areas.is_empty() {
        areas.push(Context::Global);
    }
    let mut keys: Vec<Bound> = Vec::new();
    for (area, k) in &bound {
        match keys.iter_mut().find(|b| b.keys == *k) {
            Some(b) => b.areas.push(*area),
            None => keys.push(Bound {
                keys: k.clone(),
                areas: vec![*area],
            }),
        }
    }
    let sorted = |v: &[(Context, String)]| {
        let mut v = v.to_vec();
        v.sort_by(|a, b| (a.0.name(), &a.1).cmp(&(b.0.name(), &b.1)));
        v
    };
    let changed = sorted(&bound) != sorted(&shipped);
    ActionKeys {
        action: action.to_string(),
        label: parsed.label(),
        does: plain(parsed.spec().does),
        areas,
        keys,
        changed,
    }
}

fn area_named(name: &str) -> Context {
    Context::ALL
        .into_iter()
        .find(|c| c.name() == name)
        .expect("a listed area is an area")
}

/// `does` as plain text: a sentence, without the backticks and the links
/// to the specs the key reference has.
fn plain(does: &str) -> String {
    let mut out = String::new();
    let mut rest = does;
    while let Some(at) = rest.find(" ([") {
        out.push_str(&rest[..at]);
        rest = match rest[at..].find("))") {
            Some(end) => &rest[at + end + 2..],
            None => "",
        };
    }
    out.push_str(rest);
    let out = out.replace('`', "");
    let mut chars = out.chars();
    match chars.next() {
        Some(first) => format!("{}{}.", first.to_uppercase(), chars.as_str()),
        None => out,
    }
}

/// Whether `keys` begin with a key pressed without Ctrl, Alt or Command,
/// other than a function key: bound everywhere, such keys are taken from the
/// console and the tree, where they move and type.
pub fn types(keys: &str) -> bool {
    let first = keys.split(' ').find(|k| !k.is_empty()).unwrap_or_default();
    Chord::parse(first).is_ok_and(|c| {
        let key = c.key();
        let function =
            key.key.len() > 1 && key.key.starts_with('F') && key.key[1..].parse::<u8>().is_ok();
        !(key.ctrl || key.alt || key.meta || function)
    })
}

/// The actions `text`, the user's file, binds in `area` as it reads now.
fn bound_in(text: &str, area: Context) -> Result<Vec<(String, String)>, Error> {
    Ok(Keymap::with_user(text)?
        .listed()
        .into_iter()
        .filter(|(a, _, _)| *a == area.name())
        .map(|(_, k, act)| (k, act))
        .collect())
}

fn edit_error(e: keys_edit::Error) -> Error {
    Error(e.to_string())
}

/// `text` with `keys` running `action` in each of `areas`; a binding adroit
/// has already is left to it rather than written again.
pub fn add(text: &str, action: &str, keys: &str, areas: &[Context]) -> Result<String, Error> {
    let mut out = text.to_string();
    for area in areas {
        out = if keys_edit::default_of(area.name(), keys).as_deref() == Some(action) {
            keys_edit::reset(&out, area.name(), keys)
        } else {
            keys_edit::bind(&out, area.name(), keys, action)
        }
        .map_err(edit_error)?;
    }
    Keymap::with_user(&out)?;
    Ok(out)
}

/// `text` with `keys` no longer running `action` in any of `areas`; an area
/// where they run something else keeps it.
pub fn remove(text: &str, action: &str, keys: &str, areas: &[Context]) -> Result<String, Error> {
    let mut out = text.to_string();
    for area in areas {
        let runs = bound_in(&out, *area)?
            .into_iter()
            .any(|(k, a)| k == keys && a == action);
        if runs {
            out = keys_edit::unbind(&out, area.name(), keys).map_err(edit_error)?;
        }
    }
    Ok(out)
}

/// `text` with `old` taken from `action` in `old_areas` and `new` given to
/// it in `new_areas`, both or neither.
pub fn replace(
    text: &str,
    action: &str,
    (old, old_areas): (&str, &[Context]),
    (new, new_areas): (&str, &[Context]),
) -> Result<String, Error> {
    add(
        &remove(text, action, old, old_areas)?,
        action,
        new,
        new_areas,
    )
}

/// `text` with `action` run by adroit's own keys again: the file's own
/// bindings of it gone, and adroit's that the file took away or bound to
/// something else put back.
pub fn reset(text: &str, action: &str) -> Result<String, Error> {
    let mut out = text.to_string();
    for area in Context::ALL {
        for (keys, a) in bound_in(&out, area)? {
            if a == action {
                out = keys_edit::reset(&out, area.name(), &keys).map_err(edit_error)?;
            }
        }
    }
    for (area, keys, a) in Keymap::defaults().listed() {
        if a == action {
            out = keys_edit::reset(&out, area, &keys).map_err(edit_error)?;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(sections: &'a [Section], action: &str) -> (&'static str, &'a ActionKeys) {
        sections
            .iter()
            .find_map(|s| {
                s.actions
                    .iter()
                    .find(|a| a.action == action)
                    .map(|a| (s.title, a))
            })
            .unwrap_or_else(|| panic!("{action} is listed"))
    }

    fn keys_of(a: &ActionKeys) -> Vec<(&str, Vec<&str>)> {
        a.keys
            .iter()
            .map(|b| (b.keys.as_str(), b.areas.iter().map(|c| c.name()).collect()))
            .collect()
    }

    #[test]
    fn every_action_is_listed_once_under_a_heading_with_what_it_does() {
        let sections = sections("").unwrap();
        let all: Vec<&str> = sections
            .iter()
            .flat_map(|s| s.actions.iter().map(|a| a.action.as_str()))
            .collect();
        for name in Action::names() {
            assert_eq!(all.iter().filter(|a| **a == name).count(), 1, "{name}");
        }
        assert!(!all.contains(&"none"));
        let (title, down) = find(&sections, "cursor.down");
        assert_eq!(title, "Tree");
        assert_eq!(down.label, "Select the next row");
        assert!(down.does.starts_with("Move the selection"), "{}", down.does);
        assert!(!down.changed);
        assert!(
            sections.iter().all(|s| !s.actions.is_empty()),
            "{sections:#?}"
        );
    }

    #[test]
    fn headings_take_their_actions_by_name_before_prefix() {
        let sections = sections("").unwrap();
        let cases = [
            ("window.left", "Windows"),
            ("window.open", "Application"),
            ("window.close", "Application"),
            ("tree.toggle", "Application"),
            ("details.toggle", "Application"),
            ("details.log", "Details"),
            ("filter", "Sessions"),
            ("tab.go 3", "Tabs"),
            ("console.send Escape", "Console"),
        ];
        for (action, want) in cases {
            assert_eq!(find(&sections, action).0, want, "{action}");
        }
    }

    #[test]
    fn an_action_with_an_argument_follows_its_kind_in_order() {
        let sections = sections("").unwrap();
        let tabs: Vec<&str> = sections
            .iter()
            .find(|s| s.title == "Tabs")
            .unwrap()
            .actions
            .iter()
            .map(|a| a.action.as_str())
            .collect();
        let one = tabs.iter().position(|a| *a == "tab.go 1").unwrap();
        assert_eq!(tabs[one + 1], "tab.go 2");
        assert_eq!(find(&sections, "tab.go 2").1.label, "Go to tab 2");
    }

    #[test]
    fn keys_are_told_with_the_areas_they_run_the_action_in() {
        let sections = sections("").unwrap();
        let down = find(&sections, "cursor.down").1;
        assert_eq!(down.areas, [Context::Tree, Context::Details]);
        assert_eq!(
            keys_of(down),
            [
                ("j", vec!["tree", "details"]),
                ("ArrowDown", vec!["tree", "details"]),
            ]
        );
        assert!(down.everywhere(&down.keys[0]));
        let new = find(&sections, "session.new").1;
        assert_eq!(new.areas, [Context::Global, Context::Tree]);
        // Global covers every area; the tree alone does not.
        assert!(new.everywhere(&new.keys[0]));
        assert!(!new.everywhere(&new.keys[1]));
    }

    #[test]
    fn an_action_no_binding_names_works_everywhere() {
        let sections = sections("").unwrap();
        let scratch = find(&sections, "session.scratch").1;
        assert_eq!(scratch.areas, [Context::Global]);
        assert!(scratch.keys.is_empty());
    }

    #[test]
    fn what_the_file_changes_marks_the_action() {
        let sections = sections("[tree]\nJ = \"cursor.down\"\nk = \"none\"\n").unwrap();
        assert!(find(&sections, "cursor.down").1.changed);
        assert!(find(&sections, "cursor.up").1.changed);
        assert!(!find(&sections, "cursor.first").1.changed);
        assert_eq!(
            keys_of(find(&sections, "cursor.up").1),
            [("ArrowUp", vec!["tree", "details"]), ("k", vec!["details"])]
        );
    }

    #[test]
    fn plain_text_drops_backticks_and_links_to_the_specs() {
        assert_eq!(
            plain("open the picker ([details](tree.md#going)), to go to `x`"),
            "Open the picker, to go to x."
        );
    }

    #[test]
    fn keys_that_type_are_those_without_ctrl_alt_or_command_but_function_keys() {
        for keys in ["j", "G", "g g", "Enter", "Space", "Shift+Tab"] {
            assert!(types(keys), "{keys}");
        }
        for keys in [
            "Ctrl+q h", "Cmd+k", "Alt+x", "Mod+p", "F1", "F12", "Hyper+x",
        ] {
            assert!(!types(keys), "{keys}");
        }
    }

    #[test]
    fn a_key_added_runs_the_action_in_every_area_given() {
        let out = add(
            "# mine\n[tree]\n",
            "cursor.down",
            "J",
            &[Context::Tree, Context::Details],
        )
        .unwrap();
        assert!(out.starts_with("# mine\n"), "{out}");
        let down = find(&sections(&out).unwrap(), "cursor.down").1.clone();
        assert!(
            down.keys
                .iter()
                .any(|b| b.keys == "J" && down.everywhere(b))
        );
    }

    #[test]
    fn adding_one_of_adroit_s_own_bindings_writes_nothing() {
        let out = add("", "cursor.down", "j", &[Context::Tree]).unwrap();
        assert_eq!(out.trim(), "");
    }

    #[test]
    fn a_key_added_that_is_no_key_is_refused() {
        assert!(add("", "cursor.down", "Hyper+j", &[Context::Tree]).is_err());
    }

    #[test]
    fn a_key_removed_goes_from_each_area_it_runs_the_action_in() {
        let out = remove("", "cursor.down", "j", &[Context::Tree, Context::Details]).unwrap();
        let down = find(&sections(&out).unwrap(), "cursor.down").1.clone();
        assert_eq!(keys_of(&down), [("ArrowDown", vec!["tree", "details"])]);
        assert!(down.changed);
        // An area where the keys run something else keeps them.
        let out = remove("", "cursor.down", "j", &[Context::Viewer]).unwrap();
        assert_eq!(out.trim(), "");
    }

    #[test]
    fn a_key_replaced_moves_to_the_new_keys_and_areas() {
        let out = replace(
            "",
            "cursor.down",
            ("j", &[Context::Tree, Context::Details]),
            ("Ctrl+n", &[Context::Tree]),
        )
        .unwrap();
        let down = find(&sections(&out).unwrap(), "cursor.down").1.clone();
        assert_eq!(
            keys_of(&down),
            [
                ("ArrowDown", vec!["tree", "details"]),
                ("Ctrl+n", vec!["tree"]),
            ]
        );
    }

    #[test]
    fn a_reset_brings_back_adroit_s_keys_for_the_action_alone() {
        let file = "[tree]\nJ = \"cursor.down\"\nj = \"tab.close\"\nk = \"none\"\n[details]\nj = \"none\"\n";
        let out = reset(file, "cursor.down").unwrap();
        let sections = sections(&out).unwrap();
        assert!(!find(&sections, "cursor.down").1.changed, "{out}");
        assert!(find(&sections, "cursor.up").1.changed, "{out}");
    }
}
