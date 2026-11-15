//! The picker that goes to a session or starts one.

use adroit_core::msg::Msg;
use adroit_core::picker::fuzzy::Query;
use adroit_core::picker::{GoPicker, Mark, Mode, Narrowing, Purpose, go_lines, short, tree_rows};
use dioxus::prelude::*;

use super::{PickerKeys, QueryField};
use crate::bridge::AppCtx;

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
pub(super) fn Go(picker: GoPicker) -> Element {
    let app = use_context::<AppCtx>();
    let home = app.picker.read().home.clone();
    let hosts = app.hosts.read();
    let tree = app.tree.read();
    let work = hosts.work(&picker.environment);
    let adding = picker.purpose == Purpose::Add;
    let title = match adding {
        true => "Add a directory to the tree",
        false => "Go to a session, or start one",
    };
    rsx! {
        Dialog { kind: "picker-dialog", onclose: move |_| app.dispatch(Msg::PickerClose), title: "{title}",
            match picker.mode {
                Mode::List => {
                    let query = Query::new(&Narrowing::new(&picker.list.query).text);
                    let rows = go_lines(&picker, &home, &hosts, &tree);
                    rsx! {
                        div { class: "filter-field",
                            QueryField { query: picker.list.query.clone() }
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
                                        kind: if matches!(line.mark, Mark::Directory | Mark::Clone) { "mono" } else { "" },
                                        selected: i == picker.list.cursor,
                                        onclick: move |_| app.dispatch(Msg::PickerChoose(i)),
                                        PlaceMark { icon: match line.mark { Mark::Scratch => Icon::Notes, Mark::Session => Icon::Message, Mark::Directory => Icon::Folder, Mark::Clone => Icon::FolderDown, Mark::Owner => Icon::BrandGithub, Mark::Help => Icon::InfoCircle } }
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
                                label: (r.depth == 0).then(|| short(&r.path, &picker.home)),
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
            PickerKeys {}
        }
    }
}
