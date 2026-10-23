//! What is drawn over the window: the keys help, a menu opened on a row, a
//! tab or a link, a question to answer, and the banner about the sessions'
//! daemon. They only draw: the view state holds them.

use adroit_core::msg::Msg;
use adroit_core::overlay::context_menu::{self, Target};
use adroit_core::tree::listed;
use dioxus::prelude::*;

use crate::actions::Action;
use crate::bridge::AppCtx;
use crate::components::Keys;
use crate::icons::{Icon, IconView};
use crate::keymap::Context;
use crate::keys_help;
use crate::keys_help_view::KeysHelpView;
use crate::nav::{self, Item};
use crate::standing::{self, Standing};
use adroit_core::content::Tab;

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
    drop(chrome);
    rsx! {
        if let Some(groups) = groups {
            KeysHelpView {
                groups,
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
    let (x, y, target) = (menu.x, menu.y, menu.target);
    let session = match &target {
        Target::Row(Item::Session(id) | Item::Waiting(id)) | Target::Tab(Tab::Session(id)) => {
            let host = app.host.read();
            nav::find(listed(host.work.as_ref()), id).map(|(_, s)| s.clone())
        }
        _ => None,
    };
    // The keys that work wherever the keys are come first.
    let contexts: &[Context] = match target {
        Target::Row(_) => &[Context::Global, Context::Tree],
        Target::Tab(_) => &[Context::Global, Context::Console, Context::Tree],
        Target::Link { .. } => &[Context::Details, Context::Tree, Context::Global],
    };
    // Each entry with whether a separator goes above it.
    let entries: Vec<(Action, &str, Option<String>, bool)> =
        context_menu::groups(&target, session.as_ref())
            .into_iter()
            .enumerate()
            .flat_map(|(g, group)| {
                group
                    .into_iter()
                    .enumerate()
                    .map(move |(i, (a, label))| (a, label, g > 0 && i == 0))
            })
            .map(|(a, label, apart)| {
                let keys = keymap.keys_in(contexts, &a);
                (a, label, keys, apart)
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
                for (action , label , keys , apart) in entries {
                    {
                        let target = target.clone();
                        let act = action.clone();
                        rsx! {
                            Fragment { key: "{action}",
                                if apart {
                                    div { class: "context-menu-sep" }
                                }
                                div {
                                    class: "context-menu-item",
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
    rsx! {
        div { class: "dialog-scrim",
            div { class: "dialog",
                div { class: "dialog-title", IconView { icon, size: 16 } "{title}" }
                div { "{body}" }
                div { class: "dialog-actions",
                    button { class: "button", onclick: move |_| app.dispatch(Msg::ConfirmAnswer(false)),
                        kbd { "n" } "Cancel"
                    }
                    button { class: if ask.destroys() { "button danger" } else { "button primary" },
                        onclick: move |_| app.dispatch(Msg::ConfirmAnswer(true)),
                        kbd { "y" } "{act}"
                    }
                }
            }
        }
    }
}

/// The banner offering to restart a daemon this window cannot work with.
#[component]
pub(crate) fn DaemonBanner() -> Element {
    let app = use_context::<AppCtx>();
    let Some(note) = app.overlays.read().banner.shown().map(str::to_string) else {
        return rsx! {};
    };
    rsx! {
        div { class: "daemon-banner",
            IconView { icon: Icon::ServerBolt }
            span { class: "daemon-banner-title", "This window cannot work with the sessions' daemon" }
            span { class: "daemon-banner-note", title: "{note}", "{note} Restarting it ends every session it holds." }
            span { class: "flex-spacer" }
            button { class: "button", onclick: move |_| app.dispatch(Msg::BannerClose), "Later" }
            button { class: "button danger",
                onclick: move |_| app.dispatch(Msg::Confirm(Box::new(adroit_core::overlay::confirm::Ask::RestartDaemon))),
                "Restart…"
            }
        }
    }
}

/// What the banner says of a daemon this window cannot work with, as
/// `standing` has it; `None` while it can, or none answers.
pub(crate) fn old_daemon_note(standing: &Standing) -> Option<String> {
    match standing {
        Standing::Incompatible(Some(said)) => Some(format!(
            "It runs adroit {said}, and this window adroit {}.",
            standing::build()
        )),
        Standing::Incompatible(None) => {
            Some("It runs a build of adroit too old to say which.".into())
        }
        Standing::Absent | Standing::Compatible => None,
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
    fn a_daemon_this_window_cannot_work_with_is_named_beside_this_window_s_build() {
        let note = old_daemon_note(&Standing::Incompatible(Some(
            "0.1.0 (v0.1.0-3-gabc)".into(),
        )))
        .unwrap();
        assert!(note.contains("0.1.0 (v0.1.0-3-gabc)"), "{note}");
        assert!(note.contains(&standing::build()), "{note}");
    }

    #[test]
    fn a_daemon_too_old_to_say_its_versions_is_shown_too() {
        assert!(old_daemon_note(&Standing::Incompatible(None)).is_some());
    }

    #[test]
    fn a_daemon_this_window_can_work_with_or_none_at_all_shows_no_banner() {
        assert_eq!(old_daemon_note(&Standing::Compatible), None);
        assert_eq!(old_daemon_note(&Standing::Absent), None);
    }
}
