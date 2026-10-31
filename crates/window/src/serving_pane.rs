//! This computer served as a work machine, managed from its desktop: the
//! services, pairing other computers and forgetting them.

use std::collections::HashMap;
use std::time::Duration;

use adroit_connect::serving::{Asking, Client, Serving, ThisComputer};
use dioxus::prelude::*;

use crate::components::{SettingGroup, SettingRow, SettingsError, Toggle};

/// How often the pane asks the gateway again, so a request a client just
/// sent shows without the operator reaching for a button.
const LOOK_AGAIN: Duration = Duration::from_secs(1);

/// The start of a fingerprint, as the CLI shows it.
pub(crate) fn short(fingerprint: &str) -> &str {
    fingerprint.get(..16).unwrap_or(fingerprint)
}

/// A computer's name, marked when it is this computer reaching its own
/// gateway.
fn named(name: &str, this_computer: bool) -> String {
    match this_computer {
        true => format!("{name} (this computer)"),
        false => name.to_string(),
    }
}

fn asking_hint(request: &Asking) -> String {
    let paired = match request.paired {
        true => "Paired already; pairing again keeps it paired. ",
        false => "",
    };
    format!(
        "{paired}Type the code it shows. Key {}",
        short(&request.fingerprint)
    )
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// How long ago `then` was, as a person reads it, both in seconds since the
/// Unix epoch.
fn since(then: u64, now: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    let ago = now.saturating_sub(then);
    let (count, unit) = match ago {
        0..MINUTE => return "just now".into(),
        MINUTE..HOUR => (ago / MINUTE, "minute"),
        HOUR..DAY => (ago / HOUR, "hour"),
        _ => (ago / DAY, "day"),
    };
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} {unit}{plural} ago")
}

#[component]
pub(crate) fn ServingPane() -> Element {
    let here = use_hook(ThisComputer::current);
    // None until the gateway was first asked.
    let mut serving = use_signal(|| None::<Option<Serving>>);
    let mut asking = use_signal(Vec::<Asking>::new);
    let mut clients = use_signal(Vec::<Client>::new);
    let mut codes = use_signal(HashMap::<u64, String>::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut notice = use_signal(|| None::<String>);
    // What turning it on asks for; once on, the gateway tells what it does.
    let mut advertise = use_signal(|| true);
    use_future({
        let here = here.clone();
        move || {
            let here = here.clone();
            async move {
                loop {
                    let now = here.serving().await;
                    let waiting = match &now {
                        Some(now) if now.pairing => here.asking().await.unwrap_or_default(),
                        _ => Vec::new(),
                    };
                    // Set only when changed: drawing the pane again while
                    // nothing changed would disturb what is being typed.
                    if *asking.peek() != waiting {
                        asking.set(waiting);
                    }
                    if serving.peek().as_ref() != Some(&now) {
                        serving.set(Some(now));
                    }
                    match here.clients() {
                        Ok(paired) if *clients.peek() != paired => clients.set(paired),
                        Ok(_) => {}
                        Err(why) => error.set(Some(why)),
                    }
                    tokio::time::sleep(LOOK_AGAIN).await;
                }
            }
        }
    });
    let on = matches!(&*serving.read(), Some(Some(_)));
    let pairing = matches!(&*serving.read(), Some(Some(now)) if now.pairing);
    rsx! {
        // Above the groups, so what was just done is seen without scrolling.
        if let Some(why) = error() { SettingsError { "{why}" } }
        if let Some(text) = notice() { p { class: "settings-agent-note", role: "status", "{text}" } }
        SettingGroup { title: "Accept connections",
            SettingRow { label: "Serve this computer",
                hint: if on { "Turning it off stops the agents and sessions paired computers started here. Your own work here keeps running." } else { "Paired computers can run agents and sessions here. It keeps serving across logins and restarts." },
                match &*serving.read() {
                    None => rsx! { span { class: "settings-hint", "Checking…" } },
                    Some(None) => rsx! {
                        button { r#type: "button", class: "primary", disabled: busy(), onclick: {
                            let here = here.clone();
                            move |_| {
                                busy.set(true); error.set(None); notice.set(None);
                                let here = here.clone();
                                spawn(async move {
                                    match here.serve(advertise()).await {
                                        Ok(notes) if notes.is_empty() => {}
                                        Ok(notes) => notice.set(Some(notes.join(" "))),
                                        Err(why) => error.set(Some(why)),
                                    }
                                    busy.set(false);
                                });
                            }
                        }, if busy() { "Starting…" } else { "Turn on" } }
                    },
                    Some(Some(_)) => rsx! {
                        button { r#type: "button", disabled: busy(), onclick: move |_| {
                            busy.set(true); error.set(None); notice.set(None);
                            spawn(async move {
                                if let Err(why) = adroit_connect::serving::disable().await { error.set(Some(why)); }
                                busy.set(false);
                            });
                        }, if busy() { "Stopping…" } else { "Turn off" } }
                    },
                }
            }
            SettingRow { label: "Announce on this network", hint: "Computers on the same network find this one without typing its address. Off, they pair with its address alone.",
                Toggle {
                    checked: match &*serving.read() { Some(Some(now)) => now.announcing, _ => advertise() },
                    onchange: {
                        let here = here.clone();
                        move |announce: bool| {
                            advertise.set(announce);
                            if !on { return; }
                            busy.set(true); error.set(None); notice.set(None);
                            let here = here.clone();
                            spawn(async move {
                                if let Err(why) = here.serve(announce).await { error.set(Some(why)); }
                                busy.set(false);
                            });
                        }
                    },
                }
            }
        }
        if on {
            SettingGroup { title: "Pair a computer",
                SettingRow { label: if pairing { "Waiting for requests" } else { "Allow pairing" },
                    hint: if pairing { "On the other computer, choose Add environment below the main window's tree and pick this computer or type its address." } else { "Accept pairing requests for the next ten minutes." },
                    if pairing {
                        button { r#type: "button", onclick: {
                            let here = here.clone();
                            move |_| { let here = here.clone(); spawn(async move { error.set(here.close_pairing().await.err()); }); }
                        }, "Stop" }
                    } else {
                        button { r#type: "button", class: "primary", onclick: {
                            let here = here.clone();
                            move |_| { let here = here.clone(); notice.set(None); spawn(async move { error.set(here.open_pairing().await.err()); }); }
                        }, "Allow pairing" }
                    }
                }
                if pairing && asking.read().is_empty() {
                    SettingRow { label: "No request yet", hint: "A request shows here as soon as the other computer asks." }
                }
                for request in asking.read().clone() {
                    SettingRow { key: "{request.id}", label: named(&request.name, request.this_computer), hint: asking_hint(&request),
                        // Kept on every key, so Pair is enabled as soon as a code is typed.
                        input { class: "settings-command", r#type: "text", aria_label: "Code shown on {request.name}", placeholder: "Code", autocomplete: "off", spellcheck: false,
                            value: codes.read().get(&request.id).cloned().unwrap_or_default(),
                            oninput: move |e| { codes.write().insert(request.id, e.value()); },
                        }
                        button { r#type: "button", onclick: {
                            let here = here.clone();
                            move |_| { let here = here.clone(); spawn(async move { error.set(here.refuse(request.id).await.err()); }); }
                        }, "Refuse" }
                        button { r#type: "button", class: "primary", disabled: codes.read().get(&request.id).is_none_or(|code| code.trim().is_empty()), onclick: {
                            let here = here.clone();
                            let name = request.name.clone();
                            move |_| {
                                let (here, name) = (here.clone(), name.clone());
                                let code = codes.peek().get(&request.id).cloned().unwrap_or_default();
                                error.set(None);
                                spawn(async move {
                                    match here.approve(request.id, &code).await {
                                        Ok(()) => notice.set(Some(format!("Paired {name}. Pairing is closed now; allow it again to pair another computer."))),
                                        Err(why) => error.set(Some(format!("Not paired: {why}. If {name} still asks, it shows a new code."))),
                                    }
                                    codes.write().remove(&request.id);
                                });
                            }
                        }, "Pair" }
                    }
                }
            }
        }
        SettingGroup { title: "Paired computers",
            if clients.read().is_empty() {
                SettingRow { label: "None yet", hint: "" }
            }
            for client in clients.read().clone() {
                SettingRow { key: "{client.fingerprint}", label: named(&client.name, client.this_computer), hint: format!("Paired {} · Key {}", since(client.paired_at, now()), short(&client.fingerprint)),
                    button { r#type: "button", class: "danger", onclick: {
                        let here = here.clone();
                        let client = client.clone();
                        move |_| {
                            let (here, client) = (here.clone(), client.clone());
                            spawn(async move {
                                match here.revoke(&client.fingerprint).await {
                                    Ok(()) => notice.set(Some(format!("{} can no longer connect.", client.name))),
                                    Err(why) => error.set(Some(why)),
                                }
                            });
                        }
                    }, "Revoke" }
                }
            }
        }
        if let Some(Some(now)) = &*serving.read() {
            SettingGroup { title: "How to reach it",
                SettingRow { label: "Addresses", hint: "Type one of these on the other computer, or find this computer on the local network.",
                    span { class: "serving-detail", "{now.addresses.join(\", \")}" }
                }
                SettingRow { label: "This computer's key", hint: "Paired computers check it every time they connect.",
                    code { class: "serving-detail", title: "{now.server}", "{short(&now.server)}" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pairing_is_dated_by_how_long_ago_it_was() {
        let now = 10 * 24 * 60 * 60;
        assert_eq!(since(now - 5, now), "just now");
        assert_eq!(since(now - 60, now), "1 minute ago");
        assert_eq!(since(now - 3 * 60 * 60, now), "3 hours ago");
        assert_eq!(since(now - 2 * 24 * 60 * 60, now), "2 days ago");
        assert_eq!(since(now + 5, now), "just now", "a clock set back");
    }

    #[test]
    fn this_computer_reaching_its_own_gateway_is_marked() {
        assert_eq!(named("mac", true), "mac (this computer)");
        assert_eq!(named("mac", false), "mac");
    }

    #[test]
    fn a_fingerprint_is_shown_by_its_start() {
        assert_eq!(short("0123456789abcdef0123"), "0123456789abcdef");
        assert_eq!(short("0123"), "0123");
    }
}
