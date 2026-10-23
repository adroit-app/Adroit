//! A window holding one viewer alone, to try a viewer outside the layout.
//!
//! Keys here are resolved against the `viewer` area's default bindings
//! only: the user's keys file and the keymap's other areas are the main
//! window's.

use std::path::PathBuf;

use dioxus::desktop::{Config as Desktop, LogicalSize, WindowBuilder, WindowCloseBehaviour};
use dioxus::prelude::*;

use super::view::{Command, Viewer};
use crate::keymap::{Chord, Context, Keymap, Step};
use crate::settings;
use adroit_core::viewers::ViewerAction;

/// What a key comes to after the keys pressed before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    Run(ViewerAction),
    /// The keys so far begin a binding.
    Wait,
    Nothing,
}

/// The binding `pending` keys and then `pressed` make, as the keys file
/// writes keys (`Ctrl+d`, `g g`).
pub fn resolve(pending: &[String], pressed: &str) -> Resolved {
    let key = |k: &str| Chord::parse(k).ok().map(|c| c.key());
    let before: Option<Vec<_>> = pending.iter().map(|k| key(k)).collect();
    let (Some(before), Some(now)) = (before, key(pressed)) else {
        return Resolved::Nothing;
    };
    match Keymap::defaults().resolve(&[Context::Viewer], &before, &now) {
        Step::Run(action) => ViewerAction::of(&action).map_or(Resolved::Nothing, Resolved::Run),
        Step::Wait => Resolved::Wait,
        Step::Nothing => Resolved::Nothing,
    }
}

/// The key `e` names, as the keys file writes it.
fn pressed(e: &KeyboardEvent) -> Option<String> {
    let key = match e.key() {
        Key::Character(c) if c == " " => "Space".to_string(),
        Key::Character(c) if c == "+" => "Plus".to_string(),
        Key::Character(c) => c,
        Key::Shift | Key::Control | Key::Alt | Key::Meta => return None,
        k => k.to_string(),
    };
    let m = e.modifiers();
    if m.meta() || m.alt() {
        return None;
    }
    Some(if m.ctrl() { format!("Ctrl+{key}") } else { key })
}

/// Open a window showing the file at `path` in its viewer, until it closes.
pub fn launch(path: PathBuf) {
    let title = path.display().to_string();
    let cfg = Desktop::new()
        .with_menu(None)
        .with_close_behaviour(WindowCloseBehaviour::WindowCloses)
        .with_background_color((15, 17, 20, 255))
        .with_custom_head(
            "<style>html, body, #main { margin: 0; height: 100%; overflow: hidden; }</style>"
                .to_string(),
        )
        .with_window(
            WindowBuilder::new()
                .with_title(title)
                .with_inner_size(LogicalSize::new(960.0, 720.0)),
        );
    dioxus::LaunchBuilder::desktop()
        .with_cfg(cfg)
        .with_context(path)
        .launch(TrialWindow);
}

#[component]
fn TrialWindow() -> Element {
    let path: PathBuf = use_context();
    let mut command = use_signal(|| None::<Command>);
    let mut pending = use_signal(Vec::<String>::new);
    let root_style = use_hook(|| {
        let theme = settings::current().theme;
        let palette =
            crate::themes::palette(&theme, &crate::settings::themes_dir()).unwrap_or_default();
        format!(
            ":root{{{}}}",
            crate::themes::css(&crate::themes::window(&palette))
        )
    });
    rsx! {
        style { "{root_style}" }
        div {
            style: "height: 100%; outline: none;",
            tabindex: "0",
            onmounted: move |e| async move {
                let _ = e.set_focus(true).await;
            },
            onkeydown: move |e: KeyboardEvent| {
                let Some(key) = pressed(&e) else {
                    return;
                };
                let resolved = resolve(&pending.peek(), &key);
                match resolved {
                    Resolved::Run(action) => {
                        e.prevent_default();
                        pending.write().clear();
                        let seq = command.peek().map_or(1, |c| c.seq + 1);
                        command.set(Some(Command { seq, action }));
                    }
                    Resolved::Wait => {
                        e.prevent_default();
                        pending.write().push(key);
                    }
                    Resolved::Nothing => pending.write().clear(),
                }
            },
            Viewer {
                path,
                command,
                on_close: move |_| dioxus::desktop::window().close(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bound_key_runs_its_action() {
        assert_eq!(resolve(&[], "j"), Resolved::Run(ViewerAction::ScrollDown));
        assert_eq!(
            resolve(&[], "Ctrl+d"),
            Resolved::Run(ViewerAction::ScrollHalfDown)
        );
        assert_eq!(
            resolve(&[], "Space"),
            Resolved::Run(ViewerAction::PlayToggle)
        );
    }

    #[test]
    fn the_first_key_of_a_sequence_waits_for_the_next() {
        assert_eq!(resolve(&[], "g"), Resolved::Wait);
        assert_eq!(
            resolve(&["g".into()], "g"),
            Resolved::Run(ViewerAction::ScrollTop)
        );
        assert_eq!(resolve(&["g".into()], "j"), Resolved::Nothing);
    }

    #[test]
    fn an_unbound_key_does_nothing() {
        assert_eq!(resolve(&[], "z"), Resolved::Nothing);
    }
}
