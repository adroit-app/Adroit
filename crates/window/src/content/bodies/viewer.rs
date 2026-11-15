//! A window showing a file, or an image a console drew, in its viewer.

use std::path::PathBuf;

use adroit_model::EnvironmentId;
use dioxus::prelude::*;

use crate::content::WindowId;
use crate::pictures::{self, Gallery};
use crate::themes::Document;
use crate::viewers;

/// A window's viewer, handed the actions sent to this window alone.
#[component]
pub(crate) fn ViewerWindow(
    window: WindowId,
    path: PathBuf,
    root: PathBuf,
    theme: ReadSignal<Document>,
    left_at: Signal<viewers::LeftAt>,
    commands: ReadSignal<Option<(WindowId, viewers::Command)>>,
    on_close: EventHandler<()>,
    on_link: EventHandler<PathBuf>,
) -> Element {
    let command = use_memo(move || commands().filter(|(w, _)| *w == window).map(|(_, c)| c));
    rsx! {
        div { class: "win-viewer",
            viewers::Viewer {
                path,
                root,
                theme,
                command,
                on_close,
                on_link,
                left: Some((window, left_at)),
            }
        }
    }
}

/// A window's image viewer on the image `id` that `environment`'s `session`'s
/// console drew, as `gallery` keeps it, handed the actions sent to this
/// window alone.
#[component]
pub(crate) fn DrawnWindow(
    window: WindowId,
    environment: EnvironmentId,
    session: String,
    id: u64,
    name: String,
    gallery: ReadSignal<Gallery>,
    commands: ReadSignal<Option<(WindowId, viewers::Command)>>,
    on_close: EventHandler<()>,
) -> Element {
    let command = use_memo(move || commands().filter(|(w, _)| *w == window).map(|(_, c)| c));
    let src = use_memo(move || {
        gallery
            .read()
            .store
            .find(&environment, &session, id)
            .map(pictures::kept_shown)
    });
    rsx! {
        div { class: "win-viewer",
            viewers::DrawnViewer { src: src(), name, command, on_close }
        }
    }
}
