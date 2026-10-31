//! The picker that goes to a session or starts one.

use adroit_core::msg::Msg;
use adroit_core::picker::fuzzy::Query;
use adroit_core::picker::{Mark, Mode, Picker, go_lines, short, tree_rows};
use dioxus::prelude::*;

use super::{PickerKeys, QueryField};
use crate::bridge::AppCtx;
use adroit_core::actions::Action;
use adroit_core::keymap::Context;

use crate::components::{Dialog, FileLook, FileRow, ListRow, PlaceMark};
use crate::icons::Icon;

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
    let hosts = app.hosts.read();
    let work = hosts.own().work.as_ref();
    let fixed = |keys: &[(&str, &str)]| -> Vec<(String, String)> {
        keys.iter()
            .map(|(k, w)| (k.to_string(), w.to_string()))
            .collect()
    };
    let keys = match picker.mode {
        Mode::List => fixed(&[
            ("Ctrl+n/p", "move"),
            ("Enter", "choose"),
            ("Ctrl+f", "file tree"),
            ("Esc", "close"),
        ]),
        // The filer's keys, as the keymap binds them.
        Mode::Tree => {
            let km = app.chrome.read().keymap.clone();
            let filer = [
                (Action::FilerDown, "down"),
                (Action::FilerUp, "up"),
                (Action::FilerInto, "open"),
                (Action::FilerOut, "close"),
                (Action::FilerHidden, "hidden"),
                (Action::FilerOpen, "choose"),
            ];
            filer
                .iter()
                .filter_map(|(a, w)| km.keys_for(Context::Filer, a).map(|k| (k, w.to_string())))
                .chain(fixed(&[("Ctrl+f", "list"), ("Esc", "close")]))
                .collect()
        }
    };
    rsx! {
        Dialog { kind: "picker-dialog", title: "Go to a session, or start one",
            match picker.mode {
                Mode::List => {
                    let query = Query::new(&picker.query);
                    let rows = go_lines(&picker, &home, &hosts);
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
                                        PlaceMark { icon: match line.mark { Mark::Scratch => Icon::Notes, Mark::Session => Icon::Message, Mark::Directory => Icon::Folder } }
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
                            FileRow {
                                key: "{r.root.display()}:{r.path.display()}",
                                path: r.path.clone(),
                                depth: r.depth,
                                look: FileLook {
                                    dir: true,
                                    back: adroit_model::files::is_hidden(&r.path),
                                    ..FileLook::default()
                                },
                                open: picker.filer.is_expanded(&r),
                                label: (r.depth == 0).then(|| short(&r.path, &home)),
                                selected: picker.filer.cursor() == Some(&r),
                                onclick: {
                                    let path = r.path.clone();
                                    move |_| app.dispatch(Msg::PickerDir(path.clone()))
                                },
                            }
                        }
                    }
                },
            }
            PickerKeys { keys }
        }
    }
}
