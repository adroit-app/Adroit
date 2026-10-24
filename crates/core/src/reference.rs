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
        let does = match spec.repeats {
            true => format!("{}; repeats ([keys](keys.md#repeating))", spec.does),
            false => spec.does.to_string(),
        };
        writeln!(
            out,
            "| {} | {} | {} | {} |",
            cell(&format!("`{}`", usage(spec))),
            cell(bar),
            cell(&rebase_links(&does)),
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
    // The schema of the settings as adroit writes them: what it reads only
    // from earlier files is left out.
    let schema = serde_json::to_value(
        schemars::generate::SchemaSettings::default()
            .for_serialize()
            .into_generator()
            .into_root_schema_for::<Settings>(),
    )
    .unwrap();
    let defs = schema["$defs"].as_object().cloned().unwrap_or_default();
    let written: toml::Table = toml::to_string(&Settings::default())
        .unwrap()
        .parse()
        .unwrap();
    let mut out = String::new();
    writeln!(out, "# Settings reference\n\n{GENERATED}\n").unwrap();
    out.push_str(
        "Every setting `settings.toml` holds, table by table: its type, what it is\n\
         when the file does not name it, and what it does\n\
         ([settings](../settings.md)).\n\n",
    );
    let properties = schema["properties"].as_object().unwrap();
    let (nested, top): (Vec<_>, Vec<_>) = properties
        .iter()
        .partition(|(_, field)| table(field, &defs).is_some() || entries(field, &defs).is_some());
    out.push_str("## The top level\n\n");
    rows(&mut out, "", top, &defs);
    for (name, field) in nested {
        if let Some(def) = table(field, &defs) {
            section(
                &mut out,
                "##",
                &format!("[{name}]"),
                name,
                field,
                def,
                &defs,
            );
            continue;
        }
        let def = entries(field, &defs).unwrap();
        section(
            &mut out,
            "##",
            &format!("[[{name}]]"),
            name,
            field,
            def,
            &defs,
        );
        if let Some(entries) = written
            .get(name)
            .filter(|e| e.as_array().is_some_and(|e| !e.is_empty()))
        {
            let file = toml::Table::from_iter([(name.clone(), entries.clone())]);
            writeln!(
                out,
                "\nWhen the file holds none, it holds these, as adroit writes them:\n\n```toml\n{}```",
                toml::to_string_pretty(&file).unwrap()
            )
            .unwrap();
        }
    }
    out
}

/// A table's rows, then each table within it under a heading of its own.
fn section(
    out: &mut String,
    level: &str,
    heading: &str,
    path: &str,
    field: &Value,
    def: &Value,
    defs: &Map<String, Value>,
) {
    writeln!(out, "\n{level} `{heading}`\n").unwrap();
    if let Some(about) = description(field).or_else(|| description(def)) {
        writeln!(out, "{about}\n").unwrap();
    }
    let (inner, leaves): (Vec<_>, Vec<_>) = def["properties"]
        .as_object()
        .unwrap()
        .iter()
        .partition(|(_, f)| table(f, defs).is_some() || entries(f, defs).is_some());
    rows(out, &format!("{path}."), leaves, defs);
    for (name, f) in inner {
        let path = format!("{path}.{name}");
        let (heading, def) = match table(f, defs) {
            Some(def) => (format!("[{path}]"), def),
            None => (format!("[[{path}]]"), entries(f, defs).unwrap()),
        };
        section(out, "###", &heading, &path, f, def, defs);
    }
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

/// The definition a field's `$ref` names, through an optional one, or the
/// field itself.
fn resolve<'a>(field: &'a Value, defs: &'a Map<String, Value>) -> &'a Value {
    let field = field["anyOf"]
        .as_array()
        .and_then(|any| any.iter().find(|f| f["type"] != "null"))
        .unwrap_or(field);
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

/// The definition of each entry of a field that is an array of tables.
fn entries<'a>(field: &'a Value, defs: &'a Map<String, Value>) -> Option<&'a Value> {
    let def = resolve(field, defs);
    (def["type"] == "array")
        .then(|| table(&def["items"], defs))
        .flatten()
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
    // One that may be left out is typed as its value and null.
    let kind = match &schema["type"] {
        Value::Array(kinds) => kinds.iter().find(|k| *k != "null"),
        kind => Some(kind),
    };
    match kind.and_then(Value::as_str) {
        Some("array") => format!("array of {}s", type_of(&schema["items"])),
        Some("object") => format!("table of {}s", type_of(&schema["additionalProperties"])),
        Some(kind) => kind.to_string(),
        None => String::new(),
    }
}

/// A doc comment as one line.
fn description(schema: &Value) -> Option<String> {
    let text = schema["description"].as_str()?;
    Some(rebase_links(
        &text.split_whitespace().collect::<Vec<_>>().join(" "),
    ))
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
        assert!(s.contains("| `profile` | string | `\"claude\"` |"), "{s}");
        assert!(
            s.contains("| `profiles.agent` | one of `claude`, `codex`, `pi`, `opencode` |  |"),
            "{s}"
        );
        assert!(
            s.contains("| `profiles.launch.command` | array of strings | `[]` |"),
            "{s}"
        );
        // An array of tables within a table has a heading of its own too.
        assert!(s.contains("### `[[profiles.opencode.permissions]]`"), "{s}");
        assert!(
            s.contains("| `profiles.opencode.permissions.effect` | string | `\"\"` | `allow`, `ask` or `deny`. |"),
            "{s}"
        );
        assert!(
            s.contains(
                "| `profiles.launch.env` | table of strings | `{}` | Variables set for it. |"
            ),
            "{s}"
        );
        assert!(
            s.contains("| `delegation.max_started` | integer | `4` |"),
            "{s}"
        );
        // A setting that may be left out is of the type it has when it is not.
        assert!(s.contains("| `profiles.home` | string |  |"), "{s}");
        assert!(
            s.contains("## `[[profiles]]`\n\nThe profiles sessions"),
            "{s}"
        );
        assert!(
            s.contains("### `[profiles.claude]`\n\nClaude Code's model"),
            "{s}"
        );
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
        // An entry of an array of tables is no default of its fields, so only
        // its fields are looked for.
        fn leaves(prefix: &str, table: &toml::Table, out: &mut Vec<(String, Option<toml::Value>)>) {
            for (name, value) in table {
                let path = format!("{prefix}{name}");
                match value {
                    toml::Value::Table(t) if name != "env" => leaves(&format!("{path}."), t, out),
                    toml::Value::Array(entries)
                        if entries.iter().all(toml::Value::is_table) && !entries.is_empty() =>
                    {
                        let mut fields = Vec::new();
                        for entry in entries {
                            leaves(&format!("{path}."), entry.as_table().unwrap(), &mut fields);
                        }
                        out.extend(fields.into_iter().map(|(name, _)| (name, None)));
                    }
                    _ => out.push((path, Some(value.clone()))),
                }
            }
        }
        let mut found = Vec::new();
        leaves("", &written, &mut found);
        for (name, value) in found {
            // An array of tables written empty is a heading of its own, as is
            // one in an entry of another.
            if value
                .as_ref()
                .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty))
                && reference.contains(&format!(" `[[{name}]]`\n"))
            {
                continue;
            }
            let row = format!("| `{name}` |");
            let line = reference.lines().find(|l| l.starts_with(&row));
            let line = line.unwrap_or_else(|| panic!("{name} is not in the reference"));
            if let Some(value) = value {
                assert!(line.contains(&format!("| `{value}` |")), "{line}");
            }
        }
    }

    #[test]
    fn a_setting_retired_is_left_out_of_the_reference() {
        assert!(!settings().contains("orchestration"));
    }
}
