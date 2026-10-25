//! The console's reader as the window runs it: a Neovim of its own, started
//! again whenever it quits and sized as the console, shown what the console
//! holds when reading starts and again as it changes while it is read.

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use adroit_core::msg::Msg;
use adroit_nvim::{Embedded, Event as NvimEvent, Mark};
use adroit_term::{Pane, Size};
use dioxus::prelude::*;
use tokio::sync::{Notify, mpsc};

use super::Hub;
use super::restart::Restarts;
use crate::bridge::AppCtx;
use crate::console_read;

/// The least time between showing the console's reader what the console
/// holds and showing it again, while the console keeps changing: each
/// showing hands Neovim the whole console.
const READER_GAP: Duration = Duration::from_millis(30);

/// The reader and what the window draws of it.
#[derive(Clone)]
pub(crate) struct Reader {
    pub reader: Signal<Option<Arc<Embedded>>>,
    pub reader_frame: Signal<u64>,
    /// The line and column the reader shows from, which images follow.
    pub reader_view: Signal<(usize, usize)>,
    /// How many lines the reader holds, and how many of them are the
    /// screen, the last.
    pub read_rows: Signal<(usize, usize)>,
    app: AppCtx,
    hub: Hub,
    frame: Signal<u64>,
    console_size: Signal<Size>,
}

/// The reader started for the console drawn at `console_size`, its pane
/// redrawn as `frame` moves.
pub(crate) fn use_reader(
    app: AppCtx,
    hub: &Hub,
    frame: Signal<u64>,
    console_size: Signal<Size>,
) -> Reader {
    let reader = Reader {
        reader: use_signal(|| None),
        reader_frame: use_signal(|| 0),
        reader_view: use_signal(|| (0, 0)),
        read_rows: use_signal(|| (0, 0)),
        app,
        hub: hub.clone(),
        frame,
        console_size,
    };
    // Told when the console changes while it is read.
    let console_changed = use_hook(|| Rc::new(Notify::new()));
    use_effect({
        let console_changed = console_changed.clone();
        move || {
            let _ = frame();
            if app.console.read().reading {
                console_changed.notify_one();
            }
        }
    });
    use_hook({
        let reader = reader.clone();
        move || {
            let mut started = reader.clone();
            spawn(async move { started.run().await });
            spawn(async move { reader.follow(console_changed).await });
        }
    });
    use_effect({
        let reader = reader.reader;
        move || {
            let size = console_size();
            if let Some(r) = reader.peek().clone() {
                spawn(async move {
                    let _ = r.resize(size.cols, size.rows).await;
                });
            }
        }
    });
    reader
}

impl Reader {
    fn pane(&self) -> Option<Arc<Pane>> {
        let tab = self.app.active()?;
        self.hub.panes.borrow().get(&tab).cloned()
    }

    /// Start the reader, tell the window what it does until it quits, and
    /// start it again.
    async fn run(&mut self) {
        let app = self.app;
        let mut restarts = Restarts::default();
        // Whether the status line tells of a reader that could not run.
        let mut told = false;
        loop {
            let (tx, rx) = mpsc::unbounded_channel();
            let size = *self.console_size.peek();
            let started = Embedded::spawn_reader(
                &super::nvim_program(),
                &[],
                size.cols,
                size.rows,
                move |ev| {
                    let _ = tx.send(ev);
                },
            )
            .await;
            let (lived, trouble) = match started {
                Ok(r) => {
                    if told {
                        told = false;
                        app.status(None);
                    }
                    let born = Instant::now();
                    self.reader.set(Some(Arc::new(r)));
                    self.serve(rx).await;
                    self.reader.set(None);
                    app.dispatch(Msg::ReaderQuit);
                    (
                        Some(born.elapsed()),
                        "The console's reader quit as it started".to_string(),
                    )
                }
                Err(e) => (None, format!("cannot start the console's reader: {e}")),
            };
            let wait = restarts.wait(lived);
            if wait.is_zero() {
                app.notice("The console's reader quit; it starts afresh".into());
                continue;
            }
            app.status(Some(format!(
                "{trouble}; trying again in {}s",
                wait.as_secs()
            )));
            told = true;
            tokio::time::sleep(wait).await;
        }
    }

    /// Tell the window what the reader does until it quits.
    async fn serve(&mut self, mut rx: mpsc::UnboundedReceiver<NvimEvent>) {
        let app = self.app;
        while let Some(ev) = rx.recv().await {
            match ev {
                NvimEvent::Redraw => *self.reader_frame.write() += 1,
                NvimEvent::Scrolled { top, left } => self.reader_view.set((top, left)),
                NvimEvent::Yank { text, lines } => match crate::clipboard::copy_to_clipboard(&text)
                {
                    Ok(()) if lines == 1 => app.notice("Copied a line".into()),
                    Ok(()) => app.notice(format!("Copied {lines} lines")),
                    Err(e) => app.notice(format!("cannot copy: {e}")),
                },
                _ => {}
            }
        }
    }

    /// Show the reader what the console holds again as it changes while it
    /// is read.
    async fn follow(&self, console_changed: Rc<Notify>) {
        let mut shown = None::<u64>;
        loop {
            console_changed.notified().await;
            if !self.app.console.peek().reading {
                shown = None;
                continue;
            }
            let f = *self.frame.peek();
            if shown == Some(f) {
                continue;
            }
            shown = Some(f);
            let (Some(p), Some(r)) = (self.pane(), self.reader.peek().clone()) else {
                continue;
            };
            let Some((lines, marks, cur, background)) = self.everything(&p).await else {
                continue;
            };
            let _ = r.show(&lines, &marks, cur, false, &background).await;
            tokio::time::sleep(READER_GAP).await;
        }
    }

    /// Everything `p` holds, as the reader takes it, the cursor where the
    /// screen, at the end, has it, with the background it is drawn on.
    async fn everything(
        &self,
        p: &Pane,
    ) -> Option<(Vec<String>, Vec<Mark>, (usize, usize), String)> {
        let palette = self.hub.palette.peek().clone();
        let cols = self.console_size.peek().cols;
        let mut all = p.everything(&self.hub.daemon, cols).await.ok()?;
        let screen = p.screen();
        let above = all.rows.saturating_sub(screen.rows);
        all.cursor = screen.cursor.map(|c| adroit_term::Cursor {
            row: c.row + above,
            ..c
        });
        let mut read_rows = self.read_rows;
        read_rows.set((all.rows, screen.rows));
        let (lines, marks, cur) = console_read::reading(&all, palette.foreground);
        Some((lines, marks, cur, all.background.hex()))
    }

    /// Start reading the console, the cursor where the console's is, and give
    /// the reader `keys`, the ones that started it.
    pub(crate) fn enter(&self, keys: Vec<String>) {
        let (Some(p), Some(r)) = (self.pane(), self.reader.peek().clone()) else {
            return;
        };
        let reader = self.clone();
        spawn(async move {
            let Some((lines, marks, cur, background)) = reader.everything(&p).await else {
                return;
            };
            let _ = r.press("<C-\\><C-N>").await;
            if r.show(&lines, &marks, cur, true, &background).await.is_ok() {
                reader.app.dispatch(Msg::ReadingStarted);
                let _ = r.input(&keys.concat()).await;
            }
        });
    }

    /// The wheel over the reader: it scrolls the reader, and past the first
    /// or last line it holds, a program that scrolls what it shows itself,
    /// which the reader then follows.
    pub(crate) fn scroll(&self, lines: i32) {
        let Some(r) = self.reader.peek().clone() else {
            return;
        };
        let top = self.reader_view.peek().0;
        if let Some(p) = self.pane().filter(|p| p.takes_mouse()) {
            let page = r.state().screen().rows;
            let held = self.read_rows.peek().0;
            let past = (lines > 0 && top == 0) || (lines < 0 && top + page >= held);
            if past {
                p.wheel(lines, 0, 0);
                let mut frame = self.frame;
                *frame.write() += 1;
                return;
            }
        }
        let keys = if lines > 0 {
            format!("{lines}<C-y>")
        } else {
            format!("{}<C-e>", -lines)
        };
        spawn(async move {
            let _ = r.press(&keys).await;
        });
    }

    /// A click on the reader goes to the cell of the program's screen under
    /// it; the screen is the last of the lines the reader holds.
    pub(crate) fn click(&self, (col, row): (usize, usize)) {
        let Some(p) = self.pane() else {
            return;
        };
        let (top, left) = *self.reader_view.peek();
        let (held, screen) = *self.read_rows.peek();
        if let Some(row) = (top + row).checked_sub(held.saturating_sub(screen)) {
            p.click(left + col, row);
        }
    }
}
