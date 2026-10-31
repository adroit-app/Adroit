//! The keys and menu entries the window takes: the hidden text area that
//! has the keys, what it receives turned into messages with what the window
//! knows of its programs, the menu bar's entries and Command keys, and an
//! action run where the keys are now.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adroit_core::actions::Action;
use adroit_core::console::reader::Edge;
use adroit_core::console::{Probe, ReaderAt};
use adroit_core::content::{Area, Tab};
use adroit_core::keymap::MENU_KEYS;
use adroit_core::msg::Msg;
use adroit_core::overlay;
use adroit_model::Key;
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::shell::Ui;
use crate::{keys, menu_bar, nav, settings_window};

/// How long the window waits before it runs [`INPUT`] again, once the keys
/// it listened for are lost.
const RESTART_INPUT_AFTER: Duration = Duration::from_millis(200);

/// How often the window looks whether its page has its keys.
const LOOK_FOR_KEYS_EVERY: Duration = Duration::from_millis(500);

/// Keeps the hidden text area focused, but while a text field has the keys,
/// and turns what it receives into messages. Text is taken from
/// `beforeinput` (from `keydown` while the text area is read-only), pastes
/// from `paste` and
/// input-method commits from `compositionend`, each stopped in JavaScript
/// before the text area changes: a default prevented from Rust arrives too late
/// once the event has crossed to the webview, and the stray character then
/// lands out of order. Other keys have their default stopped here for the same
/// reason; Rust reads them from `keydown`. Command keys the menu bar holds keep
/// their default, so the menu runs them, as it does copy, paste and quit:
/// `window.adroitHeld` lists them as [`menu_bar::left_to_menu`] names them.
const INPUT: &str = include_str!("../assets/js/input.js");

impl Ui {
    /// What the window knows of its programs now, which the view state
    /// routes a key and runs an action by.
    fn probe(&self) -> Probe {
        let pane = self.active_pane();
        let reader = self.consoles.reader.peek().clone().map(|r| {
            let screen = r.state().screen();
            ReaderAt {
                normal: r.state().mode() == "normal",
                edge: Edge {
                    line: self.consoles.reader_view.peek().0 + screen.cursor.map_or(0, |c| c.row),
                    lines: self.consoles.read_rows.peek().0,
                    page: screen.rows,
                },
            }
        });
        let inserting = self
            .consoles
            .nvim
            .peek()
            .as_ref()
            .is_some_and(|n| n.state().mode().starts_with("insert"));
        Probe {
            pane: pane.is_some(),
            scrolls_itself: pane.as_ref().is_some_and(|p| p.takes_mouse()),
            reader,
            inserting,
            rows: self.local.console_size.peek().rows,
        }
    }

    /// Run an action where the keys are now: what a binding runs, and what a
    /// click on a row's or a tab's menu runs. `key` is the key that ran it,
    /// which some actions pass on; a click has none.
    pub(crate) fn perform(&self, action: Action, key: Option<Key>) {
        let probe = Box::new(self.probe());
        self.app.dispatch(Msg::Perform { action, key, probe });
    }

    /// A key the hidden text area received: to what is drawn over the
    /// window while something is, else routed by the view state.
    fn handle_key(&self, key: Key) {
        let app = self.app;
        if key.key == "Process"
            || key.key == "Dead"
            || key.key == "Unidentified"
            || keys::held_alone(&key)
        {
            return;
        }
        // What is drawn over the window holds the keys while it is.
        if let Some(holder) = app.held().first().copied() {
            keys::trace(|| format!("to the {holder:?}"));
            app.dispatch(Msg::HeldKey(key));
            return;
        }
        let area = app.focus();
        let probe = self.probe();
        keys::trace(|| format!("area={area:?} {probe:?}"));
        app.dispatch(Msg::Key {
            key,
            probe: Box::new(probe),
        });
    }
}

/// Take the keys, the text and the menu bar's entries for as long as the
/// window is open.
pub(crate) fn use_input(ui: &Ui) {
    let app = ui.app;
    // What a menu entry runs, once the view state has selected what it was
    // opened on.
    use_hook({
        let ui = RefCell::new(ui.clone());
        move || {
            app.hand_perform(move |action, key| {
                if let Ok(ui) = ui.try_borrow_mut() {
                    ui.perform(action, key);
                }
            })
        }
    });
    use_menu_bar(ui);
    crate::settings_window::use_asks(app);
    // Whether the page has the keys, and whether an input method composes
    // there, as it last told.
    let page_focused = use_hook(|| Rc::new(Cell::new(true)));
    let composing = use_hook(|| Rc::new(Cell::new(false)));
    // The window made key again gives the keys to its page, whatever in the
    // window took them meanwhile: AppKit sends them to the first responder,
    // and with anything but the page there no key reaches the window. Not
    // while a composition is open, which a new first responder ends.
    dioxus::desktop::use_wry_event_handler({
        let desktop = dioxus::desktop::window();
        let composing = composing.clone();
        move |event, _| {
            use dioxus::desktop::tao::event::Event;
            if let Event::WindowEvent {
                window_id,
                event: dioxus::desktop::WindowEvent::Focused(on),
                ..
            } = event
                && *window_id == desktop.window.id()
            {
                app.dispatch(Msg::WindowFocused(*on));
                if *on && !composing.get() {
                    keys::trace(|| "window made key: the page takes the keys");
                    let _ = desktop.webview.focus();
                }
            }
        }
    });
    use_hook({
        let ui = ui.clone();
        let page_focused = page_focused.clone();
        move || {
            spawn(listen(ui, page_focused, composing));
        }
    });
    // A window that has the keys while its page has not, past a moment, gives
    // them back to the page: nothing else in the window uses them, and with
    // them elsewhere every key is lost while the window looks as it should.
    use_hook(move || {
        let desktop = dioxus::desktop::window();
        spawn(async move {
            let mut watch = keys::Watch::default();
            loop {
                tokio::time::sleep(LOOK_FOR_KEYS_EVERY).await;
                let key = desktop.window.is_focused();
                if watch.look(key, page_focused.get(), desktop.webview.is_devtools_open()) {
                    keys::trace(|| "the page lost the keys of the window: giving them back");
                    let _ = desktop.webview.focus();
                }
            }
        });
    });
}

/// Keys, text, pastes and input-method commits from the hidden text area.
/// Keys with a modifier and keys that type nothing come as keys; plain
/// characters come as text.
async fn listen(ui: Ui, page_focused: Rc<Cell<bool>>, composing: Rc<Cell<bool>>) {
    let app = ui.app;
    let run = || {
        document::eval(&format!(
            "window.adroitHeld ??= {MENU_KEYS:?};\nwindow.adroitDebugKeys = {};\n{INPUT}",
            *keys::TRACED
        ))
    };
    let mut input = run();
    let desktop = dioxus::desktop::window();
    let mut unanswered = keys::Unanswered::default();
    loop {
        let message = match keys::after(input.recv::<keys::Message>().await) {
            keys::Next::Handle(message) => message,
            keys::Next::Skip => {
                keys::trace(|| "passed over a message that cannot be read");
                continue;
            }
            keys::Next::Restart => {
                keys::trace(|| "the keys were lost: running the script again");
                // A script that cannot keep its channel is not run again at
                // once, over and over.
                tokio::time::sleep(RESTART_INPUT_AFTER).await;
                input = run();
                continue;
            }
        };
        if let Some(key) = message.key() {
            keys::trace(|| format!("received {key:?}"));
            ui.handle_key(key);
            continue;
        }
        match message {
            keys::Message::Trace { text } => keys::trace(|| format!("js {text}")),
            keys::Message::Focused { on } => page_focused.set(on),
            keys::Message::Composing { on } => composing.set(on),
            keys::Message::Dropped { answered } => match unanswered.dropped(answered) {
                keys::Lost::Nothing => {
                    keys::trace(|| format!("dropped as composing, answered={answered}"))
                }
                keys::Lost::Ended(n) => lost(&format!(
                    "{n} keys dropped as composing went unanswered by the input method"
                )),
                keys::Lost::Reset(n) => {
                    lost(&format!(
                        "{n} keys in a row dropped as composing went unanswered by the input method: resetting it"
                    ));
                    composing.set(false);
                    reset_input_method(&desktop);
                }
            },
            keys::Message::Paste { text } => {
                keys::trace(|| format!("text {text:?} pasted=true"));
                app.dispatch(Msg::Text { text, pasted: true });
            }
            keys::Message::Image { mime, data } => {
                app.dispatch(Msg::ImagePasted { mime, data });
            }
            keys::Message::Text { text } if text.chars().count() == 1 => {
                ui.handle_key(Key::new(&text));
            }
            keys::Message::Text { text } => {
                keys::trace(|| format!("text {text:?} pasted=false"));
                app.dispatch(Msg::Text {
                    text,
                    pasted: false,
                });
            }
            keys::Message::Key { .. } | keys::Message::Lost => {}
        }
    }
}

/// Tell of keys the input method lost, on the standard error and in the
/// lost keys' log, whether keys are traced or not.
fn lost(what: &str) {
    eprintln!("[keys] {what}");
    let _ = keys::record(&keys::lost_log(), what, keys::LOST_LOG_LIMIT);
}

/// Reset the input method, which stopped answering the keys: the page empties
/// its text area and gives it focus again, the composition's marked text is
/// discarded, and the page loses first responder and takes it back.
fn reset_input_method(desktop: &dioxus::desktop::DesktopContext) {
    document::eval("window.adroitInput?.reset?.();");
    #[cfg(target_os = "macos")]
    resign_first_responder(desktop);
    let _ = desktop.webview.focus();
}

#[cfg(target_os = "macos")]
fn resign_first_responder(desktop: &dioxus::desktop::DesktopContext) {
    use dioxus::desktop::tao::platform::macos::WindowExtMacOS;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSTextInputContext, NSWindow};
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if let Some(context) = NSTextInputContext::currentInputContext(mtm) {
        context.discardMarkedText();
    }
    // SAFETY: tao hands out the window's NSWindow, alive as long as
    // `desktop`.
    let window = unsafe { &*(desktop.window.ns_window() as *const NSWindow) };
    window.makeFirstResponder(None);
}

/// The menu bar shows the Command keys in force, and runs what an entry
/// names where the keys are now.
fn use_menu_bar(ui: &Ui) {
    let app = ui.app;
    let keymap = use_memo(move || app.chrome.read().keymap.clone());
    // The page leaves the menu bar's keys to it, so a key runs once, from
    // the menu.
    use_effect(move || {
        let map = keymap();
        menu_bar::show_shortcuts(&map);
        let held = serde_json::to_string(&menu_bar::left_to_menu(&map)).unwrap_or_default();
        document::eval(&format!("window.adroitHeld = {held};"));
    });
    // The page learns which Option keys count as Alt, now and as the settings
    // change.
    let as_alt = use_memo(move || app.config.read().settings.console.option_as_alt.name());
    use_effect(move || {
        let as_alt = as_alt();
        document::eval(&format!("window.adroitOptionAsAlt = '{as_alt}';"));
    });
    // Run what a menu bar entry names where the keys are now, as its key
    // would: not behind the picker, a dialog or a name being typed, which
    // hold the keys. The entries act on this window whichever of adroit's is
    // in front, and bring it forward to show what they did.
    dioxus::desktop::use_muda_event_handler({
        let ui = ui.clone();
        let desktop = dioxus::desktop::window();
        move |event| {
            let Some(action) = menu_bar::action_of(&event.id().0) else {
                return;
            };
            // Closing acts on the window in front, as macOS's own Close does,
            // so it does not reach behind Settings to a tab.
            if matches!(action, Action::TabClose | Action::WindowClose)
                && (settings_window::close_in_front()
                    || crate::environment_manager::close_in_front())
            {
                return;
            }
            if overlay::holds_menu_bar(&app.held()) {
                return;
            }
            if action != Action::SettingsOpen {
                desktop.window.set_focus();
            }
            app.dispatch(Msg::KeysHelpClose);
            app.dispatch(Msg::MenuClose);
            app.dispatch(Msg::SequenceDropped);
            ui.perform(action, None);
        }
    });
}

/// How the session the focused window shows stands to the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Answering {
    /// The focused window shows a session.
    pub session_shown: bool,
    /// That session shows a dialog of its agent's own.
    pub in_dialog: bool,
    /// Every key answers it: a dialog, or the choice it waits on.
    pub answering: bool,
}

/// How the session the focused window shows stands to the keys, read so a
/// component follows it.
pub(crate) fn answering(app: AppCtx) -> Answering {
    let active = app.content.read().active().cloned();
    let session_shown = matches!(active, Some(Tab::Session(_, _)));
    let in_dialog = session_shown && app.console.read().dialog;
    let answering = match &active {
        Some(Tab::Session(environment, id)) => {
            let hosts = app.hosts.read();
            let tree = adroit_core::tree::listed(hosts.work(environment));
            in_dialog || nav::find(tree, id).is_some_and(|(_, s)| s.state.takes_every_key())
        }
        _ => false,
    };
    Answering {
        session_shown,
        in_dialog,
        answering,
    }
}

/// The hidden text area that has the keys, which takes text, as an input
/// method composes it, only while what has the keys takes text.
#[component]
pub(crate) fn KeysField() -> Element {
    let app = use_context::<AppCtx>();
    let Answering {
        session_shown,
        answering,
        ..
    } = answering(app);
    let passing = app.console.read().direct;
    let (mode, selected_image) = {
        let prompt = app.prompt.read();
        (prompt.mode, prompt.selected.clone())
    };
    let held = overlay::holding(&app.overlays.read(), &app.picker.read(), &app.chrome.read());
    let takes_text = match app.content.read().focus {
        // A dialog's text is typed in its own field.
        _ if overlay::takes_text_away(&held) => false,
        Area::Console => passing || answering || !session_shown,
        Area::Prompt => {
            answering || (selected_image.is_none() && matches!(mode.0, "insert" | "cmd"))
        }
        Area::Sidebar | Area::Details => false,
    };
    rsx! {
        // The script makes it read-only, once no composition is open.
        textarea {
            id: "adroit-keys",
            autofocus: true,
            "data-takes-text": if takes_text { "yes" } else { "no" },
        }
    }
}
