//! The foot of the window: the status bar with the mode the keys are in and
//! the keys that matter where the keys are, from the bindings in force.

use adroit_core::actions::Action;
use adroit_core::console::route::{self, Footing};
use adroit_core::content::Area;
use adroit_core::keymap::{Chord, Context};
use dioxus::prelude::*;

use super::Ui;
use super::status_bar::StatusBar;
use crate::input::{Answering, answering};
use crate::view::Keys;

#[component]
pub(crate) fn Footer() -> Element {
    let ui = use_context::<Ui>();
    let app = ui.app;
    let area = app.content.read().focus;
    let Answering {
        session_shown,
        in_dialog,
        answering,
    } = answering(app);
    let (reading, passing) = {
        let console = app.console.read();
        (console.reading, console.direct)
    };
    let reader_mode = reading.then(|| {
        let _ = (ui.consoles.reader_frame)();
        (ui.consoles.reader)()
            .map(|r| r.state().mode().to_string())
            .unwrap_or_default()
    });
    let active = app.content.read().active().cloned();
    // The picker holds every key while it is open, the console's too.
    let (mode, class) = Footing {
        picking: app.picker.read().open.is_some(),
        area,
        shown: active.as_ref(),
        direct: passing,
        answering,
        reading: reader_mode.as_deref(),
        prompt: app.prompt.read().mode,
    }
    .mode();
    let km = app.chrome.read().keymap.clone();
    let k_moves = km.keys_for_all(
        Context::Global,
        &[
            Action::WindowLeft,
            Action::WindowDown,
            Action::WindowUp,
            Action::WindowRight,
        ],
    );
    // The footer names the keys that work where the keys are now.
    let k_waiting_next = km.keys_in(route::footer_contexts(area), &Action::WaitingNext);
    let k_write = km.keys_for(Context::Console, &Action::PromptWrite);
    let k_escape = km.keys_for(
        Context::Console,
        &Action::ConsoleSend(Chord::parse("Escape").ok()),
    );
    let k_every = km.keys_for(Context::Console, &Action::ConsolePassEveryKey);
    let k_ctrl_q = km.keys_for(
        Context::Global,
        &Action::ConsoleSend(Chord::parse("Ctrl+q").ok()),
    );
    rsx! {
        StatusBar { mode, class, dialog: in_dialog,
            if let Some(k) = k_moves {
                span { class: "kbd-list", Keys { keys: k } " move" }
            }
            if let Some(k) = k_waiting_next {
                span { class: "kbd-list", Keys { keys: k } " next waiting" }
            }
            if area == Area::Sidebar && app.tree.read().filtering {
                span { class: "kbd-list", kbd { "Enter" } " keep the filter" }
                span { class: "kbd-list", kbd { "Esc" } " clear it" }
            }
            if area == Area::Prompt && in_dialog {
                span { class: "kbd-list", "every key goes to the session's dialog" }
                span { class: "kbd-list", "the prompt is back once it closes" }
            } else if area == Area::Console && in_dialog && !passing {
                span { class: "kbd-list", "every key goes to the session's dialog" }
                span { class: "kbd-list", "the window's moves leave" }
            } else if area == Area::Prompt && answering {
                span { class: "kbd-list", "every key answers the session" }
                span { class: "kbd-list", "the prompt is back once it is answered" }
            } else if area == Area::Console && answering && !passing {
                span { class: "kbd-list", "every key answers the session" }
                span { class: "kbd-list", "the window's moves leave" }
            } else if area == Area::Console && passing {
                span { class: "kbd-list", kbd { "Esc" } " stop passing keys" }
                span { class: "kbd-list", kbd { "Ctrl-c" } " send Esc" }
            } else if area == Area::Console && session_shown && reading {
                span { class: "kbd-list", kbd { "v" } kbd { "V" } " select" }
                span { class: "kbd-list", kbd { "y" } " copy" }
                span { class: "kbd-list", kbd { "/" } " search" }
                span { class: "kbd-list", kbd { "q" } " live" }
                if let Some(k) = k_write.clone() {
                    span { class: "kbd-list", Keys { keys: k } " write" }
                }
            } else if area == Area::Console && session_shown {
                span { class: "kbd-list", kbd { "j" } kbd { "k" } " read" }
                if let Some(k) = k_write.clone() {
                    span { class: "kbd-list", Keys { keys: k } " write" }
                }
                if let Some(k) = k_escape {
                    span { class: "kbd-list", Keys { keys: k } " send Esc" }
                }
                if let Some(k) = k_every {
                    span { class: "kbd-list", Keys { keys: k } " pass every key" }
                }
            } else if area == Area::Console {
                if let Some(k) = k_ctrl_q {
                    span { class: "kbd-list", Keys { keys: k } " send Ctrl-q" }
                }
            }
        }
    }
}
