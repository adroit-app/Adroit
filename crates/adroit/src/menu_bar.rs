//! The menu bar: every action a key can run that is not only a key, under the
//! menu a Mac app keeps it in, each showing the Command key bound to it.

use std::cell::RefCell;

use dioxus::desktop::muda::accelerator::Accelerator;
use dioxus::desktop::muda::{
    AboutMetadata, Icon as MenuIcon, IconMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu,
};

use crate::icons::Icon;
use crate::keymap::{Action, Chord, Context, Keymap, MENU_KEYS, Step};

/// An entry macOS draws and runs itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    About,
    Services,
    Hide,
    HideOthers,
    ShowAll,
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    FullScreen,
    Minimize,
    Zoom,
    BringAllToFront,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Action(Action, &'static str),
    System(System),
    Separator,
}

/// One menu of the bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: &'static str,
    pub entries: Vec<Entry>,
}

/// The key macOS gives the full screen entry.
const FULL_SCREEN: &str = "Cmd+Ctrl+f";

/// The menus of the bar, in order.
pub fn sections() -> Vec<Section> {
    use Entry::{Action as Run, Separator, System as Sys};
    vec![
        Section {
            title: "Adroit",
            entries: vec![
                Sys(System::About),
                Run(Action::SettingsOpen, "Settings…"),
                Run(Action::DaemonRestart, "Restart Session Daemon…"),
                Separator,
                Sys(System::Services),
                Separator,
                Sys(System::Hide),
                Sys(System::HideOthers),
                Sys(System::ShowAll),
                Separator,
                Sys(System::Quit),
            ],
        },
        Section {
            title: "Session",
            entries: vec![
                Run(Action::SessionNew, "New Session"),
                Run(Action::SessionScratch, "New Scratch Session"),
                Run(Action::SessionAgentNext, "Next Agent of New Session"),
                Run(Action::SessionAgentPrev, "Previous Agent of New Session"),
                Run(Action::SessionPick, "Go to or Start Session…"),
                Separator,
                Run(Action::PromptSend, "Send Prompt"),
                Run(Action::PromptImages, "Select Attached Image"),
                Separator,
                Run(Action::ShellOpen, "Open Shell"),
                Run(Action::NeovimOpen, "Open Neovim"),
                Run(Action::TerminalStop, "Stop Terminal"),
                Run(Action::TabClose, "Close Tab"),
                Run(Action::WindowClose, "Close Window"),
                Separator,
                Run(Action::SessionRename, "Rename…"),
                Run(Action::SessionTakeOver, "Take Over…"),
                Run(Action::SessionStop, "End…"),
            ],
        },
        Section {
            title: "Edit",
            entries: vec![
                Sys(System::Undo),
                Sys(System::Redo),
                Separator,
                Sys(System::Cut),
                Sys(System::Copy),
                Sys(System::Paste),
                Sys(System::SelectAll),
            ],
        },
        Section {
            title: "View",
            entries: vec![
                Run(Action::TreeToggle, "Toggle Tree"),
                Run(Action::DetailsToggle, "Toggle Details"),
                Separator,
                Run(Action::DetailsOverview, "Overview"),
                Run(Action::DetailsChanges, "Changes"),
                Run(Action::DetailsLog, "Log"),
                Separator,
                Run(Action::Filter, "Filter…"),
                Run(Action::WorktreesToggle, "Toggle All Worktrees"),
                Run(Action::Refresh, "Refresh"),
                Separator,
                Run(Action::ZoomIn, "Zoom In"),
                Run(Action::ZoomOut, "Zoom Out"),
                Run(Action::ZoomReset, "Actual Size"),
                Separator,
                Sys(System::FullScreen),
            ],
        },
        Section {
            title: "Go",
            entries: vec![
                Run(Action::TabNext, "Next Tab"),
                Run(Action::TabPrev, "Previous Tab"),
                Run(Action::TabLast, "Tab Before"),
                Separator,
                Run(Action::SessionNext, "Next Session"),
                Run(Action::SessionPrev, "Previous Session"),
                Run(Action::WaitingNext, "Next Waiting"),
                Run(Action::WaitingPrev, "Previous Waiting"),
                Separator,
                Run(Action::WindowLeft, "Left Window"),
                Run(Action::WindowDown, "Window Below"),
                Run(Action::WindowUp, "Window Above"),
                Run(Action::WindowRight, "Right Window"),
                Run(Action::WindowNext, "Next Window"),
                Run(Action::WindowPrev, "Previous Window"),
                Run(Action::WindowTop, "First Window"),
                Run(Action::WindowBottom, "Last Window"),
                Run(Action::WindowBack, "Back to Window Before"),
                Run(Action::PromptFocus, "Prompt"),
            ],
        },
        Section {
            title: "Layout",
            entries: vec![
                Run(Action::WindowVsplit, "Split Side by Side"),
                Run(Action::WindowSplit, "Split One Above the Other"),
                Run(Action::WindowNew, "Split for a New Session"),
                Run(Action::WindowQuit, "Close Split"),
                Run(Action::WindowOnly, "Close Other Splits"),
                Separator,
                Run(Action::WindowMoveLeft, "Move Window to the Left"),
                Run(Action::WindowMoveDown, "Move Window to the Bottom"),
                Run(Action::WindowMoveUp, "Move Window to the Top"),
                Run(Action::WindowMoveRight, "Move Window to the Right"),
                Run(Action::WindowExchange, "Exchange Windows"),
                Run(Action::WindowRotate, "Rotate Windows"),
                Run(Action::WindowToTab, "Move Window to a New Tab"),
                Separator,
                Run(Action::WindowEqualize, "Equal Sizes"),
                Run(Action::WindowWider, "Wider"),
                Run(Action::WindowNarrower, "Narrower"),
                Run(Action::WindowTaller, "Taller"),
                Run(Action::WindowShorter, "Shorter"),
                Run(Action::WindowMaximizeWidth, "Widest"),
                Run(Action::WindowMaximizeHeight, "Tallest"),
            ],
        },
        Section {
            title: "Window",
            entries: vec![
                Sys(System::Minimize),
                Sys(System::Zoom),
                Separator,
                Sys(System::BringAllToFront),
            ],
        },
    ]
}

/// The Command key the menu shows for `action` and runs it by: the first
/// chord with Command bound to it in `global` that every area leaves to it.
pub fn shortcut(keymap: &Keymap, action: &Action) -> Option<Chord> {
    let full_screen = Chord::parse(FULL_SCREEN).ok();
    // A menu's key works wherever the keys are, so an area's own binding, or
    // an area's other meaning for the key, cannot be one.
    keymap
        .sequences_for(Context::Global, action)
        .into_iter()
        .filter_map(|seq| match seq {
            [chord] => Some(chord),
            _ => None,
        })
        .find(|chord| {
            let key = chord.key();
            key.meta
                && Some(*chord) != full_screen.as_ref()
                && accelerator(chord).is_some()
                && Context::ALL.into_iter().all(|ctx| {
                    keymap.resolve(&[ctx, Context::Global], &[], &key) == Step::Run(action.clone())
                })
        })
        .cloned()
}

/// `chord` as the menu takes it: by the key it is on, which a character typed
/// with Shift, such as `}`, does not name.
pub fn accelerator(chord: &Chord) -> Option<Accelerator> {
    let key = chord.key();
    let mut text = String::new();
    for (on, m) in [
        (key.meta, "super+"),
        (key.ctrl, "control+"),
        (key.alt, "alt+"),
        (key.shift, "shift+"),
    ] {
        if on {
            text.push_str(m);
        }
    }
    text.push_str(match key.key.as_str() {
        " " => "Space",
        k => k,
    });
    text.parse().ok()
}

/// The keys the window leaves to the menu rather than taking itself, as it
/// names them: a letter for Command with it whatever else is held, which
/// macOS's own entries answer to, and each Command key the entries show.
pub fn left_to_menu(keymap: &Keymap) -> Vec<String> {
    let full_screen = Chord::parse(FULL_SCREEN).expect("the full screen key reads");
    let bound = keymap.resolve(&Context::ALL, &[], &full_screen.key()) != Step::Nothing;
    let shortcuts = sections()
        .into_iter()
        .flat_map(|s| s.entries)
        .filter_map(|e| match e {
            Entry::Action(a, _) => shortcut(keymap, &a),
            _ => None,
        });
    MENU_KEYS
        .iter()
        .map(|k| k.to_string())
        .chain((!bound).then(|| full_screen.to_string()))
        .chain(shortcuts.map(|c| c.to_string()))
        .collect()
}

/// The action a menu entry runs, by the entry's id: the action's name.
pub fn action_of(id: &str) -> Option<Action> {
    id.parse().ok()
}

thread_local! {
    /// The entries that run actions, to show the keys in force on.
    static ITEMS: RefCell<Vec<(Action, IconMenuItem)>> = const { RefCell::new(Vec::new()) };
}

/// The menu bar, its entries showing no keys until [`show_shortcuts`].
pub fn build() -> Menu {
    let bar = Menu::new();
    let mut items = Vec::new();
    for section in sections() {
        let menu = Submenu::new(section.title, true);
        for entry in section.entries {
            let _ = match entry {
                Entry::Action(action, label) => {
                    let item =
                        IconMenuItem::with_id(action.to_string(), label, true, icon(&action), None);
                    let appended = menu.append(&item);
                    items.push((action, item));
                    appended
                }
                Entry::System(s) => menu.append(&system(s)),
                Entry::Separator => menu.append(&PredefinedMenuItem::separator()),
            };
        }
        #[cfg(target_os = "macos")]
        if section.title == "Window" {
            menu.set_as_windows_menu_for_nsapp();
        }
        let _ = bar.append(&menu);
    }
    // The menu dioxus would have drawn offers its developer tools in a debug
    // build; this one keeps them.
    if cfg!(debug_assertions) {
        let develop = Submenu::new("Develop", true);
        let _ = develop.append(&MenuItem::with_id(
            "dioxus-toggle-dev-tools",
            "Toggle Developer Tools",
            true,
            None,
        ));
        let _ = bar.append(&develop);
    }
    ITEMS.with(|i| *i.borrow_mut() = items);
    #[cfg(target_os = "macos")]
    as_templates(&bar);
    bar
}

/// How many pixels square an entry's icon is drawn: macOS shows it 18
/// points tall, twice that on a Retina display.
pub const ICON_PX: u32 = 36;

/// The icon an entry running `action` shows.
pub fn icon(action: &Action) -> Option<MenuIcon> {
    let icon = Icon::of(action)?;
    MenuIcon::from_rgba(icon.rgba(ICON_PX)?, ICON_PX, ICON_PX).ok()
}

/// Mark every entry's icon a template image, which macOS draws in the menu's
/// text colour, light or dark and highlighted; muda leaves them as drawn,
/// black on a dark menu.
#[cfg(target_os = "macos")]
fn as_templates(bar: &Menu) {
    use dioxus::desktop::muda::ContextMenu;
    use objc2_app_kit::NSMenu;

    fn mark(menu: &NSMenu) {
        for item in menu.itemArray() {
            if let Some(image) = item.image() {
                image.setTemplate(true);
            }
            if let Some(sub) = item.submenu() {
                mark(&sub);
            }
        }
    }
    // SAFETY: muda hands out the bar's NSMenu, alive as long as `bar`.
    let menu = unsafe { &*(bar.ns_menu() as *const NSMenu) };
    mark(menu);
}

fn system(s: System) -> PredefinedMenuItem {
    match s {
        System::About => PredefinedMenuItem::about(
            None,
            Some(AboutMetadata {
                name: Some("Adroit".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                ..Default::default()
            }),
        ),
        System::Services => PredefinedMenuItem::services(None),
        System::Hide => PredefinedMenuItem::hide(None),
        System::HideOthers => PredefinedMenuItem::hide_others(None),
        System::ShowAll => PredefinedMenuItem::show_all(None),
        System::Quit => PredefinedMenuItem::quit(None),
        System::Undo => PredefinedMenuItem::undo(None),
        System::Redo => PredefinedMenuItem::redo(None),
        System::Cut => PredefinedMenuItem::cut(None),
        System::Copy => PredefinedMenuItem::copy(None),
        System::Paste => PredefinedMenuItem::paste(None),
        System::SelectAll => PredefinedMenuItem::select_all(None),
        System::FullScreen => PredefinedMenuItem::fullscreen(None),
        System::Minimize => PredefinedMenuItem::minimize(None),
        System::Zoom => PredefinedMenuItem::maximize(None),
        System::BringAllToFront => PredefinedMenuItem::bring_all_to_front(None),
    }
}

/// Show on each entry the key [`shortcut`] gives it in `keymap`.
pub fn show_shortcuts(keymap: &Keymap) {
    ITEMS.with(|items| {
        for (action, item) in items.borrow().iter() {
            let accel = shortcut(keymap, action).as_ref().and_then(accelerator);
            let _ = item.set_accelerator(accel);
        }
    });
}

#[cfg(test)]
mod tests {
    use dioxus::desktop::muda::accelerator::{Code, Modifiers};

    use super::*;

    /// The actions that only make sense as keys: they move or scroll within
    /// an area, or act on the key pressed.
    const ONLY_KEYS: &[&str] = &[
        "none",
        "cursor.down",
        "cursor.up",
        "cursor.first",
        "cursor.last",
        "cursor.open",
        "cursor.show",
        "cursor.fold",
        "cursor.copy",
        "cursor.reveal",
        "scroll.down",
        "scroll.up",
        "scroll.half_down",
        "scroll.half_up",
        "scroll.top",
        "scroll.bottom",
        "console.send",
        "console.send_next",
        "console.pass_every_key",
        "prompt.write",
        "prompt.history_prev",
        "prompt.history_next",
    ];

    fn offered() -> Vec<Action> {
        sections()
            .into_iter()
            .flat_map(|s| s.entries)
            .filter_map(|e| match e {
                Entry::Action(a, _) => Some(a),
                _ => None,
            })
            .collect()
    }

    fn shown(keymap: &Keymap, action: Action) -> Option<String> {
        shortcut(keymap, &action).map(|c| c.to_string())
    }

    #[test]
    fn every_action_that_is_not_only_a_key_is_in_the_menu_bar_once() {
        let offered = offered();
        for name in Action::NAMES {
            let action: Action = name.parse().unwrap();
            let times = offered.iter().filter(|a| **a == action).count();
            if ONLY_KEYS.contains(name) {
                assert_eq!(times, 0, "{name} is only a key");
            } else {
                assert_eq!(times, 1, "{name}");
            }
        }
    }

    #[test]
    fn every_entry_that_runs_an_action_shows_its_icon() {
        for action in offered() {
            assert!(icon(&action).is_some(), "{action}");
        }
    }

    #[test]
    fn no_two_entries_of_the_menu_bar_share_an_icon() {
        let mut seen = std::collections::HashMap::new();
        for action in offered() {
            let icon = Icon::of(&action).unwrap();
            if let Some(other) = seen.insert(icon, action.clone()) {
                panic!("{action} and {other} both show {}", icon.name());
            }
        }
    }

    #[test]
    fn a_menu_entry_runs_the_action_it_names() {
        for action in offered() {
            assert_eq!(action_of(&action.to_string()), Some(action.clone()));
        }
        assert_eq!(action_of("dioxus-toggle-dev-tools"), None);
        assert_eq!(action_of("7"), None);
    }

    /// What the menu shows for `text` where the tests run: `Mod` is
    /// Command on macOS, and the menu shows only Command keys.
    fn written(text: &str) -> Option<String> {
        let chord = Chord::parse(text).unwrap();
        chord.key().meta.then(|| chord.to_string())
    }

    #[test]
    fn an_entry_shows_the_mod_key_bound_to_its_action_everywhere() {
        let map = Keymap::defaults();
        assert_eq!(shown(&map, Action::SessionNew), written("Mod+n"));
        assert_eq!(shown(&map, Action::SessionPick), written("Mod+p"));
        assert_eq!(shown(&map, Action::DetailsToggle), written("Mod+d"));
        assert_eq!(shown(&map, Action::SessionStop), written("Mod+Backspace"));
        assert_eq!(shown(&map, Action::TabClose), written("Mod+w"));
        assert_eq!(shown(&map, Action::NeovimOpen), written("Mod+e"));
        assert_eq!(shown(&map, Action::WindowClose), Some("Cmd+W".into()));
    }

    #[test]
    fn an_entry_whose_action_has_no_default_key_shows_none() {
        let map = Keymap::defaults();
        assert_eq!(shown(&map, Action::SessionScratch), None);
        assert_eq!(shown(&map, Action::WorktreesToggle), None);
    }

    #[test]
    fn a_sequence_shows_no_shortcut_in_the_menu() {
        let map = Keymap::defaults();
        assert_eq!(shown(&map, Action::WaitingNext), None);
        assert_eq!(shown(&map, Action::WindowLeft), None);
    }

    #[test]
    fn a_key_without_command_shows_no_shortcut_in_the_menu() {
        let map =
            Keymap::with_user("[global]\n\"Cmd+r\" = \"none\"\n\"Ctrl+Alt+r\" = \"refresh\"\n")
                .unwrap();
        assert_eq!(shown(&map, Action::Refresh), None);
        // The sidebar's `R` is no shortcut either.
        assert_eq!(shown(&map, Action::SessionRename), None);
    }

    #[test]
    fn the_shortcut_follows_the_bindings_in_force() {
        let map =
            Keymap::with_user("[global]\n\"Cmd+r\" = \"none\"\n\"Cmd+j\" = \"refresh\"\n").unwrap();
        assert_eq!(shown(&map, Action::Refresh), Some("Cmd+j".into()));
    }

    #[test]
    fn a_command_key_that_means_something_else_in_an_area_shows_no_shortcut() {
        let only_there =
            Keymap::with_user("[global]\n\"Cmd+r\" = \"none\"\n[tree]\n\"Cmd+j\" = \"refresh\"\n")
                .unwrap();
        assert_eq!(shown(&only_there, Action::Refresh), None);
        let overridden = Keymap::with_user("[console]\n\"Cmd+r\" = \"console.send\"\n").unwrap();
        assert_eq!(shown(&overridden, Action::Refresh), None);
    }

    #[test]
    fn a_key_the_menu_cannot_draw_as_it_is_typed_shows_no_shortcut() {
        // A brace is typed with Shift; the menu would show the bracket.
        let map = Keymap::with_user("[global]\n\"Cmd+}\" = \"session.next\"\n").unwrap();
        assert_eq!(shown(&map, Action::SessionNext), None);
        // `Mod+Plus` is passed over for `Mod+=`.
        assert_eq!(shown(&Keymap::defaults(), Action::ZoomIn), written("Mod+="));
    }

    #[test]
    fn a_command_key_the_full_screen_entry_shows_is_not_taken_from_it() {
        let map =
            Keymap::with_user("[global]\n\"Cmd+Ctrl+f\" = \"refresh\"\n\"Cmd+r\" = \"none\"\n")
                .unwrap();
        assert_eq!(shown(&map, Action::Refresh), None);
    }

    #[test]
    fn a_chord_is_given_to_the_menu_by_the_key_it_is_on() {
        let accel = |text: &str| accelerator(&Chord::parse(text).unwrap());
        assert_eq!(
            accel("Cmd+n"),
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyN))
        );
        assert_eq!(
            accel("Cmd+N"),
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::SHIFT),
                Code::KeyN
            ))
        );
        assert_eq!(
            accel("Cmd+Ctrl+Alt+1"),
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::CONTROL | Modifiers::ALT),
                Code::Digit1
            ))
        );
        assert_eq!(
            accel("Cmd+Backspace"),
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::Backspace))
        );
        assert_eq!(
            accel("Cmd+Space"),
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::Space))
        );
        assert_eq!(accel("Cmd+}"), None);
        assert_eq!(accel("Cmd+Plus"), None);
    }

    #[test]
    fn the_window_leaves_the_system_s_letters_and_every_shortcut_to_the_menu() {
        let held = left_to_menu(&Keymap::defaults());
        for letter in MENU_KEYS {
            assert!(held.contains(&letter.to_string()), "{letter}");
        }
        for chord in ["Mod+n", "Mod+p", "Mod+,", "Mod+w"] {
            if let Some(shown) = written(chord) {
                assert!(held.contains(&shown), "{chord}");
            }
        }
        for chord in ["Cmd+Ctrl+f", "Cmd+W"] {
            assert!(held.contains(&chord.to_string()), "{chord}");
        }
        // The window takes what the menu does not show.
        assert!(!held.contains(&"Cmd+}".to_string()));
        assert!(!held.contains(&"Cmd+Plus".to_string()));
    }

    #[test]
    fn a_held_key_is_written_as_the_window_reports_it() {
        let held = left_to_menu(
            &Keymap::with_user(
                "[global]\n\"Cmd+k\" = \"session.scratch\"\n\"Cmd+Alt+j\" = \"worktrees.toggle\"\n\"Cmd+J\" = \"session.next\"\n",
            )
            .unwrap(),
        );
        for (key, shift, alt) in [("k", false, false), ("j", false, true), ("J", true, false)] {
            let pressed = Chord::pressed(key, false, alt, shift, true).unwrap();
            assert!(held.contains(&pressed), "{pressed}");
        }
    }

    #[test]
    fn the_keys_left_to_the_menu_follow_the_bindings_in_force() {
        let held = left_to_menu(
            &Keymap::with_user(
                "[global]\n\"Cmd+n\" = \"none\"\n\"Cmd+j\" = \"session.new\"\n\"Cmd+Ctrl+f\" = \"filter\"\n",
            )
            .unwrap(),
        );
        assert!(held.contains(&"Cmd+j".to_string()));
        assert!(!held.contains(&"Cmd+n".to_string()));
        // Bound, the full screen entry's key is the window's.
        assert!(!held.contains(&"Cmd+Ctrl+f".to_string()));
    }
}
