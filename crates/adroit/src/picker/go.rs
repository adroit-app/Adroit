//! The picker that goes to a session or starts one.

use adroit_core::msg::Msg;
use adroit_core::picker::fuzzy::Query;
use adroit_core::picker::{self, Mark, Mode, Picker, go_rows, short, tree_rows};
use dioxus::prelude::*;

use super::QueryField;
use crate::bridge::AppCtx;
use crate::components::{Dialog, KeyHint, ListRow};
use crate::icons::{Icon, IconView};

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
pub(super) fn Go(picker: Picker) -> Element {
    let app = use_context::<AppCtx>();
    let home = app.picker.read().home.clone();
    let host = app.host.read();
    let work = host.work.as_ref();
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
        Dialog { kind: "picker-dialog", title: "Go to a session, or start one",
            match picker.mode {
                Mode::List => {
                    let query = Query::new(&picker.query);
                    let rows = picker::lines(&go_rows(&picker, &home, work), &home);
                    rsx! {
                        div { class: "filter-field",
                            QueryField { query: picker.query.clone() }
                        }
                        div { class: "list-rows",
                            if rows.is_empty() {
                                ListRow { kind: "dim", "Nothing matches" }
                            }
                            for (i, line) in rows.iter().enumerate() {
                                Fragment { key: "{i}:{line.name}",
                                    if let Some(heading) = line.heading {
                                        div { class: "picker-heading", "{heading}" }
                                    }
                                    ListRow {
                                        kind: if line.mark == Mark::Directory { "mono" } else { "" },
                                        selected: i == picker.cursor,
                                        onclick: move |_| app.dispatch(Msg::PickerChoose(i)),
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
                        for r in tree_rows(&picker, work) {
                            ListRow {
                                key: "{r.root.display()}:{r.path.display()}",
                                kind: "mono",
                                selected: picker.filer.cursor() == Some(&r),
                                style: "padding-left: {8 + r.depth * 16}px",
                                onclick: {
                                    let path = r.path.clone();
                                    move |_| app.dispatch(Msg::PickerDir(path.clone()))
                                },
                                span { class: "picker-chevron",
                                    IconView { icon: if picker.filer.is_expanded(&r) { Icon::ChevronDown } else { Icon::ChevronRight }, size: 12 }
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
                    KeyHint { key: "{k}", keys: "{k}", does: "{what}" }
                }
            }
        }
    }
}
