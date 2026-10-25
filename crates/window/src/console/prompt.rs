//! The prompt's Neovim as the window runs it: started, and started again
//! when it quits, as `:q` makes it, or when its configuration is saved, with
//! what is written kept; sized to the prompt card, its rows to the lines it
//! asks for within the settings' bounds.

use std::sync::Arc;
use std::time::Instant;

use adroit_core::actions::Action;
use adroit_core::msg::Msg;
use adroit_nvim::{Embedded, Event as NvimEvent, View};
use dioxus::prelude::*;
use tokio::sync::mpsc;

use super::restart::Restarts;
use super::{mode_of, prompt_rows};
use crate::bridge::AppCtx;

/// The prompt's Neovim and the grid the window draws it in.
#[derive(Clone, Copy)]
pub(crate) struct PromptNeovim {
    pub nvim: Signal<Option<Arc<Embedded>>>,
    pub prompt_frame: Signal<u64>,
    /// The line the prompt shows from, and how many it has.
    pub prompt_view: Signal<(usize, usize)>,
    /// The lines the prompt shows.
    pub prompt_rows: Signal<usize>,
    /// The fewest and the most lines the settings let the prompt show.
    bounds: Memo<(usize, usize)>,
    /// The lines Neovim asks for, which the settings bound.
    prompt_height: Signal<usize>,
    prompt_cols: Signal<u16>,
}

/// The prompt's Neovim, its card `prompt_width` wide in cells `cell_width`
/// wide, drawn taller by a row while images are attached. `commands_wanted`
/// is told when a `/` asks for the agent's commands, and `asked` each action a
/// key or a command of the prompt asks for.
pub(crate) fn use_prompt_neovim(
    app: AppCtx,
    has_images: Memo<bool>,
    cell_width: Signal<f64>,
    prompt_width: Signal<f64>,
    commands_wanted: Callback<()>,
    asked: Callback<Action>,
) -> PromptNeovim {
    let prompt = PromptNeovim {
        nvim: use_signal(|| None),
        prompt_frame: use_signal(|| 0),
        prompt_view: use_signal(|| (0, 0)),
        prompt_rows: use_signal(|| app.config.peek().settings.prompt_min_rows),
        bounds: use_memo(move || {
            let s = &app.config.read().settings;
            (s.prompt_min_rows, s.prompt_max_rows)
        }),
        prompt_height: use_signal(|| 1),
        prompt_cols: use_signal(|| 80),
    };
    // Fitted again as images come and go, and as the settings bound it anew.
    use_effect(move || {
        let has_images = has_images();
        let _ = (prompt.bounds)();
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
        spawn(async move { prompt.run(app, has_images, commands_wanted, asked).await });
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
        let (least, most) = *self.bounds.peek();
        let rows = prompt_rows(*self.prompt_height.peek(), has_images, least, most);
        if rows == *self.prompt_rows.peek() {
            return None;
        }
        let mut prompt_rows = self.prompt_rows;
        prompt_rows.set(rows);
        Some(rows)
    }

    async fn run(
        self,
        app: AppCtx,
        has_images: Memo<bool>,
        commands_wanted: Callback<()>,
        asked: Callback<Action>,
    ) {
        let mut nvim = self.nvim;
        let mut restarts = Restarts::default();
        // Whether the status line tells of a Neovim that could not run.
        let mut told = false;
        // What was written in the Neovim before, for the one started next.
        let mut kept: Option<(String, View)> = None;
        loop {
            let (tx, rx) = mpsc::unbounded_channel();
            let started = Embedded::spawn(&adroit_nvim::program(), &[], 80, 1, move |ev| {
                let _ = tx.send(ev);
            })
            .await;
            let (lived, trouble) = match started {
                Ok(e) => {
                    if told {
                        told = false;
                        app.status(None);
                    }
                    let born = Instant::now();
                    if let Some((text, view)) = kept.take() {
                        let _ = e.put_back(&text).await;
                        let _ = e.set_view(&view).await;
                    }
                    let embedded = Arc::new(e);
                    nvim.set(Some(embedded.clone()));
                    app.dispatch(Msg::PromptRuns(true));
                    kept = self
                        .serve(&embedded, rx, app, has_images, commands_wanted, asked)
                        .await;
                    nvim.set(None);
                    app.dispatch(Msg::PromptRuns(false));
                    if kept.is_some() {
                        // Asked for, not quit, so not one that cannot start.
                        continue;
                    }
                    (
                        Some(born.elapsed()),
                        "Neovim quit as it started".to_string(),
                    )
                }
                Err(e) => (None, format!("cannot start Neovim: {e}")),
            };
            let wait = restarts.wait(lived);
            if wait.is_zero() {
                app.notice("Neovim quit; the prompt starts afresh".into());
                continue;
            }
            // One that quits as it starts is not started over and over.
            app.status(Some(format!(
                "{trouble}; trying again in {}s",
                wait.as_secs()
            )));
            told = true;
            tokio::time::sleep(wait).await;
        }
    }

    /// Tell the window what the prompt's Neovim does until it quits, or
    /// until its configuration is saved: then it is quit, and what is written
    /// in it is given back for the Neovim started next.
    async fn serve(
        &self,
        embedded: &Embedded,
        mut rx: mpsc::UnboundedReceiver<NvimEvent>,
        app: AppCtx,
        has_images: Memo<bool>,
        commands_wanted: Callback<()>,
        asked: Callback<Action>,
    ) -> Option<(String, View)> {
        let mut prompt_frame = self.prompt_frame;
        let mut prompt_view = self.prompt_view;
        let mut prompt_height = self.prompt_height;
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
                NvimEvent::Action(name) => match name.parse::<Action>() {
                    Ok(action) => asked(action),
                    Err(e) => app.notice(format!("cannot run {name}: {e}")),
                },
                NvimEvent::ConfigurationSaved => {
                    let (Ok(text), Ok(view)) = (embedded.text().await, embedded.view().await)
                    else {
                        continue;
                    };
                    let _ = embedded.quit().await;
                    app.notice("The prompt's configuration was saved; it starts again".into());
                    return Some((text, view));
                }
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
        None
    }
}
