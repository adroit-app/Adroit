//! Attaching to the sessions and shells the window shows, which panes are
//! live, and closing a shell's.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use adroit_core::msg::Msg;
use adroit_term::{Pane, Size};
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::console::{Hub, Panes};
use crate::content::Tab;
use crate::shell::Ui;
use adroit_model::daemon as sessions;

/// How long, in 50 ms steps, a prompt waits for a session pane that has just
/// attached.
pub(crate) const ATTACH_WAIT_STEPS: usize = 60;

/// How often the window tells when the user last typed into each session
/// and which sessions are live here, as their panes exit.
const LIVE_EVERY: Duration = Duration::from_secs(2);

/// The daemon's id of the session a tab shows; `None` for a draft and for a
/// session running in another terminal.
fn kept_id(tab: &Tab) -> Option<u128> {
    match tab {
        Tab::Session(id) => sessions::id_of(id),
        Tab::Terminal(id) => Some(*id),
        Tab::Draft { .. } | Tab::Viewer(_) => None,
    }
}

/// The pane a tab shows, attaching to its session when there is none yet:
/// `None` until the attach lands, when the tab is woken and `attached` told.
pub(crate) fn ensure_pane(
    app: AppCtx,
    hub: &Hub,
    tab: &Tab,
    size: Size,
    attached: impl FnOnce() + 'static,
) -> Option<Arc<Pane>> {
    if let Some(p) = hub.panes.borrow().get(tab)
        && !p.exited()
    {
        return Some(p.clone());
    }
    let id = kept_id(tab)?;
    if !hub.attaching.borrow_mut().insert(tab.clone()) {
        return None;
    }
    tell_live(app, hub);
    let hub = hub.clone();
    let tab = tab.clone();
    spawn(async move {
        let wake = hub.wake.clone();
        let me = tab.clone();
        let palette = hub.palette.peek().clone();
        let pane = Pane::attach(&hub.daemon, id, size, &palette, move || {
            let _ = wake.send(me.clone());
        })
        .await;
        hub.attaching.borrow_mut().remove(&tab);
        if let Ok(pane) = pane {
            hub.panes.borrow_mut().insert(tab.clone(), Arc::new(pane));
            attached();
            let _ = hub.wake.send(tab);
        }
        tell_live(app, &hub);
    });
    None
}

/// The pane a tab shows, waiting a while for its attach when there is none.
pub(crate) async fn pane_for(app: AppCtx, hub: &Hub, tab: &Tab, size: Size) -> Option<Arc<Pane>> {
    for _ in 0..ATTACH_WAIT_STEPS {
        if let Some(p) = ensure_pane(app, hub, tab, size, || {}) {
            return Some(p);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}

/// The sessions shown live here, or being attached to.
fn live_sessions(hub: &Hub) -> BTreeSet<String> {
    let panes = hub.panes.borrow();
    let attaching = hub.attaching.borrow();
    panes
        .iter()
        .filter(|(_, p)| !p.exited())
        .map(|(t, _)| t)
        .chain(attaching.iter())
        .filter_map(|t| match t {
            Tab::Session(id) => Some(id.clone()),
            _ => None,
        })
        .collect()
}

/// Tell the view state which sessions are shown live here: one started a
/// moment ago may not be listed yet, and is not taken for ended.
pub(crate) fn tell_live(app: AppCtx, hub: &Hub) {
    let live = live_sessions(hub);
    app.dispatch(Msg::PanesLive(live));
}

/// Tell the host, for as long as the window is open, when the user last
/// typed into each session, and the view state which sessions are live
/// here, as their panes exit.
pub(crate) async fn tell_live_on(app: AppCtx, hub: Hub) {
    let mut typed = BTreeMap::new();
    let mut told_live = None;
    loop {
        let inputs: BTreeMap<u128, u64> = hub
            .panes
            .borrow()
            .iter()
            .filter(|(tab, _)| matches!(tab, Tab::Session(_)))
            .filter_map(|(_, pane)| pane.last_input_ms().map(|at| (pane.id(), at)))
            .collect();
        if inputs != typed {
            typed = inputs.clone();
            app.dispatch(Msg::Typed(inputs));
        }
        let live = live_sessions(&hub);
        if told_live.as_ref() != Some(&live) {
            told_live = Some(live.clone());
            app.dispatch(Msg::PanesLive(live));
        }
        tokio::time::sleep(LIVE_EVERY).await;
    }
}

impl Ui {
    /// End a shell and close its windows, showing another thing of its
    /// worktree if no window is left.
    pub(crate) fn end_shell(&self, id: u128) {
        self.app
            .tell_host(adroit_protocol::Request::EndTerminal { id });
        close_shell(self.app, &self.hub.panes, id);
    }

    /// Keep the panes and the images drawn only of these sessions and
    /// terminals: one no longer listed has nothing to show.
    pub(crate) fn keep_panes(&self, sessions: &BTreeSet<String>, terminals: &BTreeSet<u128>) {
        self.hub.panes.borrow_mut().retain(|tab, _| match tab {
            Tab::Session(id) => sessions.contains(id),
            Tab::Terminal(id) => terminals.contains(id),
            Tab::Draft { .. } | Tab::Viewer(_) => true,
        });
        let mut gallery = self.local.gallery;
        let gone = gallery
            .peek()
            .store
            .sessions()
            .any(|id| !sessions.contains(id));
        if gone {
            gallery.write().store.retain(|id| sessions.contains(id));
        }
    }
}

/// Close a shell's windows, and show another thing of its worktree if no
/// window is left.
pub(crate) fn close_shell(app: AppCtx, panes: &Panes, id: u128) {
    panes.borrow_mut().remove(&Tab::Terminal(id));
    app.dispatch(Msg::ShellClosed(id));
}
