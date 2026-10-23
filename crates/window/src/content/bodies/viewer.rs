//! A window showing a file in its viewer.

use std::path::PathBuf;

use dioxus::prelude::*;

use crate::content::WindowId;
use crate::viewers;

/// A window's viewer, handed the actions sent to this window alone.
#[component]
pub(crate) fn ViewerWindow(
    window: WindowId,
    path: PathBuf,
    commands: ReadSignal<Option<(WindowId, viewers::Command)>>,
    on_close: EventHandler<()>,
) -> Element {
    let command = use_memo(move || commands().filter(|(w, _)| *w == window).map(|(_, c)| c));
    rsx! {
        div { class: "win-viewer",
            viewers::Viewer { path, command, on_close }
        }
    }
}
