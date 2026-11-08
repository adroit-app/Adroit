//! The list of what the terminal host took with it, to bring back.

use adroit_core::msg::Msg;
use adroit_core::picker::interrupted::{Waiting, rows};
use adroit_core::picker::short;
use adroit_model::InterruptedKind;
use adroit_model::time::{ago, now_ms};
use dioxus::prelude::*;

use super::{PickerKeys, QueryField};
use crate::bridge::AppCtx;
use crate::components::{Dialog, ListRow, PlaceMark};
use crate::icons::Icon;

#[component]
pub(super) fn Interrupted(list: Waiting) -> Element {
    let app = use_context::<AppCtx>();
    let home = app.picker.read().home.clone();
    let hosts = app.hosts.read();
    let rows = rows(&list, &home, &hosts);
    let now = now_ms();
    let keys: &[(&str, &str)] = &[
        ("Ctrl+n/p", "move"),
        ("Enter", "bring back"),
        ("Esc", "close"),
    ];
    rsx! {
        Dialog { kind: "picker-dialog", icon: Icon::Restore, title: "Bring back what the sessions' daemon took",
            div { class: "filter-field",
                QueryField { query: list.query.clone(), placeholder: "Sessions and shells" }
            }
            div { class: "list-rows",
                if rows.is_empty() {
                    ListRow { kind: "dim",
                        if list.query.is_empty() { "Nothing waits to be brought back" } else { "Nothing matches" }
                    }
                }
                for (i, waiting) in rows.into_iter().enumerate() {
                    ListRow {
                        key: "{waiting.id}",
                        selected: i == list.cursor,
                        onclick: move |_| app.dispatch(Msg::PickerChoose(i)),
                        PlaceMark { icon: if waiting.is_session() { Icon::Message } else { Icon::Terminal } }
                        span { class: "list-row-name",
                            "{waiting.name}"
                            span { class: "picker-place", "{short(&waiting.cwd, &home)}" }
                        }
                        span { class: "list-row-meta",
                            match &waiting.what {
                                InterruptedKind::Session { agent, .. } => agent.label(),
                                InterruptedKind::Terminal { .. } => "terminal",
                            }
                            " · started {ago(now.saturating_sub(waiting.started_ms))}"
                        }
                    }
                }
            }
            PickerKeys { keys: keys.iter().map(|(k, w)| (k.to_string(), w.to_string())).collect() }
        }
    }
}
