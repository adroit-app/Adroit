//! Changing the user's key bindings file in place, as the Settings window
//! does: what else the file holds, comments included, is left as it was.

use toml_edit::{DocumentMut, Item, Table, value};

use crate::keymap::{Chord, Context, Keymap};

pub type Error = toml_edit::TomlError;

fn area_table<'a>(doc: &'a mut DocumentMut, area: &str) -> &'a mut Table {
    if !doc.contains_table(area) {
        doc[area] = Item::Table(Table::new());
    }
    doc[area].as_table_mut().expect("an area is a table")
}

fn chords(keys: &str) -> Option<Vec<Chord>> {
    keys.split(' ')
        .filter(|k| !k.is_empty())
        .map(|k| Chord::parse(k).ok())
        .collect()
}

/// Whether `a` and `b` are the same keys, however each is spelled: the file
/// may write `Shift+g` where the window shows `G`.
fn same_keys(a: &str, b: &str) -> bool {
    a == b || chords(a).is_some_and(|c| chords(b) == Some(c))
}

/// The keys of `table` that are `keys`, under the spellings it has.
fn spellings(table: &Table, keys: &str) -> Vec<String> {
    table
        .iter()
        .map(|(k, _)| k)
        .filter(|k| same_keys(k, keys))
        .map(str::to_string)
        .collect()
}

/// `text` with `keys` in `area` bound to `action`, under the spelling the
/// file already has for them.
pub fn bind(text: &str, area: &str, keys: &str, action: &str) -> Result<String, Error> {
    let mut doc: DocumentMut = text.parse()?;
    let table = area_table(&mut doc, area);
    let mut found = spellings(table, keys).into_iter();
    let kept = found.next().unwrap_or_else(|| keys.to_string());
    for other in found {
        table.remove(&other);
    }
    table[kept.as_str()] = value(action);
    Ok(doc.to_string())
}

/// `text` with `keys` in `area` doing nothing: taken out of the file when
/// only the file bound them, bound to `none` over a default binding.
pub fn unbind(text: &str, area: &str, keys: &str) -> Result<String, Error> {
    if default_of(area, keys).is_some() {
        return bind(text, area, keys, "none");
    }
    reset(text, area, keys)
}

/// `text` without its own binding of `keys` in `area`, so the default, if
/// any, is in force again.
pub fn reset(text: &str, area: &str, keys: &str) -> Result<String, Error> {
    let mut doc: DocumentMut = text.parse()?;
    if let Some(t) = doc.get_mut(area).and_then(Item::as_table_mut) {
        for k in spellings(t, keys) {
            t.remove(&k);
        }
    }
    Ok(doc.to_string())
}

/// `text` without any binding of its own, everything else kept.
pub fn reset_all(text: &str) -> Result<String, Error> {
    let mut doc: DocumentMut = text.parse()?;
    // The tables are emptied, not removed: a comment over one is kept with it.
    for ctx in Context::ALL {
        if let Some(t) = doc.get_mut(ctx.name()).and_then(Item::as_table_mut) {
            t.clear();
        }
    }
    Ok(doc.to_string())
}

/// Whether `keys`, trimmed, can be bound to `action` in `area`: some are
/// given, and a keymap takes the binding.
pub fn bindable(area: &str, keys: &str, action: &str) -> bool {
    !keys.is_empty()
        && bind("", area, keys, action)
            .ok()
            .is_some_and(|t| Keymap::with_user(&t).is_ok())
}

/// `text` with the binding of `was` in `area`, or a new one when there was
/// none, made `keys` running `action`: `was` let go of when the keys
/// change, and a binding the defaults make anyway left to them.
pub fn rebind(
    text: &str,
    area: &str,
    was: Option<&str>,
    keys: &str,
    action: &str,
) -> Result<String, Error> {
    let cleared = match was {
        Some(old) if old != keys => unbind(text, area, old)?,
        _ => text.to_string(),
    };
    if default_of(area, keys).as_deref() == Some(action) {
        reset(&cleared, area, keys)
    } else {
        bind(&cleared, area, keys, action)
    }
}

/// The action the defaults bind `keys` to in `area`.
pub fn default_of(area: &str, keys: &str) -> Option<String> {
    Keymap::defaults()
        .listed()
        .into_iter()
        .find(|(a, k, _)| *a == area && same_keys(k, keys))
        .map(|(_, _, act)| act)
}

/// The bindings the user's `text` takes away from the defaults: area and
/// keys.
pub fn removed(text: &str) -> Vec<(&'static str, String)> {
    let Ok(table) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    Context::ALL
        .into_iter()
        .flat_map(|ctx| {
            let entries = table
                .get(ctx.name())
                .and_then(|t| t.as_table())
                .cloned()
                .unwrap_or_default();
            entries
                .into_iter()
                .filter(|(_, a)| a.as_str() == Some("none"))
                .map(move |(k, _)| (ctx.name(), k))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "# my keys\n\n[tree]\n# down twice\n\"J\" = \"cursor.down\"\n";

    #[test]
    fn a_binding_is_added_and_the_file_s_comments_stay() {
        let out = bind(FILE, "tree", "K", "cursor.up").unwrap();
        assert!(out.starts_with("# my keys\n"), "{out}");
        assert!(
            out.contains("# down twice\n\"J\" = \"cursor.down\""),
            "{out}"
        );
        assert!(out.contains("K = \"cursor.up\""), "{out}");
        let fresh = bind("", "console", "Ctrl+x", "tab.close").unwrap();
        assert_eq!(
            Keymap::with_user(&fresh)
                .unwrap()
                .keys_for(Context::Console, &"tab.close".parse().unwrap()),
            Some("Ctrl+x".into())
        );
    }

    #[test]
    fn taking_away_a_default_binds_it_to_none_and_a_user_s_own_leaves_the_file() {
        let out = unbind(FILE, "tree", "j").unwrap();
        assert!(out.contains("j = \"none\""), "{out}");
        assert_eq!(removed(&out), vec![("tree", "j".to_string())]);
        let out = unbind(FILE, "tree", "J").unwrap();
        assert!(!out.contains("\"J\""), "{out}");
        assert!(out.starts_with("# my keys\n"), "{out}");
    }

    #[test]
    fn a_reset_brings_the_default_back() {
        let changed = bind(FILE, "tree", "j", "tab.close").unwrap();
        let out = reset(&changed, "tree", "j").unwrap();
        let map = Keymap::with_user(&out).unwrap();
        assert_eq!(
            map.keys_for(Context::Tree, &"cursor.down".parse().unwrap()),
            Some("j".into())
        );
        let all = reset_all(&changed).unwrap();
        assert!(all.starts_with("# my keys\n"), "{all}");
        assert_eq!(Keymap::with_user(&all).unwrap(), Keymap::defaults());
    }

    #[test]
    fn keys_the_file_spells_another_way_are_found_by_the_spelling_shown() {
        let file =
            "[tree]\n# bottom\n\"Shift+g\" = \"cursor.down\"\n\"Alt+Ctrl+x\" = \"tab.close\"\n";
        let out = reset(file, "tree", "G").unwrap();
        assert!(!out.contains("Shift+g"), "{out}");
        let out = bind(file, "tree", "Ctrl+Alt+x", "cursor.up").unwrap();
        assert!(out.contains("# bottom\n"), "{out}");
        assert!(out.contains("\"Alt+Ctrl+x\" = \"cursor.up\""), "{out}");
        assert!(!out.contains("\"Ctrl+Alt+x\""), "{out}");
        let out = unbind(file, "tree", "Ctrl+Alt+x").unwrap();
        assert!(!out.contains("Alt+Ctrl+x"), "{out}");
    }

    #[test]
    fn keys_are_bindable_when_some_are_given_and_the_keymap_takes_them() {
        assert!(bindable("tree", "J", "cursor.down"));
        assert!(bindable("tree", "g g", "cursor.first"));
        assert!(!bindable("tree", "", "cursor.down"));
        assert!(!bindable("tree", "Hyper+j", "cursor.down"));
        assert!(!bindable("tree", "J", "no.such.action"));
    }

    #[test]
    fn a_binding_saved_under_its_own_keys_takes_the_new_action() {
        let out = rebind(FILE, "tree", Some("J"), "J", "cursor.up").unwrap();
        assert_eq!(out, bind(FILE, "tree", "J", "cursor.up").unwrap());
    }

    #[test]
    fn a_binding_saved_under_other_keys_lets_go_of_the_old_ones() {
        let out = rebind(FILE, "tree", Some("J"), "K", "cursor.down").unwrap();
        assert!(!out.contains("\"J\""), "{out}");
        assert!(out.contains("K = \"cursor.down\""), "{out}");
        let out = rebind("", "tree", Some("j"), "J", "cursor.down").unwrap();
        assert!(out.contains("j = \"none\""), "{out}");
        assert!(out.contains("J = \"cursor.down\""), "{out}");
    }

    #[test]
    fn a_binding_saved_as_the_default_is_left_to_the_defaults() {
        let out = rebind(FILE, "tree", Some("J"), "j", "cursor.down").unwrap();
        assert!(!out.contains("\"J\""), "{out}");
        assert!(!out.contains("j ="), "{out}");
        assert_eq!(
            rebind("", "tree", None, "K", "cursor.up").unwrap(),
            bind("", "tree", "K", "cursor.up").unwrap()
        );
    }

    #[test]
    fn the_defaults_are_known_by_area_and_keys() {
        assert_eq!(default_of("tree", "j"), Some("cursor.down".into()));
        assert_eq!(default_of("tree", "J"), None);
        assert_eq!(default_of("tree", "Shift+g"), Some("cursor.last".into()));
    }
}
