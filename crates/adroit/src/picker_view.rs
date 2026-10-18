//! The picker drawn over the window. It only draws: the window feeds the
//! keys into [`Picker`].

use std::path::PathBuf;

use dioxus::prelude::*;

use crate::filer::Row;
use crate::fuzzy::Query;
use crate::keys;
use crate::paths::home;
use crate::picker::{Candidate, Mode, Picker, short};

#[component]
pub fn PickerView(
    picker: Picker,
    rows: Vec<Candidate>,
    tree: Vec<Row>,
    onpick: EventHandler<PathBuf>,
    /// Told what the query field holds as it is typed in.
    onquery: EventHandler<String>,
    /// Told the keys the query field does not type with.
    onkey: EventHandler<adroit_term::Key>,
) -> Element {
    let home = home();
    let keys: &[(&str, &str)] = match picker.mode {
        Mode::List => &[
            ("Ctrl+n/p", "move"),
            ("Enter", "choose"),
            ("Ctrl+f", "file tree"),
            ("Esc", "close"),
        ],
        Mode::Tree => &[
            ("j/k", "move"),
            ("l", "open"),
            ("h", "close"),
            ("!", "hidden"),
            ("Enter", "choose"),
            ("Ctrl+f", "list"),
            ("Esc", "close"),
        ],
    };
    rsx! {
        div { class: "dialog-scrim",
            div { class: "dialog picker-dialog",
                // A click inside is not one on the shade around it, which closes.
                onclick: move |e| e.stop_propagation(),
                div { class: "dialog-title", "Where does the session work?" }
                match picker.mode {
                    Mode::List => {
                        let query = Query::new(&picker.query);
                        rsx! {
                            div { class: "filter-field",
                                input {
                                    class: "filter-field-input",
                                    value: "{picker.query}",
                                    autocomplete: "off",
                                    spellcheck: "false",
                                    "autocorrect": "off",
                                    "autocapitalize": "off",
                                    onmounted: move |e: MountedEvent| async move {
                                        let _ = e.data().set_focus(true).await;
                                    },
                                    oninput: move |e: Event<FormData>| onquery.call(e.value()),
                                    onkeydown: move |e: Event<KeyboardData>| {
                                        if let Some(k) = keys::of_field(&e).filter(|k| !keys::edits_text(k)) {
                                            // Tab and the arrows would move the
                                            // focus or the caret.
                                            e.prevent_default();
                                            onkey.call(k);
                                        }
                                    },
                                }
                            }
                            div { class: "list-rows",
                                if rows.is_empty() {
                                    div { class: "list-row dim", "No directory matches" }
                                }
                                for (i, c) in rows.iter().enumerate() {
                                    div {
                                        key: "{c.path.display()}",
                                        class: if i == picker.cursor { "list-row mono selected" } else { "list-row mono" },
                                        onclick: {
                                            let path = c.path.clone();
                                            move |_| onpick.call(path.clone())
                                        },
                                        span { class: "list-row-name",
                                            for (j, s) in query.highlight_path(&short(&c.path, &home)).into_iter().enumerate() {
                                                if s.matched {
                                                    span { key: "{j}", class: "picker-match", "{s.text}" }
                                                } else {
                                                    span { key: "{j}", "{s.text}" }
                                                }
                                            }
                                        }
                                        span { class: "list-row-meta", "{c.source.label()}" }
                                    }
                                }
                            }
                        }
                    }
                    Mode::Tree => rsx! {
                        div { class: "list-rows",
                            for r in tree.iter() {
                                div {
                                    key: "{r.root.display()}:{r.path.display()}",
                                    class: if picker.filer.cursor() == Some(r) { "list-row mono selected" } else { "list-row mono" },
                                    style: "padding-left: {8 + r.depth * 16}px",
                                    onclick: {
                                        let path = r.path.clone();
                                        move |_| onpick.call(path.clone())
                                    },
                                    span { class: "picker-chevron",
                                        if picker.filer.is_expanded(r) { "▾" } else { "▸" }
                                    }
                                    span { class: "list-row-name",
                                        if r.depth == 0 {
                                            "{short(&r.path, &home)}"
                                        } else {
                                            "{r.path.file_name().unwrap_or_default().to_string_lossy()}"
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
                div { class: "picker-keys",
                    for (k, what) in keys.iter() {
                        span { key: "{k}", class: "kbd-list", kbd { "{k}" } " {what}" }
                    }
                }
            }
        }
    }
}
