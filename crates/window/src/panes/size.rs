//! How large each console is drawn and its program sized: the cell the
//! console's font draws measured, the focused window's terminal fitted to
//! what the prompt card leaves of its console, every other window's to its
//! own, and each attached at that size.

use std::collections::HashSet;
use std::time::Duration;

use adroit_core::msg::Msg;
use adroit_term::Size;
use dioxus::prelude::*;

use super::ensure_pane;
use crate::content::{Tab, WindowId, shows_prompt};
use crate::shell::Ui;

/// The console screen's top padding.
const SCREEN_TOP: f64 = 10.0;

/// The console's height kept clear besides the prompt card: the screen's top
/// padding and the card's margin below and above.
const CONSOLE_GAP: f64 = SCREEN_TOP + 14.0 + 8.0;

/// The width and height of one cell, measured with the console's own font.
const MEASURE: &str = include_str!("../../assets/js/measure.js");

/// The console's height left to its terminal, with the prompt card of this
/// height under it, if one is shown.
fn console_room(height: f64, card: Option<f64>) -> f64 {
    match card {
        Some(card) => height - card - CONSOLE_GAP,
        None => height - SCREEN_TOP,
    }
}

/// The terminal of a console drawn `width` wide with `room` for its rows,
/// in cells `cell_width` by `cell_height`.
fn fitted(width: f64, room: f64, cell_width: f64, cell_height: f64) -> Size {
    Size {
        cell_width,
        cell_height,
        ..Size::new(
            ((width - 24.0) / cell_width).floor().max(10.0) as u16,
            (room / cell_height).floor().max(4.0) as u16,
        )
    }
}

/// Keep every console of the shown tab measured, sized and attached.
pub(crate) fn use_console_sizes(ui: &Ui) {
    let app = ui.app;
    let local = ui.local;
    let (mut cell_width, mut cell_height) = (local.cell_width, local.cell_height);
    let (mut console_box, mut console_size) = (local.console_box, local.console_size);
    let mut attached = local.attached;
    // The font the console is drawn by, and the zoom, which the console is
    // measured again for.
    let face = use_memo(move || {
        let config = app.config.read();
        let s = &config.settings;
        (s.font_family.clone(), s.font_size, s.font_weight)
    });
    let zoomed = use_memo(move || app.chrome.read().zoom);
    let active = use_memo(move || app.content.read().active().cloned());
    let tree = use_memo(move || {
        let host = app.host.read();
        adroit_core::tree::listed(host.work.as_ref()).clone()
    });

    // The cell the console's font draws: as wide as its characters and as
    // tall as the font's own lines, as a terminal sizes it, measured again
    // whenever the font changes or the window is zoomed, with the room the
    // console has.
    use_effect(move || {
        let _ = face();
        let _ = zoomed();
        spawn(async move {
            // A zoom is laid out after the frame it was asked in.
            tokio::time::sleep(Duration::from_millis(50)).await;
            let mut e = document::eval(MEASURE);
            if let Ok((w, h)) = e.recv::<(f64, f64)>().await
                && w > 0.0
                && h > 0.0
            {
                let h = h.round();
                cell_height.set(h);
                cell_width.set(w);
            }
            // A resize is not told of a zoom, which changes the room in the
            // console's own pixels all the same.
            let mut room = document::eval(
                "const c = document.querySelector('.win.focused > .console, .content-windows > .console');
                 const r = c ? c.getBoundingClientRect() : {width: 0, height: 0};
                 dioxus.send([r.width, r.height]);",
            );
            if let Ok((w, h)) = room.recv::<(f64, f64)>().await
                && w > 0.0
                && *console_box.peek() != (w, h)
            {
                console_box.set((w, h));
            }
        });
    });

    // The console's terminal fills what the prompt card leaves of the console,
    // in cells of the measured width; all of it when no card is shown.
    use_effect(move || {
        let (width, height) = console_box();
        let cw = cell_width();
        let _ = (face(), cell_height());
        if width <= 0.0 {
            return;
        }
        let card = shows_prompt(active().as_ref()).then_some((local.composer_height)());
        let next = fitted(width, console_room(height, card), cw, cell_height());
        if *console_size.peek() != next {
            console_size.set(next);
        }
    });

    // Every other window of the shown tab shows its program at its own size;
    // one the focused window shows as well keeps the focused window's size.
    use_effect({
        let hub = ui.hub.clone();
        move || {
            // Taken out of the view state first: attaching tells it which
            // sessions are live.
            let windows: Vec<(WindowId, Tab)> = {
                let view = app.content.read();
                let Some(tab) = view.windows.current() else {
                    return;
                };
                let focused = view.active();
                let mut sized = HashSet::new();
                tab.windows()
                    .into_iter()
                    .filter(|w| {
                        Some(&w.shows) != focused
                            && !matches!(w.shows, Tab::Draft { .. })
                            && sized.insert(w.shows.clone())
                    })
                    .map(|w| (w.id, w.shows.clone()))
                    .collect()
            };
            let boxes = local.win_boxes.read().clone();
            let cw = cell_width();
            let ch = cell_height();
            let _ = (face(), tree.read(), attached());
            for (id, shows) in windows {
                let Some(&(width, height)) = boxes.get(&id).filter(|(w, _)| *w > 0.0) else {
                    continue;
                };
                let size = fitted(width, console_room(height, None), cw, ch);
                let me = shows.clone();
                let pane = ensure_pane(app, &hub, &shows, size, move || {
                    *attached.write() += 1;
                    app.dispatch(Msg::InputMayHold(me));
                });
                if let Some(pane) = pane {
                    pane.resize(size);
                }
            }
        }
    });

    // Attach the active tab's session at the console's size.
    use_effect({
        let hub = ui.hub.clone();
        move || {
            let Some(tab) = active() else { return };
            let size = console_size();
            // Read, so a session listed only now is attached to once it is,
            // and one attached is sized to the console as it is by then.
            let _ = (tree.read(), attached());
            let me = tab.clone();
            let pane = ensure_pane(app, &hub, &tab, size, move || {
                *attached.write() += 1;
                app.dispatch(Msg::InputMayHold(me));
            });
            if let Some(pane) = pane {
                pane.resize(size);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_console_s_terminal_takes_the_prompt_card_s_room_when_no_card_is_shown() {
        let with_card = console_room(600.0, Some(120.0));
        let without = console_room(600.0, None);
        assert_eq!(with_card, 600.0 - 120.0 - CONSOLE_GAP);
        assert_eq!(without, 600.0 - SCREEN_TOP);
        assert!(without > with_card + 120.0);
    }
}
