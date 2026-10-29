//! Where the prompt's and the reader's Neovims keep their configuration, and
//! the ways to it: its path copied, its folder in the Finder, its `init.lua`
//! edited in a Neovim of the main window.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use adroit_core::msg::Msg;
use adroit_model::paths::tilde;
use dioxus::desktop::window;
use dioxus::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use super::{open_with, run_open};
use crate::bridge::AppCtx;
use crate::components::{SettingGroup, SettingRow};
use crate::icons::{Icon, IconView};
use crate::link::Link;
use crate::paths::home;

thread_local! {
    /// The main window, for the messages the Settings window asks it to step.
    static MAIN: RefCell<Option<UnboundedSender<Msg>>> = const { RefCell::new(None) };
}

/// Step in the main window, brought forward, the messages the Settings
/// window asks it to, for as long as the main window is open. The Settings
/// window is a document of its own, which cannot reach the main window's
/// state.
pub(crate) fn use_asks(app: AppCtx) {
    use_hook(move || {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        MAIN.with(|m| *m.borrow_mut() = Some(tx));
        let desktop = window();
        spawn(async move {
            while let Some(msg) = rx.recv().await {
                desktop.window.set_focus();
                app.dispatch(msg);
            }
        });
    });
}

fn ask_main(msg: Msg) {
    MAIN.with(|m| {
        if let Some(tx) = &*m.borrow() {
            let _ = tx.send(msg);
        }
    });
}

/// The folder the Neovim `name` reads its configuration from:
/// `<name>/nvim` beside `settings`.
pub(crate) fn folder(settings: &Path, name: &str) -> PathBuf {
    settings
        .parent()
        .unwrap_or(Path::new(""))
        .join(name)
        .join("nvim")
}

/// The configuration of the Neovim `name`, `prompt` or `console`.
#[component]
pub(super) fn NeovimGroup(name: &'static str) -> Element {
    let remote = use_context::<Link>().remote_status().is_some();
    let dir = folder(&crate::paths::config_dir().join("settings.toml"), name);
    let mut copied = use_signal(|| false);
    let shown = tilde(&dir, &home());
    rsx! {
        SettingGroup { title: "Neovim",
            SettingRow { label: "Configuration on this computer", hint: "{shown}",
                button {
                    title: "Copy the folder's path",
                    onclick: {
                        let dir = dir.clone();
                        move |_| {
                            let ok = crate::clipboard::copy_to_clipboard(&dir.to_string_lossy()).is_ok();
                            copied.set(ok);
                        }
                    },
                    IconView { icon: Icon::Copy, size: 13 }
                    if copied() { "Copied" } else { "Copy path" }
                }
                button {
                    onclick: {
                        let dir = dir.clone();
                        move |_| {
                            let _ = std::fs::create_dir_all(&dir);
                            run_open(&dir);
                        }
                    },
                    IconView { icon: Icon::FolderOpen, size: 14 }
                    "Show in Finder"
                }
                button {
                    title: "Edit init.lua in a Neovim of the main window",
                    onclick: move |_| {
                        // Neovim starts in the folder, which has to be there.
                        let _ = std::fs::create_dir_all(&dir);
                        if remote {
                            open_with("-t", &dir.join("init.lua"), "");
                        } else {
                            ask_main(Msg::EditFile(dir.join("init.lua")));
                        }
                    },
                    IconView { icon: Icon::Code, size: 14 }
                    if remote { "Edit locally" } else { "Edit init.lua" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_neovim_s_configuration_is_a_folder_of_its_own_beside_the_settings() {
        let settings = Path::new("/c/adroit/settings.toml");
        assert_eq!(
            folder(settings, "prompt"),
            Path::new("/c/adroit/prompt/nvim")
        );
        assert_eq!(
            folder(settings, "console"),
            Path::new("/c/adroit/console/nvim")
        );
    }
}
