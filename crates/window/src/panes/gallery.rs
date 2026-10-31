//! The images the consoles drew: kept as they are drawn, listed in the
//! details panel for the session shown, and handed whole to the image viewer.

use adroit_core::msg::Msg;
use adroit_model::EnvironmentId;
use adroit_model::time::now_ms;
use dioxus::prelude::*;

use crate::details;
use crate::nav;
use crate::pictures::{self, Gallery};
use crate::shell::Ui;

/// Keep the images `drawn` in `environment`'s `session`'s console, encoded
/// away from the window's thread.
pub(crate) fn keep_drawn(
    mut gallery: Signal<Gallery>,
    environment: EnvironmentId,
    session: String,
    drawn: Vec<adroit_term::Drawn>,
) {
    // A program may send the same image again as it redraws; it is kept
    // already, and encoding it again would be for nothing.
    let drawn: Vec<_> = {
        let g = gallery.peek();
        drawn
            .into_iter()
            .filter(|d| {
                g.store
                    .find(&environment, &session, pictures::id_of(d))
                    .is_none()
            })
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
            g.store.keep(&environment, &session, k);
        }
    });
}

/// The details panel lists the images the shown session's console drew.
pub(crate) fn use_shown_images(ui: &Ui) {
    let (app, gallery) = (ui.app, ui.local.gallery);
    let shown = use_memo(move || {
        let environment = app.shown_environment();
        let hosts = app.hosts.read();
        let tree = adroit_core::tree::listed(hosts.work(&environment)).clone();
        (environment, tree)
    });
    // The panel's session rather than the focused window's, which may show
    // one of its images.
    let session = use_memo(move || app.details.read().shown.session.clone());
    use_effect(move || {
        let (environment, tree) = &*shown.read();
        let drawn = match &*session.read() {
            Some(id) if nav::find(tree, id).is_some() => {
                pictures::drawn(&gallery.read().store, environment, id)
            }
            _ => Vec::new(),
        };
        app.dispatch(Msg::DetailsShown(Box::new(details::Told { drawn })));
    });
}
