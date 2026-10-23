//! A window of its own for each image of the overview, showing it whole.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use dioxus::desktop::{
    Config as Desktop, DesktopService, LogicalSize, WindowBuilder, WindowCloseBehaviour, window,
};
use dioxus::prelude::*;

use crate::settings::{self, Settings};

thread_local! {
    /// The windows open, by the key of the image each shows.
    static OPEN: RefCell<HashMap<String, Weak<DesktopService>>> = RefCell::new(HashMap::new());
}

const CSS: &str = include_str!("../assets/css/image-window.css");

/// The window's first size for an image `width` by `height` pixels, when
/// known: as large as the image, within a size that fits most screens.
fn first_size(size: Option<(u32, u32)>) -> (f64, f64) {
    const MOST: (f64, f64) = (1200.0, 860.0);
    const LEAST: (f64, f64) = (320.0, 240.0);
    let Some((w, h)) = size else {
        return (900.0, 680.0);
    };
    let (w, h) = (f64::from(w.max(1)), f64::from(h.max(1)));
    let scale = (MOST.0 / w).min(MOST.1 / h).min(1.0);
    ((w * scale).max(LEAST.0), (h * scale).max(LEAST.1))
}

/// Show the image `key` names in its window, opening one titled `title` to
/// show `src` when it has none. `size` is the image's, when known.
pub fn open(key: &str, title: &str, src: Arc<str>, size: Option<(u32, u32)>) {
    let open = OPEN.with(|o| o.borrow().get(key).and_then(Weak::upgrade));
    if let Some(w) = open {
        w.window.set_visible(true);
        w.window.set_focus();
        return;
    }
    let (w, h) = first_size(size);
    let cfg = Desktop::new()
        .with_menu(None)
        .with_close_behaviour(WindowCloseBehaviour::WindowCloses)
        .with_background_color((15, 17, 20, 255))
        .with_custom_head(format!("<style>{CSS}</style>"))
        .with_window(
            WindowBuilder::new()
                .with_title(title)
                .with_inner_size(LogicalSize::new(w, h))
                .with_min_inner_size(LogicalSize::new(240.0, 180.0)),
        );
    let dom = VirtualDom::new_with_props(
        ImageWindow,
        ImageWindowProps {
            key_: key.to_string(),
            title: title.to_string(),
            src,
        },
    );
    let _ = window().new_window(dom, cfg);
}

/// Close every image's window, as the main window goes.
pub fn close_all() {
    let open: Vec<_> = OPEN.with(|o| o.borrow_mut().drain().collect());
    for (_, w) in open {
        if let Some(w) = w.upgrade() {
            w.close();
        }
    }
}

/// Close the image's window in front, when one is; whether it was.
pub fn close_in_front() -> bool {
    let front = OPEN.with(|o| {
        o.borrow()
            .values()
            .filter_map(Weak::upgrade)
            .find(|w| w.window.is_focused())
    });
    match front {
        Some(w) => {
            w.close();
            true
        }
        None => false,
    }
}

#[component]
fn ImageWindow(key_: String, title: String, src: Arc<str>) -> Element {
    use_hook({
        let key = key_.clone();
        move || {
            let me = window();
            OPEN.with(|o| {
                let mut o = o.borrow_mut();
                o.retain(|_, w| w.strong_count() > 0);
                o.insert(key, Rc::downgrade(&me));
            });
        }
    });
    let mut now = use_signal(settings::current);
    use_hook(move || {
        spawn(async move {
            let mut changed = settings::CHANGED.subscribe();
            while changed.recv().await.is_ok() {
                now.set(settings::current());
            }
        });
    });
    // The window in the theme's colours, as the main one is.
    let root_style = {
        let theme = now.read().theme.clone();
        let palette = crate::themes::palette(&theme, &Settings::themes_dir()).unwrap_or_default();
        format!(
            ":root{{{}}}",
            crate::themes::css(&crate::themes::window(&palette))
        )
    };
    rsx! {
        style { "{root_style}" }
        div {
            class: "viewer",
            tabindex: "0",
            onmounted: move |e| async move {
                let _ = e.set_focus(true).await;
            },
            onkeydown: move |e: KeyboardEvent| {
                let w = matches!(e.key(), Key::Character(ref c) if c.eq_ignore_ascii_case("w"));
                if e.key() == Key::Escape || (e.modifiers().meta() && w) {
                    e.prevent_default();
                    window().close();
                }
            },
            img { src: "{src}", alt: "{title}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_opens_as_large_as_the_image_within_the_screen_s_reach() {
        assert_eq!(first_size(Some((640, 480))), (640.0, 480.0));
        assert_eq!(first_size(Some((2400, 860))), (1200.0, 430.0));
        assert_eq!(first_size(Some((16, 16))), (320.0, 240.0));
        assert_eq!(first_size(None), (900.0, 680.0));
    }
}
