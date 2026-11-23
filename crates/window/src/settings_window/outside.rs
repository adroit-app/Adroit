//! What programs outside Adroit may do to sessions, and the keys the user
//! gave them.

use adroit_model::time::{ago, now_ms};
use adroit_model::{MessageId, OutsideKey};
use adroit_protocol::{Answer, Ask};
use dioxus::prelude::*;

use super::Change;
use crate::components::{
    Button, ConfirmInline, SettingGroup, SettingRow, SettingsError, TextField, Toggle,
};
use crate::link::Link;
use crate::settings::Settings;

/// Why a key could not be listed, issued or taken back, as `answer` says.
fn refusal(answer: Answer) -> String {
    match answer {
        Answer::Failed { text } | Answer::Unconfirmed { text } => text,
        _ => "The work host is of an Adroit that gives no keys.".into(),
    }
}

/// When `key` was given and last used, `now` being the time.
fn dated(key: &OutsideKey, now: u64) -> String {
    let given = format!("Given {}", ago(now.saturating_sub(key.issued_ms)));
    match key.used_ms {
        Some(used) => format!("{given} · used {}", ago(now.saturating_sub(used))),
        None => format!("{given} · never used"),
    }
}

/// The key just given, when it is the one `id`.
fn shown(issued: &Option<(MessageId, String)>, id: MessageId) -> Option<String> {
    issued
        .as_ref()
        .filter(|(given, _)| *given == id)
        .map(|(_, key)| key.clone())
}

#[component]
pub(super) fn Outside(steer: bool, change: Callback<Change>) -> Element {
    let link = use_context::<Link>();
    let mut keys = use_signal(Vec::<OutsideKey>::new);
    let mut name = use_signal(String::new);
    let mut issued = use_signal(|| None::<(MessageId, String)>);
    let mut error = use_signal(|| None::<String>);
    let mut revoking = use_signal(|| None::<OutsideKey>);
    let mut listed = use_signal(|| 0u32);
    use_effect({
        let link = link.clone();
        move || {
            listed.read();
            let asked = link.ask(Ask::OutsideKeys);
            spawn(async move {
                match asked.await {
                    Answer::OutsideKeys { keys: got } => keys.set(got),
                    answer => error.set(Some(refusal(answer))),
                }
            });
        }
    });
    rsx! {
        SettingGroup { title: "Programs outside Adroit",
            if let Some(why) = error() { SettingsError { "{why}" } }
            SettingRow { label: "Let programs with a key act for you", hint: "Answer what a session asks you, interrupt its turn and end it. A session that runs commands outside a sandbox as you could get at a key or at Adroit itself.",
                Toggle {
                    checked: steer,
                    onchange: move |on| change(Box::new(move |s: &mut Settings| s.outside.steer = on)),
                }
            }
            for key in keys.read().clone() {
                SettingRow { key: "{key.id}", label: key.name.clone(),
                    hint: if shown(&issued.read(), key.id).is_some() {
                        "Its key is shown this once. Keep it where only the program reads it.".into()
                    } else {
                        dated(&key, now_ms())
                    },
                    if let Some(text) = shown(&issued.read(), key.id) {
                        code { class: "serving-detail", "{text}" }
                        Button { kind: "primary", onclick: move |_| issued.set(None), "Done" }
                    } else {
                        Button { kind: "danger", onclick: {
                            let key = key.clone();
                            move |_| revoking.set(Some(key.clone()))
                        }, "Revoke" }
                    }
                }
            }
            if let Some(key) = revoking() {
                ConfirmInline { question: format!("Revoke the key of {}?", key.name), confirm: "Revoke",
                    onkeep: move |()| revoking.set(None),
                    onconfirm: {
                        let link = link.clone();
                        move |()| {
                            revoking.set(None);
                            issued.set(None);
                            error.set(None);
                            let asked = link.ask(Ask::RevokeOutsideKey { id: key.id });
                            spawn(async move {
                                match asked.await {
                                    Answer::Done => listed += 1,
                                    answer => error.set(Some(refusal(answer))),
                                }
                            });
                        }
                    },
                }
            }
            SettingRow { label: "Give a program a key", hint: "It acts under this name, which its sessions' messages show.",
                TextField { value: name(), placeholder: "Program's name", aria_label: "Program's name",
                    onchange: move |value: String| name.set(value),
                }
                Button { disabled: name.read().trim().is_empty(), onclick: {
                    let link = link.clone();
                    move |_| {
                        error.set(None);
                        issued.set(None);
                        let asked = link.ask(Ask::IssueOutsideKey { name: name.peek().clone() });
                        spawn(async move {
                            match asked.await {
                                Answer::OutsideKeyIssued { id, key } => {
                                    issued.set(Some((id, key)));
                                    name.set(String::new());
                                    listed += 1;
                                }
                                answer => error.set(Some(refusal(answer))),
                            }
                        });
                    }
                }, "Give a key" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_just_given_is_shown_with_its_own_program_alone() {
        let (given, other) = (MessageId::new(), MessageId::new());
        let issued = Some((given, "secret".to_string()));
        assert_eq!(shown(&issued, given).as_deref(), Some("secret"));
        assert_eq!(shown(&issued, other), None);
        assert_eq!(shown(&None, given), None);
    }

    #[test]
    fn a_key_is_dated_by_when_it_was_given_and_last_used() {
        let key = |used_ms| OutsideKey {
            id: adroit_model::MessageId::new(),
            name: "Famulen".into(),
            issued_ms: 0,
            used_ms,
        };
        let hour = 60 * 60 * 1000;
        let never = dated(&key(None), 2 * hour);
        assert!(never.ends_with("never used"), "{never}");
        let used = dated(&key(Some(hour)), 2 * hour);
        assert!(used.contains("used "), "{used}");
    }
}
