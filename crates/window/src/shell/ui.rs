//! Every handle the window keeps outside the view state, put together once
//! by the root component and given to what carries out the view state's
//! effects, takes the keys and draws the frame.

use std::sync::Arc;

use adroit_core::msg::Msg;
use adroit_term::Pane;
use dioxus::prelude::*;

use super::local::Local;
use crate::bridge::AppCtx;
use crate::console::{Consoles, Hub, PromptNeovim, Reader, use_prompt_neovim, use_reader};
use crate::prompts;

#[derive(Clone)]
pub(crate) struct Ui {
    pub app: AppCtx,
    pub hub: Hub,
    pub local: Local,
    pub consoles: Consoles,
    /// The console read in a Neovim of its own, for Vim's moves, search,
    /// selection and yank, while the console says it is read.
    pub reader: Reader,
}

/// The window's handles, with the prompt's Neovim and the console's reader
/// started, given to the components below as their context.
pub(crate) fn use_ui(app: AppCtx, hub: Hub, local: Local) -> Ui {
    let has_images = use_memo(move || !app.prompt.read().images.is_empty());
    // A `/` typed asks for what it offers again, if that is old.
    let commands_wanted = use_callback(move |()| app.dispatch(Msg::CommandsWanted));
    let PromptNeovim {
        nvim,
        prompt_frame,
        prompt_view,
        prompt_rows,
        ..
    } = use_prompt_neovim(
        app,
        has_images,
        local.cell_width,
        local.prompt_width,
        commands_wanted,
    );
    let prompts = use_hook(|| prompts::keeper(nvim, app));
    let reader = use_reader(app, &hub, local.frame, local.console_size);
    // Each Neovim, as it starts and as the theme changes, is told whether
    // the theme is light, or its text keeps the colours of its dark default.
    use_effect({
        let (palette, read) = (hub.palette, reader.reader);
        move || {
            let light = !palette.read().is_dark();
            for nv in [nvim(), read()].into_iter().flatten() {
                spawn(async move {
                    let _ = nv.set_light(light).await;
                });
            }
        }
    });
    let scroll_reader = use_callback({
        let reader = reader.clone();
        move |lines: i32| reader.scroll(lines)
    });
    let click_reader = use_callback({
        let reader = reader.clone();
        move |at: (usize, usize)| reader.click(at)
    });
    let consoles = Consoles {
        frame: local.frame,
        attached: local.attached,
        nvim,
        prompt_frame,
        prompt_view,
        prompt_rows,
        reader: reader.reader,
        reader_frame: reader.reader_frame,
        reader_view: reader.reader_view,
        read_rows: reader.read_rows,
        cell_width: local.cell_width,
        console_box: local.console_box,
        composer_height: local.composer_height,
        prompt_width: local.prompt_width,
        prompts,
        scroll_reader,
        click_reader,
    };
    use_context_provider(|| consoles);
    let ui = Ui {
        app,
        hub,
        local,
        consoles,
        reader,
    };
    use_context_provider({
        let ui = ui.clone();
        move || ui
    });
    ui
}

impl Ui {
    /// The pane of what the focused window shows, if it has one.
    pub(crate) fn active_pane(&self) -> Option<Arc<Pane>> {
        let tab = self.app.active()?;
        self.hub.panes.borrow().get(&tab).cloned()
    }
}
