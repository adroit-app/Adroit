//! The images the consoles drew: kept as they are drawn, listed in the
//! details panel for the session shown, and shown in a window of their own.

use std::path::PathBuf;

use adroit_core::msg::Msg;
use adroit_model::time::now_ms;
use dioxus::prelude::*;

use crate::content::Tab;
use crate::details::{self, Shot, Source};
use crate::image_window;
use crate::nav;
use crate::pictures::{self, Gallery};
use crate::shell::Ui;

/// Keep the images `drawn` in `session`'s console, encoded away from the
/// window's thread.
pub(crate) fn keep_drawn(
    mut gallery: Signal<Gallery>,
    session: String,
    drawn: Vec<adroit_term::Drawn>,
) {
    // A program may send the same image again as it redraws; it is kept
    // already, and encoding it again would be for nothing.
    let drawn: Vec<_> = {
        let g = gallery.peek();
        drawn
            .into_iter()
            .filter(|d| g.store.find(&session, pictures::id_of(d)).is_none())
            .collect()
    };
    if drawn.is_empty() {
        return;
    }
    spawn(async move {
        let at = now_ms();
        let kept = tokio::task::spawn_blocking(move || {
            drawn
                .into_iter()
                .filter_map(|d| pictures::Kept::of(d, at))
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        if kept.is_empty() {
            return;
        }
        let mut g = gallery.write();
        for k in kept {
            g.store.keep(&session, k);
        }
    });
}

/// The details panel lists the images the shown session's console drew.
pub(crate) fn use_shown_images(ui: &Ui) {
    let (app, gallery) = (ui.app, ui.local.gallery);
    let tree = use_memo(move || {
        let host = app.host.read();
        adroit_core::tree::listed(host.work.as_ref()).clone()
    });
    let active = use_memo(move || app.content.read().active().cloned());
    use_effect(move || {
        let drawn = match &*active.read() {
            Some(Tab::Session(id)) if nav::find(&tree.read(), id).is_some() => {
                pictures::drawn(&gallery.read().store, id)
            }
            _ => Vec::new(),
        };
        app.dispatch(Msg::DetailsShown(Box::new(details::Told { drawn })));
    });
}

impl Ui {
    /// Show `shot` of the overview in a window of its own. A drawn image is
    /// found among those kept for `session`; a file too large to read here
    /// is shown by macOS's own viewer.
    pub(crate) fn show_shot(&self, shot: &Shot, session: &str) {
        match &shot.source {
            Source::Console { id, width, height } => {
                let gallery = self.local.gallery.peek();
                let Some(kept) = gallery.store.find(session, *id) else {
                    return;
                };
                image_window::open(
                    &shot.key,
                    &format!("Image {width} × {height}"),
                    pictures::kept_shown(kept),
                    Some((*width, *height)),
                );
            }
            Source::File { path, .. } => {
                let (key, path) = (shot.key.clone(), PathBuf::from(path));
                spawn(async move {
                    let read = path.clone();
                    let shown = tokio::task::spawn_blocking(move || pictures::file_shown(&read))
                        .await
                        .ok()
                        .flatten();
                    match shown {
                        Some(src) => {
                            let title = path.file_name().map_or_else(
                                || path.display().to_string(),
                                |n| n.to_string_lossy().into_owned(),
                            );
                            image_window::open(&key, &title, src, None);
                        }
                        None => crate::bridge::open([path.display().to_string()]),
                    }
                });
            }
        }
    }
}
