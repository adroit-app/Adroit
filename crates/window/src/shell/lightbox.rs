//! The lightbox: a session's images, one at a time, over the window. It
//! only draws and passes on what the mouse does; the view state holds it.

use std::path::PathBuf;
use std::sync::Arc;

use adroit_core::actions::Action;
use adroit_core::content::Tab;
use adroit_core::msg::Msg;
use adroit_core::overlay::lightbox::Lightbox as Shown;
use adroit_core::viewers::media_type;
use adroit_term::picture::data_url;
use dioxus::prelude::*;

use super::ui::Ui;
use crate::icons::{Icon, IconView};
use crate::pictures;

/// The lightbox open, or nothing.
#[component]
pub(crate) fn Lightbox() -> Element {
    let ui = use_context::<Ui>();
    let Some(shown) = ui.app.overlays.read().lightbox.clone() else {
        return rsx! {};
    };
    rsx! {
        LightboxOf { shown }
    }
}

/// Where a drag of the image started, and how large the stage was.
#[derive(Clone, Copy, PartialEq)]
struct Drag {
    from: (f64, f64),
    size: (f64, f64),
}

#[component]
fn LightboxOf(shown: Shown) -> Element {
    let ui = use_context::<Ui>();
    let app = ui.app;
    let act = move |action: Action| app.dispatch(Msg::LightboxAction(action));
    let mut stage = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let mut drag = use_signal(|| None::<Drag>);
    let thing = shown.shown().clone();
    let name = match &thing {
        Tab::Drawn { width, height, .. } => format!("Image {width} × {height}"),
        Tab::Viewer(_, path) => path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        _ => String::new(),
    };
    let many = shown.images.len() > 1;
    let counter = format!("{} / {}", shown.at + 1, shown.images.len());
    let style = format!(
        "transform: translate({}%, {}%) scale({});",
        shown.pan.0 * 100.0,
        shown.pan.1 * 100.0,
        shown.zoom.factor()
    );
    let percent = shown.zoom.percent();
    rsx! {
        div { class: "lightbox",
            // A click outside the frame closes it.
            onclick: move |_| act(Action::LightboxClose),
            div { class: "lightbox-frame",
                onclick: move |e: Event<MouseData>| e.stop_propagation(),
                div { class: "lightbox-bar",
                    span { class: "lightbox-name", "{name}" }
                    if many {
                        span { class: "lightbox-count", "{counter}" }
                    }
                    span { class: "lightbox-spacer" }
                    button { class: "lightbox-button", title: "Zoom out",
                        onclick: move |_| act(Action::LightboxZoomOut),
                        IconView { icon: Icon::ZoomOut, size: 14 }
                    }
                    button { class: "lightbox-button lightbox-zoom", title: "Fit to the lightbox",
                        onclick: move |_| act(Action::LightboxZoomFit),
                        "{percent}%"
                    }
                    button { class: "lightbox-button", title: "Zoom in",
                        onclick: move |_| act(Action::LightboxZoomIn),
                        IconView { icon: Icon::ZoomIn, size: 14 }
                    }
                    button { class: "lightbox-button", title: "Close",
                        onclick: move |_| act(Action::LightboxClose),
                        IconView { icon: Icon::X, size: 14 }
                    }
                }
                div { class: "lightbox-stage",
                    onmounted: move |e| stage.set(Some(e.data())),
                    onwheel: move |e: WheelEvent| {
                        let dy = e.delta().strip_units().y;
                        if dy < 0.0 {
                            act(Action::LightboxZoomIn);
                        } else if dy > 0.0 {
                            act(Action::LightboxZoomOut);
                        }
                    },
                    onmousedown: move |e: Event<MouseData>| {
                        e.prevent_default();
                        let at = e.client_coordinates();
                        let from = (at.x, at.y);
                        spawn(async move {
                            let Some(m) = stage() else {
                                return;
                            };
                            if let Ok(r) = m.get_client_rect().await {
                                drag.set(Some(Drag { from, size: (r.width(), r.height()) }));
                            }
                        });
                    },
                    onmousemove: move |e: Event<MouseData>| {
                        let Some(d) = drag() else {
                            return;
                        };
                        let at = e.client_coordinates();
                        let (dx, dy) = (at.x - d.from.0, at.y - d.from.1);
                        if d.size.0 > 0.0 && d.size.1 > 0.0 {
                            app.dispatch(Msg::LightboxPan {
                                dx: dx / d.size.0,
                                dy: dy / d.size.1,
                            });
                        }
                        drag.set(Some(Drag { from: (at.x, at.y), ..d }));
                    },
                    onmouseup: move |_| drag.set(None),
                    onmouseleave: move |_| drag.set(None),
                    div { class: "lightbox-image", style: "{style}",
                        LightboxImage { key: "{thing:?}", thing: thing.clone(), name: name.clone() }
                    }
                    if many {
                        button { class: "lightbox-step prev", title: "Previous image",
                            onmousedown: move |e: Event<MouseData>| e.stop_propagation(),
                            onclick: move |_| act(Action::LightboxPrev),
                            IconView { icon: Icon::ChevronLeft, size: 20 }
                        }
                        button { class: "lightbox-step next", title: "Next image",
                            onmousedown: move |e: Event<MouseData>| e.stop_propagation(),
                            onclick: move |_| act(Action::LightboxNext),
                            IconView { icon: Icon::ChevronRight, size: 20 }
                        }
                    }
                }
            }
        }
    }
}

/// The picture of `thing`: the copy the window keeps of an image a console
/// drew, or an image file read whole.
#[component]
fn LightboxImage(thing: Tab, name: String) -> Element {
    let ui = use_context::<Ui>();
    let gallery = ui.local.gallery;
    let path = match &thing {
        Tab::Viewer(_, path) => Some(path.clone()),
        _ => None,
    };
    let file = use_resource(move || {
        let path = path.clone();
        async move {
            let path: PathBuf = path?;
            tokio::task::spawn_blocking(move || {
                std::fs::read(&path)
                    .ok()
                    .map(|bytes| data_url(media_type(&path), &bytes))
            })
            .await
            .ok()
            .flatten()
        }
    });
    let src: Option<Arc<str>> = match &thing {
        Tab::Drawn {
            environment,
            session,
            id,
            ..
        } => gallery
            .read()
            .store
            .find(environment, session, *id)
            .map(pictures::kept_shown),
        _ => file.read().clone().flatten(),
    };
    let reading = path_reading(&thing, file.read().is_none());
    rsx! {
        match src {
            Some(src) => rsx! {
                img { class: "lightbox-picture", src: "{src}", alt: "{name}", draggable: false }
            },
            None if reading => rsx! {},
            None => rsx! {
                div { class: "lightbox-empty", "This image cannot be shown" }
            },
        }
    }
}

/// Whether the file `thing` shows is still being read.
fn path_reading(thing: &Tab, unread: bool) -> bool {
    matches!(thing, Tab::Viewer(..)) && unread
}
