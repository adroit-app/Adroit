//! How large each console is drawn and its program sized: the cell the
//! console's font draws measured, the focused window's terminal fitted to
//! what the prompt card leaves of its console, every other window's to its
//! own, and each attached at that size.

use std::collections::HashSet;
use std::time::Duration;

use adroit_core::msg::Msg;
use adroit_term::Size;
use dioxus::prelude::*;

use super::{ensure_pane, needs_attach};
use crate::content::{Tab, WindowId, shows_prompt};
use crate::shell::Ui;

/// The console screen's top padding.
const SCREEN_TOP: f64 = 10.0;

/// The console's height kept clear besides the prompt card: the screen's top
/// padding and the card's margin below and above.
const CONSOLE_GAP: f64 = SCREEN_TOP + 14.0 + 8.0;

/// The width and height of one cell, measured with the console's own font.
const MEASURE: &str = include_str!("../../assets/js/measure.js");

/// A recovered transport need not change the active tab or the work tree.
/// Keep failed attachments retryable without depending on either changing.
fn use_attach_retries(wanted: impl Fn() -> bool + 'static) -> Signal<u64> {
    let mut retry = use_signal(|| 0);
    let wanted = use_hook(|| std::rc::Rc::new(wanted));
    use_future(move || {
        let wanted = wanted.clone();
        async move {
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if wanted() {
                    *retry.write() += 1;
                }
            }
        }
    });
    retry
}

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

/// What the focused window's console was last measured as: its box, the
/// prompt card's height when one is shown, and the cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Measured {
    pub console: (f64, f64),
    pub card: f64,
    pub cell: (f64, f64),
}

impl Measured {
    /// The terminal of the focused window's console showing `tab`: what the
    /// prompt card leaves of the console when that tab shows one, all of it
    /// otherwise; `None` until the console is measured.
    pub(crate) fn fitted(&self, tab: Option<&Tab>) -> Option<Size> {
        let (width, height) = self.console;
        if width <= 0.0 {
            return None;
        }
        let card = shows_prompt(tab).then_some(self.card);
        let (cw, ch) = self.cell;
        Some(fitted(width, console_room(height, card), cw, ch))
    }
}

impl crate::shell::Local {
    /// The console as it was last measured, read without subscribing.
    fn measured(&self) -> Measured {
        Measured {
            console: *self.console_box.peek(),
            card: *self.composer_height.peek(),
            cell: (*self.cell_width.peek(), *self.cell_height.peek()),
        }
    }

    /// The size the focused window's console gives `tab`'s program, even
    /// while another tab is shown: a session sized for a terminal shown
    /// meanwhile would lay its screen out under the prompt card.
    pub(crate) fn console_size_for(&self, tab: &Tab) -> Size {
        self.measured()
            .fitted(Some(tab))
            .unwrap_or_else(|| *self.console_size.peek())
    }
}

/// Keep every console of the shown tab measured, sized and attached.
pub(crate) fn use_console_sizes(ui: &Ui) {
    let app = ui.app;
    let local = ui.local;
    let (mut cell_width, mut cell_height) = (local.cell_width, local.cell_height);
    let (mut console_box, mut console_size) = (local.console_box, local.console_size);
    let mut attached = local.attached;
    let retry = use_attach_retries({
        let hub = ui.hub.clone();
        move || {
            let connected = |tab: &Tab| {
                app.remote_status_of(tab.environment())
                    .is_some_and(|status| status.phase == crate::link::RemotePhase::Connected)
            };
            app.content.peek().windows.current().is_some_and(|tab| {
                tab.windows().iter().any(|window| {
                    connected(&window.shows) && needs_attach(&window.shows, &hub.panes)
                })
            })
        }
    });
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
        let hosts = app.hosts.read();
        let work = hosts.work(hosts.environment_of(active.read().as_ref()));
        adroit_core::tree::listed(work).clone()
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

    // The console as measured now, read so that a change sizes again.
    let measured = move || Measured {
        console: console_box(),
        card: (local.composer_height)(),
        cell: (cell_width(), cell_height()),
    };

    // The console's terminal fills what the prompt card leaves of the console,
    // in cells of the measured width; all of it when no card is shown.
    use_effect(move || {
        let _ = face();
        let Some(next) = measured().fitted(active().as_ref()) else {
            return;
        };
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
            let _ = (face(), tree.read(), attached(), retry());
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

    // Attach the active tab's session at the size the console gives that tab,
    // not the size it gave the tab shown before: the effect above may not
    // have run yet for the tab now shown.
    use_effect({
        let hub = ui.hub.clone();
        move || {
            let Some(tab) = active() else { return };
            let _ = face();
            let size = measured()
                .fitted(Some(&tab))
                .unwrap_or_else(|| *console_size.read());
            // Read, so a session listed only now is attached to once it is,
            // and one attached is sized to the console as it is by then.
            let _ = (tree.read(), attached(), retry());
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
    use adroit_model::EnvironmentId;

    #[test]
    fn the_console_s_terminal_takes_the_prompt_card_s_room_when_no_card_is_shown() {
        let with_card = console_room(600.0, Some(120.0));
        let without = console_room(600.0, None);
        assert_eq!(with_card, 600.0 - 120.0 - CONSOLE_GAP);
        assert_eq!(without, 600.0 - SCREEN_TOP);
        assert!(without > with_card + 120.0);
    }

    #[test]
    fn a_tab_s_program_is_sized_by_whether_that_tab_shows_the_prompt() {
        let measured = Measured {
            console: (640.0, 600.0),
            card: 120.0,
            cell: (8.0, 20.0),
        };
        let session = measured
            .fitted(Some(&Tab::Session(EnvironmentId::unbound(), "a".into())))
            .unwrap();
        let terminal = measured
            .fitted(Some(&Tab::Terminal(EnvironmentId::unbound(), 1)))
            .unwrap();
        assert_eq!(session.rows, ((600.0 - 120.0 - CONSOLE_GAP) / 20.0) as u16);
        assert_eq!(terminal.rows, ((600.0 - SCREEN_TOP) / 20.0) as u16);
        assert_eq!(session.cols, terminal.cols);
    }

    #[test]
    fn a_console_not_measured_yet_sizes_no_program() {
        let measured = Measured {
            console: (0.0, 0.0),
            card: 120.0,
            cell: (8.0, 20.0),
        };
        assert_eq!(
            measured.fitted(Some(&Tab::Session(EnvironmentId::unbound(), "a".into()))),
            None
        );
    }

    #[tokio::test]
    async fn recovered_transport_retries_attachment_without_a_tab_or_snapshot_change() {
        use std::cell::Cell;
        use std::rc::Rc;

        #[derive(Clone)]
        struct State {
            connected: Rc<Cell<bool>>,
            detached: Rc<Cell<bool>>,
            attempts: Rc<Cell<usize>>,
        }
        #[component]
        fn Frame() -> Element {
            let state = use_context::<State>();
            let wanted = state.clone();
            let retry = use_attach_retries(move || wanted.connected.get() && wanted.detached.get());
            use_effect(move || {
                let _ = retry();
                state.attempts.set(state.attempts.get() + 1);
            });
            rsx! {}
        }
        async fn settle(dom: &mut VirtualDom) {
            let until = tokio::time::Instant::now() + Duration::from_millis(650);
            while tokio::time::Instant::now() < until {
                let _ = tokio::time::timeout_at(until, dom.wait_for_work()).await;
                dom.render_immediate(&mut dioxus::core::NoOpMutations);
            }
        }
        let state = State {
            connected: Rc::new(Cell::new(false)),
            detached: Rc::new(Cell::new(true)),
            attempts: Rc::new(Cell::new(0)),
        };
        let mut dom = VirtualDom::new(Frame).with_root_context(state.clone());
        dom.rebuild_in_place();
        settle(&mut dom).await;
        let first = state.attempts.get();
        state.connected.set(true);
        settle(&mut dom).await;
        assert!(
            state.attempts.get() > first,
            "the detached pane was never retried"
        );
        let retried = state.attempts.get();
        settle(&mut dom).await;
        assert!(
            state.attempts.get() > retried,
            "a failed attach was never retried"
        );
        state.detached.set(false);
        settle(&mut dom).await;
        let live = state.attempts.get();
        settle(&mut dom).await;
        assert_eq!(state.attempts.get(), live, "a live pane was attached again");
    }
}
