//! The foot of the window: the status bar with the mode the keys are in and
//! the keys that matter where the keys are, from the bindings in force.

use adroit_core::actions::Action;
use adroit_core::console::route::{self, Footing};
use adroit_core::content::Area;
use adroit_core::keymap::{Chord, Context};
use dioxus::prelude::*;

use super::Ui;
use super::status_bar::StatusBar;
use crate::components::{Hint, KeyHint};
use crate::input::{Answering, answering};

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
    let (reading, passing, repeating) = {
        let console = app.console.read();
        (console.reading, console.direct, console.repeat.is_some())
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
        repeating,
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
                KeyHint { keys: k, does: "move" }
            }
            if let Some(k) = k_waiting_next {
                KeyHint { keys: k, does: "next waiting" }
            }
            if area == Area::Sidebar && app.tree.read().filtering {
                KeyHint { keys: "Enter", does: "keep the filter" }
                KeyHint { keys: "Esc", does: "clear it" }
            }
            if area == Area::Prompt && in_dialog {
                Hint { "every key goes to the session's dialog" }
                Hint { "the prompt is back once it closes" }
            } else if area == Area::Console && in_dialog && !passing {
                Hint { "every key goes to the session's dialog" }
                Hint { "the window's moves leave" }
            } else if area == Area::Prompt && answering {
                Hint { "every key answers the session" }
                Hint { "the prompt is back once it is answered" }
            } else if area == Area::Console && answering && !passing {
                Hint { "every key answers the session" }
                Hint { "the window's moves leave" }
            } else if area == Area::Console && passing {
                KeyHint { keys: "Esc", does: "stop passing keys" }
                KeyHint { keys: "Ctrl-c", does: "send Esc" }
            } else if area == Area::Console && session_shown && reading {
                KeyHint { keys: "v V", does: "select" }
                KeyHint { keys: "y", does: "copy" }
                KeyHint { keys: "/", does: "search" }
                KeyHint { keys: "q", does: "live" }
                if let Some(k) = k_write.clone() {
                    KeyHint { keys: k, does: "write" }
                }
            } else if area == Area::Console && session_shown {
                KeyHint { keys: "j k", does: "read" }
                if let Some(k) = k_write.clone() {
                    KeyHint { keys: k, does: "write" }
                }
                if let Some(k) = k_escape {
                    KeyHint { keys: k, does: "send Esc" }
                }
                if let Some(k) = k_every {
                    KeyHint { keys: k, does: "pass every key" }
                }
            } else if area == Area::Console {
                if let Some(k) = k_ctrl_q {
                    KeyHint { keys: k, does: "send Ctrl-q" }
                }
            }
        }
    }
}
