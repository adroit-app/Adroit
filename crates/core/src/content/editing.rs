//! A window switched between a document's viewer and a Neovim on the
//! document: the Neovim it hides runs on while the window shows the viewer,
//! so the document is read and written again without starting it anew.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use adroit_model::{EnvironmentId, TerminalProgram, TerminalSession};

use super::{Area, ContentView, Opening, Tab, TerminalAsk, WindowId};
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::{self, nav::Place};
use crate::viewers::FileKind;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Editing {
    /// The windows a Neovim is being started for, by the viewer each showed
    /// when it was asked for.
    starting: BTreeMap<WindowId, Tab>,
    /// The Neovim each window hides while it shows the viewer of its file.
    hidden: BTreeMap<WindowId, u128>,
}

/// Whether a window showing the file at `path` switches between its viewer
/// and a Neovim on it.
fn switches(path: &Path) -> bool {
    FileKind::of(path) == FileKind::Markdown
}

/// The file the Neovim by `id` of `terminals` was started on.
fn file_of(terminals: &[TerminalSession], id: u128) -> Option<&PathBuf> {
    terminals
        .iter()
        .find(|t| t.id == id && t.program == TerminalProgram::Neovim)
        .and_then(|t| t.file.as_ref())
}

/// Switch the focused window between the viewer of the document it shows
/// and a Neovim on it: the one it hid, or else one already on the file, or
/// else one started for it.
pub(crate) fn switch_editing(state: &mut ViewState) -> Vec<Effect> {
    let view = &mut state.content;
    let Some(window) = view.windows.focused_window() else {
        return Vec::new();
    };
    let Some(shows) = view.windows.shows(window).cloned() else {
        return Vec::new();
    };
    let environment = shows.environment().clone();
    if !state.hosts.is_local(&environment) {
        return Vec::new();
    }
    let work = state.hosts.work(&environment);
    let terminals = tree::terminals(work);
    match &shows {
        Tab::Viewer(_, path) if switches(path) => {
            let nvim = view
                .editing
                .hidden
                .remove(&window)
                .filter(|id| file_of(terminals, *id) == Some(path))
                .or_else(|| {
                    terminals
                        .iter()
                        .find(|t| {
                            t.program == TerminalProgram::Neovim && t.file.as_ref() == Some(path)
                        })
                        .map(|t| t.id)
                });
            if let Some(id) = nvim {
                view.windows
                    .replace_window(window, Tab::Terminal(environment, id));
                view.focus = Area::Console;
                return Vec::new();
            }
            if view.editing.starting.contains_key(&window) {
                return Vec::new();
            }
            let place =
                tree::place_of_tab(tree::listed(work), terminals, &shows).unwrap_or_else(|| {
                    Place::at(&environment, path.parent().unwrap_or(Path::new("/")))
                });
            view.editing.starting.insert(window, shows.clone());
            vec![Effect::StartTerminal(TerminalAsk {
                place,
                runs: TerminalProgram::Neovim,
                file: Some(path.clone()),
                opening: Opening::InWindow(window),
                focus: true,
            })]
        }
        Tab::Terminal(_, id) => {
            let Some(file) = file_of(terminals, *id).filter(|f| switches(f)) else {
                return Vec::new();
            };
            view.windows
                .replace_window(window, Tab::Viewer(environment, file.clone()));
            view.editing.hidden.insert(window, *id);
            view.editing
                .hidden
                .retain(|w, _| view.windows.shows(*w).is_some());
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// How a Neovim started for the window a viewer showed in opens: in that
/// window while it still shows that viewer, and else as anything opens.
pub(crate) fn started_in(view: &mut ContentView, how: Opening) -> Opening {
    let Opening::InWindow(window) = how else {
        return how;
    };
    match view.editing.starting.remove(&window) {
        Some(viewer) if view.windows.shows(window) == Some(&viewer) => how,
        _ => Opening::Existing,
    }
}

/// A Neovim asked to open as `how` could not start.
pub(crate) fn not_started(view: &mut ContentView, how: Opening) {
    if let Opening::InWindow(window) = how {
        view.editing.starting.remove(&window);
    }
}

/// The Neovims of `environment` that windows hide while they show the
/// viewer of the Neovim's file.
pub(crate) fn hidden_terminals(
    view: &ContentView,
    environment: &EnvironmentId,
    terminals: &[TerminalSession],
) -> BTreeSet<u128> {
    view.editing
        .hidden
        .iter()
        .filter(|(window, id)| match view.windows.shows(**window) {
            Some(Tab::Viewer(e, path)) => {
                e == environment && file_of(terminals, **id) == Some(path)
            }
            _ => false,
        })
        .map(|(_, id)| *id)
        .collect()
}
