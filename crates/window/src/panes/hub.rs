//! The hub of the panes the window shows, made once: their daemon, the
//! panes themselves, the theme's palette they are drawn in, and the wakeups
//! that redraw them.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use adroit_protocol::Colours;
use adroit_term::Palette;
use dioxus::prelude::*;
use tokio::sync::mpsc;

use super::{close_shell, keep_drawn};
use crate::bridge::AppCtx;
use crate::console::{Hub, Panes};
use crate::content::Tab;
use crate::shell::Local;

/// The palette of `colours`, as the host read them.
pub(crate) fn palette_of(colours: &Colours) -> Palette {
    match colours {
        Colours::Theme { text } => Palette::from_ghostty(text).unwrap_or_default(),
        Colours::Own | Colours::Unreadable { .. } => Palette::default(),
    }
}

/// The hub, made once for the component that calls it and given to the
/// components below it.
pub(crate) fn use_hub(app: AppCtx, local: Local) -> Hub {
    // The theme's palette, which a new theme replaces.
    let palette = use_signal(|| {
        let host = app.host.peek();
        let theme = host
            .work
            .as_ref()
            .and_then(|w| w.config.configured.as_ref());
        theme.map_or_else(Palette::default, |c| palette_of(&c.theme.colours))
    });
    let hub = use_hook(|| {
        let (wake, woken) = mpsc::unbounded_channel::<Tab>();
        let panes: Panes = Rc::new(RefCell::new(HashMap::new()));
        spawn(redraw_woken(app, local, panes.clone(), woken));
        Hub {
            daemon: app.daemon(),
            panes,
            attaching: Rc::new(RefCell::new(HashSet::new())),
            wake,
            palette,
        }
    });
    use_context_provider({
        let hub = hub.clone();
        move || hub
    });
    hub
}

/// Coalesce wakeups into at most one redraw a frame, and only for the panes
/// on screen; another pane is drawn afresh when it is shown. The images a
/// console drew are kept, and a shell that has exited has its windows
/// closed.
async fn redraw_woken(
    app: AppCtx,
    local: Local,
    panes: Panes,
    mut woken: mpsc::UnboundedReceiver<Tab>,
) {
    let mut frame = local.frame;
    while let Some(first) = woken.recv().await {
        tokio::time::sleep(Duration::from_millis(16)).await;
        let mut woke = vec![first];
        while let Ok(t) = woken.try_recv() {
            woke.push(t);
        }
        for tab in &woke {
            if let Tab::Session(_, id) = tab {
                let drawn = panes
                    .borrow()
                    .get(tab)
                    .map(|p| p.take_drawn())
                    .unwrap_or_default();
                if !drawn.is_empty() {
                    keep_drawn(local.gallery, id.clone(), drawn);
                }
            }
            let remote = app.remote_status().is_some();
            let gone = panes.borrow().get(tab).is_some_and(|p| {
                if remote {
                    p.program_exited()
                } else {
                    p.exited()
                }
            });
            if let (Tab::Terminal(_, id), true) = (tab, gone) {
                close_shell(app, &panes, *id);
            }
        }
        let on_screen = app
            .content
            .peek()
            .windows
            .current()
            .is_some_and(|t| t.windows().iter().any(|w| woke.contains(&w.shows)));
        if on_screen {
            *frame.write() += 1;
        }
    }
}

impl crate::shell::Ui {
    /// Draw every console open in `colours`: what is drawn, and what a
    /// program asking for its colours is told.
    pub(crate) fn repaint(&self, colours: &Colours) {
        let palette = palette_of(colours);
        for pane in self.hub.panes.borrow().values() {
            pane.set_palette(&palette);
        }
        let mut signal = self.hub.palette;
        signal.set(palette);
    }
}

/// Tell the host each time the screen of the session shown is drawn anew:
/// an agent reports no status for a dialog it opens itself, so the host
/// looks at the screen.
pub(crate) fn use_redraws_told(app: AppCtx, local: Local) {
    let active = use_memo(move || app.content.read().active().cloned());
    use_effect(move || {
        let _ = (local.frame)();
        if let Some(tab @ Tab::Session(_, id)) = &*active.read() {
            let agent = app.agent_of(Some(tab));
            app.tell_host(adroit_protocol::Request::Redrawn {
                session: id.clone(),
                agent,
            });
        }
    });
}
