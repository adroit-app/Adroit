//! The picker drawn over the window. It only draws: the window feeds the
//! keys into [`Picker`].

use std::path::PathBuf;

use dioxus::prelude::*;

use crate::filer::Row;
use crate::fuzzy::Query;
use crate::icons::{Icon, IconView};
use crate::keys;
use crate::paths::home;
use crate::picker::{Line, Mark, Mode, Picker, short};

/// `text` with what `query` marked in it drawn as matched.
fn marked(query: &Query, text: &str) -> Element {
    rsx! {
        for (j, s) in query.highlight_path(text).into_iter().enumerate() {
            if s.matched {
                span { key: "{j}", class: "picker-match", "{s.text}" }
            } else {
                span { key: "{j}", "{s.text}" }
            }
        }
    }
}

#[component]
pub fn PickerView(
    picker: Picker,
    rows: Vec<Line>,
    tree: Vec<Row>,
    /// Told the row of the list chosen, by its place in `rows`.
    onchoose: EventHandler<usize>,
    /// Told the directory chosen in the tree.
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
                div { class: "dialog-title", "Go to a session, or start one" }
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
                                    div { class: "list-row dim", "Nothing matches" }
                                }
                                for (i, line) in rows.iter().enumerate() {
                                    Fragment { key: "{i}:{line.name}",
                                        if let Some(heading) = line.heading {
                                            div { class: "picker-heading", "{heading}" }
                                        }
                                        div {
                                            class: match (line.mark == Mark::Directory, i == picker.cursor) {
                                                (true, true) => "list-row mono selected",
                                                (true, false) => "list-row mono",
                                                (false, true) => "list-row selected",
                                                (false, false) => "list-row",
                                            },
                                            onclick: move |_| onchoose.call(i),
                                            span { class: "place-mark", IconView { icon: match line.mark { Mark::Scratch => Icon::Notes, Mark::Session => Icon::Message, Mark::Directory => Icon::Folder } } }
                                            span { class: "list-row-name",
                                                {marked(&query, &line.name)}
                                                if let Some(place) = &line.place {
                                                    span { class: "picker-place", {marked(&query, place)} }
                                                }
                                            }
                                            span { class: "list-row-meta", "{line.badge}" }
                                        }
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
                                        IconView { icon: if picker.filer.is_expanded(r) { Icon::ChevronDown } else { Icon::ChevronRight }, size: 12 }
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
