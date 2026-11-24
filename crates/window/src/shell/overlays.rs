//! What is drawn over the window: the keys help, a menu opened on a row, a
//! tab or a link, a question to answer, and the banner about the sessions'
//! daemon. They only draw: the view state holds them.

use adroit_core::msg::Msg;
use adroit_core::overlay::confirm::Ask;
use adroit_core::overlay::context_menu::{Reach, Target};
use adroit_core::overlay::{self, MenuEntry, hints};
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::components::{Button, Dialog, Keys, Spacer};
use crate::icons::{Icon, IconView};
use crate::keymap::Context;
use crate::keys_help;
use crate::keys_help_view::KeysHelpView;
use crate::nav::Item;
use adroit_protocol::TerminalHostStanding;

/// The keys help for the areas the chrome holds it open for, with the
/// bindings it holds.
#[component]
pub(crate) fn KeysHelp() -> Element {
    let app = use_context::<AppCtx>();
    let chrome = app.chrome.read();
    let groups = chrome
        .keys_help
        .as_ref()
        .map(|here| keys_help::groups(&chrome.keymap, here));
    let hints = hints::keys_help(&chrome.keymap);
    drop(chrome);
    rsx! {
        if let Some(groups) = groups {
            KeysHelpView {
                groups,
                hints,
                onclose: move |_| app.dispatch(Msg::KeysHelpClose),
            }
        }
    }
}

/// The menu a right click opened, each entry with the keys that run it
/// there.
#[component]
pub(crate) fn ContextMenu() -> Element {
    let app = use_context::<AppCtx>();
    let keymap = app.chrome.read().keymap.clone();
    let Some(menu) = app.overlays.read().menu.clone() else {
        return rsx! {};
    };
    let listed = overlay::menu_entries(&menu, &app.hosts.read());
    let (x, y, target, cursor) = (menu.x, menu.y, menu.target, menu.cursor);
    // The keys that work wherever the keys are come first.
    let contexts: &[Context] = match target {
        Target::Row(_) | Target::Start(_) => &[Context::Global, Context::Tree],
        Target::Tab(_) => &[Context::Global, Context::Console, Context::Tree],
        Target::Link { .. } => &[Context::Details, Context::Tree, Context::Global],
    };
    let entries: Vec<(MenuEntry, Option<String>)> = listed
        .into_iter()
        .map(|e| {
            let keys = keymap.keys_in(contexts, &e.action);
            (e, keys)
        })
        .collect();
    rsx! {
        div {
            class: "context-menu-shade",
            onclick: move |_| app.dispatch(Msg::MenuClose),
            oncontextmenu: move |e: Event<MouseData>| {
                e.prevent_default();
                app.dispatch(Msg::MenuClose);
            },
            div {
                class: "context-menu",
                style: placed(x, y),
                onclick: move |e| e.stop_propagation(),
                for (i , (MenuEntry { action, label, apart }, keys)) in entries.into_iter().enumerate() {
                    {
                        let target = target.clone();
                        let act = action.clone();
                        rsx! {
                            Fragment { key: "{action}",
                                if apart {
                                    div { class: "context-menu-sep" }
                                }
                                div {
                                    class: if cursor == Some(i) { "context-menu-item selected" } else { "context-menu-item" },
                                    onclick: move |_| app.dispatch(Msg::MenuRun { target: target.clone(), action: act.clone() }),
                                    span { class: "context-menu-icon",
                                        if let Some(icon) = action.icon() {
                                            IconView { icon, size: 15 }
                                        }
                                    }
                                    span { class: "context-menu-label", "{label}" }
                                    if let Some(k) = keys {
                                        span { class: "context-menu-keys", Keys { keys: k } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What a right click on a row or a tab does: open the menu of `target`
/// where the pointer is, in place of the page's own.
pub(crate) fn menu_opener(app: AppCtx, target: Target) -> EventHandler<MouseEvent> {
    EventHandler::new(move |e: MouseEvent| {
        e.prevent_default();
        let at = e.client_coordinates();
        let reach = match &target {
            Target::Row(Item::Environment(environment)) => {
                crate::connection::reach_of(&app, environment)
            }
            _ => Reach::Here,
        };
        app.dispatch(Msg::MenuOpen {
            x: at.x,
            y: at.y,
            target: target.clone(),
            reach,
        });
    })
}

/// Where a menu opened at `x`, `y` is drawn: there, or moved back by as
/// much of its own size as would pass the window's right or bottom edge.
/// Its size is known only once it is laid out, so the move is a translate,
/// whose percentages are of the menu itself.
fn placed(x: f64, y: f64) -> String {
    let dx = format!("min(0px, calc(100vw - {x}px - 100% - 4px))");
    let dy = format!("max(calc(4px - {y}px), min(0px, calc(100vh - {y}px - 100% - 4px)))");
    format!("left:{x}px;top:{y}px;translate:{dx} {dy}")
}

/// The question the window holds the keys for until it is answered.
#[component]
pub(crate) fn ConfirmDialog() -> Element {
    let app = use_context::<AppCtx>();
    let Some(ask) = app.overlays.read().confirm.clone() else {
        return rsx! {};
    };
    let (icon, title, body, act) = ask.dialog();
    let (no, yes) = hints::answers(&app.chrome.read().keymap);
    // A name pasted that is taken is overwritten, renamed or skipped.
    let renames = ask.renames();
    let (rename, overwrite) = hints::pasting_answers(&app.chrome.read().keymap);
    let yes = if renames { overwrite.or(yes) } else { yes };
    let relay = match &ask {
        Ask::Relay { relay, .. } => Some((relay.clone(), ask.relay_to())),
        _ => None,
    };
    let decline = hints::decline(&app.chrome.read().keymap);
    let kind = if relay.is_some() { "relay-dialog" } else { "" };
    rsx! {
        Dialog { kind, icon, title, onclose: move |_| app.dispatch(Msg::ConfirmAnswer(false)),
            div { "{body}" }
            if let Some((relay, chosen)) = relay.clone() {
                RelayWords { relay, chosen }
            }
            div { class: "dialog-actions",
                Button { onclick: move |_| app.dispatch(Msg::ConfirmAnswer(false)),
                    if let Some(keys) = no {
                        Keys { keys }
                    }
                    if renames { "Skip" } else if relay.is_some() { "Later" } else { "Cancel" }
                }
                if relay.is_some() {
                    Button { onclick: move |_| app.perform(adroit_core::actions::Action::RelayDecline, None),
                        if let Some(keys) = decline {
                            Keys { keys }
                        }
                        "Decline"
                    }
                }
                if renames {
                    Button { onclick: move |_| app.perform(adroit_core::actions::Action::PasteRename, None),
                        if let Some(keys) = rename {
                            Keys { keys }
                        }
                        "Rename"
                    }
                }
                Button { kind: if ask.destroys() { "danger" } else { "primary" }, disabled: !ask.answerable(),
                    onclick: move |_| app.dispatch(Msg::ConfirmAnswer(true)),
                    if let Some(keys) = yes {
                        Keys { keys }
                    }
                    "{act}"
                }
            }
        }
    }
}

/// The words a session proposes to pass on, whole, and the sessions they are
/// to reach, each to leave out or put back.
#[component]
fn RelayWords(relay: adroit_model::Relay, chosen: Vec<adroit_model::SessionCode>) -> Element {
    let app = use_context::<AppCtx>();
    rsx! {
        div { class: "relay-words", "{relay.body}" }
        div { class: "relay-to",
            if relay.to.is_empty() {
                "Every session they were to reach has ended."
            }
            for to in relay.to {
                label { key: "{to.code}",
                    input {
                        r#type: "checkbox",
                        checked: chosen.contains(&to.code),
                        onchange: {
                            let code = to.code.clone();
                            move |_| app.dispatch(Msg::RelayToggle(code.clone()))
                        },
                    }
                    "{to.name} ({to.code})"
                }
            }
        }
    }
}

/// The banner saying a session proposes to pass words on as the user's.
#[component]
pub(crate) fn RelayBanner() -> Element {
    let app = use_context::<AppCtx>();
    let shown = {
        let hosts = app.hosts.read();
        let overlays = app.overlays.read();
        let proposed = overlay::relay::relay_banner(&hosts, &overlays.relays);
        proposed
            .first()
            .map(|(_, relay)| relay_title(relay, proposed.len() - 1))
    };
    let Some(title) = shown else {
        return rsx! {};
    };
    rsx! {
        div { class: "daemon-banner",
            IconView { icon: Icon::MessageUp }
            span { class: "daemon-banner-title", "{title}" }
            span { class: "daemon-banner-note", "Nothing is sent until you approve it." }
            Spacer {}
            Button { onclick: move |_| app.dispatch(Msg::RelayLater), "Later" }
            Button { kind: "primary", onclick: move |_| app.dispatch(Msg::RelayReview), "Review…" }
        }
    }
}

/// What the banner says of `relay`, with `more` proposed besides.
fn relay_title(relay: &adroit_model::Relay, more: usize) -> String {
    let to = match relay.to.as_slice() {
        [one] => one.name.clone(),
        to => format!("{} sessions", to.len()),
    };
    let more = match more {
        0 => String::new(),
        more => format!(" (and {more} more)"),
    };
    format!(
        "{} proposes to pass your words on to {to}{more}",
        relay.from_name
    )
}

/// The banner offering to restart a daemon this window cannot work with,
/// naming its environment when that is not the window's own.
#[component]
pub(crate) fn DaemonBanner() -> Element {
    let app = use_context::<AppCtx>();
    let shown = app
        .overlays
        .read()
        .banner
        .shown()
        .map(|(environment, note)| (environment.clone(), note.to_string()));
    let Some((environment, note)) = shown else {
        return rsx! {};
    };
    let title = if environment == app.environment() {
        "This window cannot work with the sessions' daemon".to_string()
    } else {
        format!(
            "This window cannot work with the sessions' daemon of {}",
            app.environment_name(&environment)
        )
    };
    rsx! {
        div { class: "daemon-banner",
            IconView { icon: Icon::ServerBolt }
            span { class: "daemon-banner-title", "{title}" }
            span { class: "daemon-banner-note", title: "{note}", "{note} Restarting it ends every session it holds." }
            Spacer {}
            Button { onclick: move |_| app.dispatch(Msg::BannerClose), "Later" }
            Button { kind: "danger",
                onclick: move |_| app.dispatch(Msg::Confirm(Box::new(adroit_core::overlay::confirm::Ask::RestartDaemon(environment.clone())))),
                "Restart…"
            }
        }
    }
}

/// The banner offering to bring back what a sessions' daemon took with it
/// when it went, naming its environment when that is not the window's own.
#[component]
pub(crate) fn InterruptedBanner() -> Element {
    let app = use_context::<AppCtx>();
    let shown = {
        let config = app.config.read();
        let hosts = app.hosts.read();
        let overlays = app.overlays.read();
        adroit_core::overlay::interrupted_banner(&config.settings, &hosts, &overlays.banner)
            .map(|(environment, waiting)| (environment, interrupted_title(&waiting)))
    };
    let Some((environment, what)) = shown else {
        return rsx! {};
    };
    let title = if environment == app.environment() {
        format!("The sessions' daemon went, taking {what}")
    } else {
        format!(
            "The sessions' daemon of {} went, taking {what}",
            app.environment_name(&environment)
        )
    };
    rsx! {
        div { class: "daemon-banner",
            IconView { icon: Icon::Restore }
            span { class: "daemon-banner-title", "{title}" }
            span { class: "daemon-banner-note", "Agents resume their conversations; shells open again where they ran." }
            Spacer {}
            Button { onclick: move |_| app.dispatch(Msg::InterruptedLater), "Later" }
            Button { onclick: move |_| app.dispatch(Msg::LetGoAll), "Let Go" }
            Button {
                onclick: move |_| app.dispatch(Msg::InterruptedOpen(environment.clone())),
                "Choose…"
            }
            Button { kind: "primary", onclick: move |_| app.dispatch(Msg::BringBackAll), "Bring Back" }
        }
    }
}

/// What the banner says a daemon took: its sessions and its shells, counted.
fn interrupted_title(waiting: &[&adroit_model::Interrupted]) -> String {
    let sessions = waiting.iter().filter(|i| i.is_session()).count();
    let terminals = waiting.len() - sessions;
    let counted = |n: usize, one: &str, many: &str| match n {
        0 => None,
        1 => Some(format!("1 {one}")),
        n => Some(format!("{n} {many}")),
    };
    [
        counted(sessions, "session", "sessions"),
        counted(terminals, "terminal", "terminals"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" and ")
}

/// What the banner says of a daemon this window cannot work with, as
/// `standing` has it; `None` while it can, or none answers.
pub(crate) fn old_daemon_note(standing: &TerminalHostStanding) -> Option<String> {
    match standing {
        TerminalHostStanding::Old { build: Some(said) } => Some(format!(
            "It runs adroit {said}, and this window adroit {}.",
            crate::build::build()
        )),
        TerminalHostStanding::Old { build: None } => {
            Some("It runs a build of adroit too old to say which.".into())
        }
        TerminalHostStanding::Absent | TerminalHostStanding::Works => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_opened_near_an_edge_is_moved_back_inside_the_window_by_its_own_size() {
        let style = placed(900.0, 700.0);
        assert!(style.starts_with("left:900px;top:700px;"), "{style}");
        // Shifted left and up by however much of its own width and height
        // would pass the window's right and bottom edges, never past the
        // top-left corner.
        assert!(
            style.contains("min(0px, calc(100vw - 900px - 100% - 4px))"),
            "{style}"
        );
        assert!(
            style.contains("max(calc(4px - 700px), min(0px, calc(100vh - 700px - 100% - 4px)))"),
            "{style}"
        );
    }

    #[test]
    fn the_banner_names_who_proposes_to_pass_words_on_and_to_how_many() {
        let to = |name: &str| adroit_model::RelayTo {
            code: "r4pj".parse().unwrap(),
            name: name.into(),
        };
        let relay = |to| adroit_model::Relay {
            id: adroit_model::MessageId::new(),
            from: "k7f3".parse().unwrap(),
            from_name: "lead".into(),
            to,
            body: "use tabs".into(),
            proposed_ms: 1,
        };
        assert_eq!(
            relay_title(&relay(vec![to("api")]), 0),
            "lead proposes to pass your words on to api"
        );
        assert_eq!(
            relay_title(&relay(vec![to("api"), to("web")]), 1),
            "lead proposes to pass your words on to 2 sessions (and 1 more)"
        );
    }

    #[test]
    fn a_daemon_this_window_cannot_work_with_is_named_beside_this_window_s_build() {
        let note = old_daemon_note(&TerminalHostStanding::Old {
            build: Some("0.1.0 (v0.1.0-3-gabc)".into()),
        })
        .unwrap();
        assert!(note.contains("0.1.0 (v0.1.0-3-gabc)"), "{note}");
        assert!(note.contains(&crate::build::build()), "{note}");
    }

    #[test]
    fn a_daemon_too_old_to_say_its_versions_is_shown_too() {
        assert!(old_daemon_note(&TerminalHostStanding::Old { build: None }).is_some());
    }

    #[test]
    fn a_daemon_this_window_can_work_with_or_none_at_all_shows_no_banner() {
        assert_eq!(old_daemon_note(&TerminalHostStanding::Works), None);
        assert_eq!(old_daemon_note(&TerminalHostStanding::Absent), None);
    }
}
