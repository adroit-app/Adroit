//! The prompt's Neovim as the window runs it: started, and started again
//! when it quits, as `:q` makes it; sized to the prompt card, its rows to
//! the lines it asks for within the settings' bounds.

use std::sync::Arc;
use std::time::{Duration, Instant};

use adroit_core::msg::Msg;
use adroit_nvim::{Embedded, Event as NvimEvent};
use dioxus::prelude::*;
use tokio::sync::mpsc;

use super::{mode_of, nvim_program, prompt_rows};
use crate::bridge::AppCtx;
use crate::settings;

/// How long the prompt's Neovim has to live to be started again once it
/// quits.
const NVIM_SETTLE: Duration = Duration::from_secs(2);

/// The prompt's Neovim and the grid the window draws it in.
#[derive(Clone, Copy)]
pub(crate) struct PromptNeovim {
    pub nvim: Signal<Option<Arc<Embedded>>>,
    pub prompt_frame: Signal<u64>,
    /// The line the prompt shows from, and how many it has.
    pub prompt_view: Signal<(usize, usize)>,
    /// The lines the prompt shows.
    pub prompt_rows: Signal<usize>,
    /// The lines Neovim asks for, which the settings bound.
    prompt_height: Signal<usize>,
    prompt_cols: Signal<u16>,
}

/// The prompt's Neovim, its card `prompt_width` wide in cells `cell_width`
/// wide, drawn taller by a row while images are attached. `commands_wanted`
/// is told when a `/` asks for the agent's commands.
pub(crate) fn use_prompt_neovim(
    app: AppCtx,
    has_images: Memo<bool>,
    cell_width: Signal<f64>,
    prompt_width: Signal<f64>,
    commands_wanted: Callback<()>,
) -> PromptNeovim {
    let prompt = PromptNeovim {
        nvim: use_signal(|| None),
        prompt_frame: use_signal(|| 0),
        prompt_view: use_signal(|| (0, 0)),
        prompt_rows: use_signal(|| settings::current().prompt_min_rows),
        prompt_height: use_signal(|| 1),
        prompt_cols: use_signal(|| 80),
    };
    use_effect(move || {
        let has_images = has_images();
        if let Some(rows) = prompt.fit(has_images)
            && let Some(nv) = prompt.nvim.peek().clone()
        {
            let cols = *prompt.prompt_cols.peek();
            spawn(async move {
                let _ = nv.resize(cols, rows as u16).await;
            });
        }
    });
    use_hook(move || {
        spawn(async move { prompt.run(app, has_images, commands_wanted).await });
    });
    // The prompt's grid is as wide as its box allows.
    use_effect(move || {
        let width = prompt_width();
        let cw = cell_width();
        let nv = (prompt.nvim)();
        if width <= 0.0 {
            return;
        }
        let cols = ((width - 28.0) / cw).floor().max(10.0) as u16;
        if cols != *prompt.prompt_cols.peek() {
            let mut prompt_cols = prompt.prompt_cols;
            prompt_cols.set(cols);
        }
        if let Some(nv) = nv {
            let rows = *prompt.prompt_rows.peek() as u16;
            spawn(async move {
                let _ = nv.resize(cols, rows).await;
            });
        }
    });
    prompt
}

impl PromptNeovim {
    /// The rows the prompt shows for the lines Neovim asks for, as the
    /// settings bound them, when they changed.
    fn fit(&self, has_images: bool) -> Option<usize> {
        let s = settings::current();
        let rows = prompt_rows(
            *self.prompt_height.peek(),
            has_images,
            s.prompt_min_rows,
            s.prompt_max_rows,
        );
        if rows == *self.prompt_rows.peek() {
            return None;
        }
        let mut prompt_rows = self.prompt_rows;
        prompt_rows.set(rows);
        Some(rows)
    }

    /// The prompt's rows fitted again to settings read anew.
    pub(crate) async fn refit(&self, has_images: bool) {
        let Some(rows) = self.fit(has_images) else {
            return;
        };
        let embedded = self.nvim.peek().clone();
        if let Some(e) = embedded {
            let cols = *self.prompt_cols.peek();
            let _ = e.resize(cols, rows as u16).await;
        }
    }

    async fn run(self, app: AppCtx, has_images: Memo<bool>, commands_wanted: Callback<()>) {
        let mut nvim = self.nvim;
        let mut prompt_frame = self.prompt_frame;
        let mut prompt_view = self.prompt_view;
        let mut prompt_height = self.prompt_height;
        loop {
            let (tx, mut rx) = mpsc::unbounded_channel();
            let started = Embedded::spawn(&nvim_program(), &[], 80, 1, move |ev| {
                let _ = tx.send(ev);
            })
            .await;
            let embedded = match started {
                Ok(e) => Arc::new(e),
                Err(e) => {
                    app.status(Some(format!("cannot start Neovim: {e}")));
                    return;
                }
            };
            let born = Instant::now();
            nvim.set(Some(embedded.clone()));
            app.dispatch(Msg::PromptRuns(true));
            let rows = *self.prompt_rows.peek() as u16;
            let cols = *self.prompt_cols.peek();
            let _ = embedded.resize(cols, rows).await;
            while let Some(ev) = rx.recv().await {
                match ev {
                    NvimEvent::Redraw => {
                        *prompt_frame.write() += 1;
                        let m = mode_of(&embedded.state());
                        if app.prompt.peek().mode != m {
                            app.dispatch(Msg::PromptMode(m.0, m.1));
                        }
                    }
                    // Only the console's reader tells of yanks and of where
                    // it is scrolled to.
                    NvimEvent::Yank { .. } | NvimEvent::Scrolled { .. } => {}
                    NvimEvent::Shell(on) => app.dispatch(Msg::PromptShell(on)),
                    NvimEvent::CommandsWanted => commands_wanted(()),
                    NvimEvent::View { top, lines } => {
                        if *prompt_view.peek() != (top, lines) {
                            prompt_view.set((top, lines));
                        }
                    }
                    NvimEvent::Height(h) => {
                        prompt_height.set(h);
                        let images = *has_images.peek();
                        if let Some(rows) = self.fit(images) {
                            let cols = *self.prompt_cols.peek();
                            let _ = embedded.resize(cols, rows as u16).await;
                        }
                    }
                }
            }
            nvim.set(None);
            app.dispatch(Msg::PromptRuns(false));
            // One that quits as it starts would be started forever.
            if born.elapsed() < NVIM_SETTLE {
                app.status(Some(
                    "Neovim quit as it started; restart adroit to try again".into(),
                ));
                return;
            }
            app.notice("Neovim quit; the prompt starts afresh".into());
        }
    }
}
