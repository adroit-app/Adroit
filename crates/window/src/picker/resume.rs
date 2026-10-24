//! The list of past conversations.

use adroit_core::msg::Msg;
use adroit_core::picker::resume::{Resume as List, rows};
use adroit_core::picker::short;
use adroit_model::time::{ago, now_ms};
use dioxus::prelude::*;

use super::{PickerKeys, QueryField};
use crate::bridge::AppCtx;
use crate::components::{Button, Dialog, ListRow, PlaceMark};
use crate::icons::Icon;

#[component]
pub(super) fn Resume(list: List) -> Element {
    let app = use_context::<AppCtx>();
    let home = app.picker.read().home.clone();
    let settings = app.config.read().settings.clone();
    let host = app.host.read();
    let rows = rows(&list, &home, host.work.as_ref());
    let now = now_ms();
    let scope = match (&list.dir, list.every) {
        (Some(dir), false) => short(dir, &home),
        _ => "every directory".to_string(),
    };
    let keys: &[(&str, &str)] = &[
        ("Ctrl+n/p", "move"),
        ("Enter", "resume"),
        ("Ctrl+a", "this directory or every one"),
        ("Esc", "close"),
    ];
    rsx! {
        Dialog { kind: "picker-dialog", icon: Icon::TransferIn, title: "Resume a conversation",
            div { class: "filter-field",
                QueryField { query: list.query.clone(), placeholder: "Conversations of {scope}" }
                if list.dir.is_some() {
                    Button {
                        onclick: move |_| app.dispatch(Msg::PickerEvery),
                        if list.every { "Every directory" } else { "{scope}" }
                    }
                }
            }
            div { class: "list-rows",
                match rows {
                    None => rsx! { ListRow { kind: "dim", "Reading the conversations…" } },
                    Some(rows) if rows.is_empty() => rsx! {
                        ListRow { kind: "dim",
                            if list.query.is_empty() { "No conversation was kept here" } else { "Nothing matches" }
                        }
                    },
                    Some(rows) => rsx! {
                        for (i, row) in rows.into_iter().enumerate() {
                            ListRow {
                                key: "{row.past.agent:?}:{row.past.id}",
                                selected: i == list.cursor,
                                onclick: move |_| app.dispatch(Msg::PickerChoose(i)),
                                PlaceMark { icon: if row.session.is_some() { Icon::Message } else { Icon::TransferIn } }
                                span { class: "list-row-name",
                                    "{row.past.title}"
                                    span { class: "picker-place", "{short(&row.past.cwd, &home)}" }
                                }
                                span { class: "list-row-meta",
                                    if row.session.is_some() { "open · " }
                                    "{adroit_core::profiles::resumed_as(&settings, &row.past)} · {ago(now.saturating_sub(row.past.at_ms))}"
                                }
                            }
                        }
                    },
                }
            }
            PickerKeys { keys: keys.iter().map(|(k, w)| (k.to_string(), w.to_string())).collect() }
        }
    }
}
