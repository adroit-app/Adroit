//! The list of past conversations drawn over the window. It only draws: the
//! window's view state holds the list, and its keys go there.

use adroit_core::msg::Msg;
use adroit_core::picker::resume::rows;
use adroit_model::Agent;
use adroit_model::time::{ago, now_ms};
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::components::{Dialog, KeyHint, ListRow};
use crate::icons::{Icon, IconView};
use crate::keys;
use crate::paths::home;
use crate::picker::short;

fn agent_label(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "Claude Code",
        Agent::Codex => "Codex",
    }
}

#[component]
pub fn ResumeView() -> Element {
    let app = use_context::<AppCtx>();
    let view = app.resume.read().clone();
    if !view.open {
        return rsx! {};
    }
    let host = app.host.read();
    let rows = rows(&view, host.work.as_ref());
    let (home, now) = (home(), now_ms());
    let scope = match (&view.dir, view.every) {
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
        div {
            onclick: move |_| app.dispatch(Msg::ResumeClose),
            Dialog { kind: "picker-dialog", icon: Icon::TransferIn, title: "Resume a conversation",
                div { class: "filter-field",
                    input {
                        class: "filter-field-input",
                        value: "{view.query}",
                        placeholder: "Conversations of {scope}",
                        autocomplete: "off",
                        spellcheck: "false",
                        "autocorrect": "off",
                        "autocapitalize": "off",
                        onmounted: move |e: MountedEvent| async move {
                            let _ = e.data().set_focus(true).await;
                        },
                        oninput: move |e: Event<FormData>| app.dispatch(Msg::ResumeQuery(e.value())),
                        onkeydown: move |e: Event<KeyboardData>| {
                            if let Some(k) = keys::of_field(&e).filter(|k| !keys::edits_text(k)) {
                                // Tab and the arrows would move the focus or
                                // the caret.
                                e.prevent_default();
                                app.dispatch(Msg::ResumeKey(k));
                            }
                        },
                    }
                    if view.dir.is_some() {
                        button { class: "button",
                            onclick: move |_| app.dispatch(Msg::ResumeEvery),
                            if view.every { "Every directory" } else { "{scope}" }
                        }
                    }
                }
                div { class: "list-rows",
                    match rows {
                        None => rsx! { ListRow { kind: "dim", "Reading the conversations…" } },
                        Some(rows) if rows.is_empty() => rsx! {
                            ListRow { kind: "dim",
                                if view.query.is_empty() { "No conversation was kept here" } else { "Nothing matches" }
                            }
                        },
                        Some(rows) => rsx! {
                            for (i, row) in rows.into_iter().enumerate() {
                                ListRow {
                                    key: "{row.past.agent:?}:{row.past.id}",
                                    selected: i == view.cursor,
                                    onclick: move |_| app.dispatch(Msg::ResumeChoose(i)),
                                    span { class: "place-mark",
                                        IconView { icon: if row.session.is_some() { Icon::Message } else { Icon::TransferIn } }
                                    }
                                    span { class: "list-row-name",
                                        "{row.past.title}"
                                        span { class: "picker-place", "{short(&row.past.cwd, &home)}" }
                                    }
                                    span { class: "list-row-meta",
                                        if row.session.is_some() { "open · " }
                                        "{agent_label(row.past.agent)} · {ago(now.saturating_sub(row.past.at_ms))}"
                                    }
                                }
                            }
                        },
                    }
                }
                div { class: "picker-keys",
                    for (k, what) in keys.iter() {
                        KeyHint { key: "{k}", keys: "{k}", does: "{what}" }
                    }
                }
            }
        }
    }
}
