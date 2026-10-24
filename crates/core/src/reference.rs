//! The references in `docs/spec/reference/`, written from what the code
//! holds: every action with its default keys, and every setting with its
//! type and default. `just references` writes them; a test fails when the
//! files checked in say otherwise.

use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::actions::{ACTIONS, Action, ActionSpec};
use crate::keymap::{Context, DEFAULTS};
use crate::settings::Settings;

/// Each reference: where it is kept, from the root of the repository, and
/// what it says.
pub fn files() -> Vec<(&'static str, String)> {
    vec![
        ("docs/spec/reference/keys.md", keys()),
        ("docs/spec/reference/settings.md", settings()),
    ]
}

const GENERATED: &str =
    "<!-- Written by `just references` from the code; edit the code, not this file. -->";

/// The key reference: every action, what it does and the keys adroit binds
/// it to, then the default bindings as a file of bindings writes them.
pub fn keys() -> String {
    let listed = default_bindings();
    let mut out = String::new();
    writeln!(out, "# Key reference\n\n{GENERATED}\n").unwrap();
    out.push_str(
        "Every action a binding can name, what it does, what the menu bar calls it,\n\
         and the keys adroit binds it to in each area before a file of bindings\n\
         changes them ([keys](../keys.md)).\n\n",
    );
    out.push_str("## Actions\n\n");
    out.push_str("| Action | Menu bar | Does | Default keys |\n| --- | --- | --- | --- |\n");
    for spec in ACTIONS {
        let bound = Context::ALL
            .into_iter()
            .filter_map(|ctx| {
                let keys: Vec<String> = listed
                    .iter()
                    .filter(|(area, _, action)| {
                        *area == ctx.name() && action.split(' ').next() == Some(spec.name)
                    })
                    .map(|(_, keys, action)| match action.split_once(' ') {
                        Some(_) => format!("`{keys}` (`{action}`)"),
                        None => format!("`{keys}`"),
                    })
                    .collect();
                (!keys.is_empty()).then(|| format!("`{}`: {}", ctx.name(), keys.join(", ")))
            })
            .collect::<Vec<_>>()
            .join("<br>");
        let bar = spec.bar.unwrap_or_default();
        writeln!(
            out,
            "| {} | {} | {} | {} |",
            cell(&format!("`{}`", usage(spec))),
            cell(bar),
            cell(&rebase_links(spec.does)),
            cell(&bound),
        )
        .unwrap();
    }
    out.push_str("\n## Default bindings\n\n```toml\n");
    out.push_str(DEFAULTS.trim_end());
    out.push_str("\n```\n");
    out
}

/// The default bindings, area by area: the area, the keys and the action, as
/// the defaults write them. Read from the text rather than from a
/// [`Keymap`](crate::keymap::Keymap), which has already made `Mod` the keys it
/// is on this platform.
fn default_bindings() -> Vec<(&'static str, String, String)> {
    let table: toml::Table = DEFAULTS.parse().expect("the default bindings read");
    Context::ALL
        .into_iter()
        .filter_map(|ctx| Some((ctx.name(), table.get(ctx.name())?.as_table()?)))
        .flat_map(|(area, entries)| {
            entries.iter().map(move |(keys, action)| {
                let action = action.as_str().unwrap_or_default().to_string();
                (area, keys.clone(), action)
            })
        })
        .collect()
}

/// How a binding names the action, its argument included.
fn usage(spec: &ActionSpec) -> &'static str {
    match spec.action {
        Action::TabGo(_) => "tab.go <n>",
        Action::ConsoleSend(_) => "console.send [<key>]",
        _ => spec.name,
    }
}

/// The settings reference: every setting `settings.toml` holds, table by
/// table, with its type, its default and what it does, as the settings'
/// schema and doc comments say.
pub fn settings() -> String {
    let schema = serde_json::to_value(schemars::schema_for!(Settings)).unwrap();
    let defs = schema["$defs"].as_object().cloned().unwrap_or_default();
    let mut out = String::new();
    writeln!(out, "# Settings reference\n\n{GENERATED}\n").unwrap();
    out.push_str(
        "Every setting `settings.toml` holds, table by table: its type, what it is\n\
         when the file does not name it, and what it does\n\
         ([settings](../settings.md)).\n\n",
    );
    let properties = schema["properties"].as_object().unwrap();
    let (tables, top): (Vec<_>, Vec<_>) = properties
        .iter()
        .partition(|(_, field)| table(field, &defs).is_some());
    out.push_str("## The top level\n\n");
    rows(&mut out, "", top, &defs);
    for (name, field) in tables {
        let def = table(field, &defs).unwrap();
        writeln!(out, "\n## `[{name}]`\n").unwrap();
        if let Some(about) = description(field).or_else(|| description(def)) {
            writeln!(out, "{about}\n").unwrap();
        }
        let fields = def["properties"].as_object().unwrap().iter().collect();
        rows(&mut out, &format!("{name}."), fields, &defs);
    }
    out
}

fn rows(out: &mut String, prefix: &str, fields: Vec<(&String, &Value)>, defs: &Map<String, Value>) {
    out.push_str("| Setting | Type | Default | Description |\n| --- | --- | --- | --- |\n");
    for (name, field) in fields {
        let target = resolve(field, defs);
        let does = description(field)
            .or_else(|| description(target))
            .unwrap_or_default();
        let default = field
            .get("default")
            .map(|v| format!("`{}`", toml_value(v)))
            .unwrap_or_default();
        writeln!(
            out,
            "| {} | {} | {} | {} |",
            cell(&format!("`{prefix}{name}`")),
            cell(&type_of(target)),
            cell(&default),
            cell(&does),
        )
        .unwrap();
    }
}

/// The definition a field's `$ref` names, or the field itself.
fn resolve<'a>(field: &'a Value, defs: &'a Map<String, Value>) -> &'a Value {
    field["$ref"]
        .as_str()
        .and_then(|r| r.strip_prefix("#/$defs/"))
        .and_then(|name| defs.get(name))
        .unwrap_or(field)
}

/// The definition of a field that is a table of settings of its own.
fn table<'a>(field: &'a Value, defs: &'a Map<String, Value>) -> Option<&'a Value> {
    let def = resolve(field, defs);
    def.get("properties").is_some().then_some(def)
}

fn type_of(schema: &Value) -> String {
    if let Some(values) = schema["enum"].as_array() {
        let names: Vec<String> = values
            .iter()
            .map(|v| {
                format!(
                    "`{}`",
                    v.as_str().map_or_else(|| toml_value(v), str::to_string)
                )
            })
            .collect();
        return format!("one of {}", names.join(", "));
    }
    match schema["type"].as_str() {
        Some("array") => format!("array of {}s", type_of(&schema["items"])),
        Some("object") => format!("table of {}s", type_of(&schema["additionalProperties"])),
        Some(kind) => kind.to_string(),
        None => String::new(),
    }
}

/// A doc comment as one line.
fn description(schema: &Value) -> Option<String> {
    let text = schema["description"].as_str()?;
    Some(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// A value as `settings.toml` writes it.
fn toml_value(value: &Value) -> String {
    match toml::Value::try_from(value) {
        Ok(v) => v.to_string(),
        Err(_) => value.to_string(),
    }
}

/// `text` fit for a cell of a Markdown table.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

/// `text` with its links to the specs' files made to reach them from the
/// reference's folder below them.
fn rebase_links(text: &str) -> String {
    static LINK: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\]\(([A-Za-z0-9_-]+\.md(?:#[^)]*)?)\)").unwrap());
    LINK.replace_all(text, "](../$1)").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_reference_names_every_action_with_what_it_does() {
        let keys = keys();
        for spec in ACTIONS {
            assert!(keys.contains(&format!("| `{}", spec.name)), "{}", spec.name);
        }
        assert!(keys.contains(&rebase_links(Action::WindowSplit.spec().does)));
    }

    #[test]
    fn the_key_reference_lists_an_action_s_default_keys_area_by_area() {
        let keys = keys();
        let row = keys
            .lines()
            .find(|l| l.starts_with("| `session.stop` |"))
            .unwrap();
        assert!(
            row.contains("`global`: `Mod+Backspace`<br>`tree`: `X`<br>`console`: `X`"),
            "{row}"
        );
        let row = keys
            .lines()
            .find(|l| l.starts_with("| `tab.go <n>` |"))
            .unwrap();
        assert!(row.contains("`Ctrl+q 3` (`tab.go 3`)"), "{row}");
    }

    #[test]
    fn the_key_reference_holds_the_default_bindings_as_the_file_writes_them() {
        assert!(keys().contains(DEFAULTS.trim()));
    }

    #[test]
    fn a_bar_in_a_key_is_kept_from_ending_the_cell() {
        let keys = keys();
        let row = keys
            .lines()
            .find(|l| l.starts_with("| `window.maximize_width` |"))
            .unwrap();
        assert!(row.contains("`Ctrl+q \\|`"), "{row}");
    }

    #[test]
    fn a_link_to_a_spec_reaches_it_from_the_reference_s_folder() {
        assert_eq!(
            rebase_links("see ([details](sessions.md#agents)) and [x](https://a.b/c.md)"),
            "see ([details](../sessions.md#agents)) and [x](https://a.b/c.md)"
        );
    }

    #[test]
    fn the_settings_reference_gives_each_setting_its_type_default_and_description() {
        let s = settings();
        assert!(
            s.contains("| `zoom` | number | `1.0` | How large the window draws everything"),
            "{s}"
        );
        assert!(
            s.contains("| `agent` | one of `claude`, `codex` | `\"claude\"` |"),
            "{s}"
        );
        assert!(
            s.contains("| `claude.command` | array of strings | `[]` |"),
            "{s}"
        );
        assert!(
            s.contains("| `claude.env` | table of strings | `{}` | Variables set for it. |"),
            "{s}"
        );
        assert!(
            s.contains("| `delegation.max_started` | integer | `4` |"),
            "{s}"
        );
        assert!(s.contains("## `[claude]`\n\nHow Claude Code starts"), "{s}");
    }

    #[test]
    fn every_setting_says_what_it_does() {
        for line in settings().lines().filter(|l| l.starts_with("| `")) {
            assert!(!line.ends_with("|  |"), "{line}");
        }
    }

    #[test]
    fn every_setting_adroit_writes_is_in_the_reference_with_the_default_it_writes() {
        let reference = settings();
        let written: toml::Table = toml::to_string(&Settings::default())
            .unwrap()
            .parse()
            .unwrap();
        let mut leaves = Vec::new();
        for (name, value) in &written {
            match value.as_table() {
                Some(t) if name != "env" => {
                    for (field, v) in t {
                        leaves.push((format!("{name}.{field}"), v.clone()));
                    }
                }
                _ => leaves.push((name.clone(), value.clone())),
            }
        }
        for (name, value) in leaves {
            let row = format!("| `{name}` |");
            let line = reference.lines().find(|l| l.starts_with(&row));
            let line = line.unwrap_or_else(|| panic!("{name} is not in the reference"));
            assert!(line.contains(&format!("| `{value}` |")), "{line}");
        }
    }

    #[test]
    fn a_setting_retired_is_left_out_of_the_reference() {
        assert!(!settings().contains("orchestration"));
    }
}
