//! Every handle the window keeps outside the view state, put together once
//! by the root component and given to what carries out the view state's
//! effects, takes the keys and draws the frame.

use std::collections::BTreeMap;
use std::sync::Arc;

use adroit_core::actions::Action;
use adroit_core::msg::Msg;
use adroit_model::EnvironmentId;
use adroit_term::Pane;
use dioxus::prelude::*;

use super::local::Local;
use crate::bridge::AppCtx;
use crate::console::{Consoles, Hub, PromptNeovim, Reader, use_prompt_neovim, use_reader};
use crate::link::RemoteStatus;
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
    // What a key or a command of the prompt's Neovim asks for runs as a
    // binding runs it, once the window's handles are put together below.
    let handles: CopyValue<Option<Ui>> = use_hook(|| CopyValue::new(None));
    let asked = use_callback(move |action: Action| {
        if let Some(ui) = handles.peek().clone() {
            ui.perform(action, None);
        }
    });
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
        asked,
    );
    let prompts = use_hook(|| prompts::keeper(nvim, app));
    use_future(move || async move {
        // Whether the prompt was last frozen, so one of an environment on
        // this computer is thawed once, not told so every time.
        let mut frozen = false;
        loop {
            let environment = app.active_environment();
            let remotes = app.remote_statuses();
            let next = prompt_frozen(
                &remotes,
                &environment,
                *app.drafts_ready.peek(),
                app.prompt.peek().is_sending(),
            );
            let nv = nvim.peek().clone();
            if let Some(nv) = nv
                && (remotes.contains_key(&environment) || next != frozen)
            {
                let _ = nv.freeze_prompt(next).await;
                frozen = next;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    });
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
    use_hook({
        let ui = ui.clone();
        move || {
            let mut handles = handles;
            handles.set(Some(ui));
        }
    });
    ui
}

/// Whether the prompt is frozen while it shows a tab of `environment`, as
/// `remotes` tell how the window stands with each environment on a work
/// machine: one on this computer is never, and one on a work machine is
/// while it may not be written to, what is written is not read back yet, or
/// a prompt is on its way.
fn prompt_frozen(
    remotes: &BTreeMap<EnvironmentId, RemoteStatus>,
    environment: &EnvironmentId,
    drafts_ready: bool,
    sending: bool,
) -> bool {
    remotes
        .get(environment)
        .is_some_and(|status| !status.writable || !drafts_ready || sending)
}

impl Ui {
    /// The pane of what the focused window shows, if it has one.
    pub(crate) fn active_pane(&self) -> Option<Arc<Pane>> {
        let tab = self.app.active()?;
        self.hub.panes.borrow().get(&tab).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_freezes_as_the_link_of_the_environment_of_the_tab_it_shows_stands() {
        let (here, there) = (EnvironmentId::unbound(), EnvironmentId::new("there"));
        let mut read_only = RemoteStatus::connected("box");
        read_only.writable = false;
        let remotes = BTreeMap::from([(there.clone(), read_only)]);
        // (environment shown, drafts ready, sending, frozen)
        let cases = [
            (&here, true, false, false),
            (&here, false, true, false),
            (&there, true, false, true),
        ];
        for (environment, ready, sending, frozen) in cases {
            assert_eq!(
                prompt_frozen(&remotes, environment, ready, sending),
                frozen,
                "{environment} {ready} {sending}"
            );
        }
        let writable = BTreeMap::from([(there.clone(), RemoteStatus::connected("box"))]);
        assert!(!prompt_frozen(&writable, &there, true, false));
        assert!(prompt_frozen(&writable, &there, false, false));
        assert!(prompt_frozen(&writable, &there, true, true));
    }
}
