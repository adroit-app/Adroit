//! What a listing of the sessions means for what the window shows: looked at
//! each time a host lists them anew, and whenever which sessions are live
//! here changes, as a session ending changes what it means.

use std::collections::BTreeSet;

use adroit_model::{EnvironmentId, State, Tree};

use super::{Content, ContentView, Kept, Tab, close_showing, from_kept};
use crate::chrome::status_line;
use crate::effect::Effect;
use crate::host::Hosts;
use crate::state::ViewState;
use crate::tree::nav::{self, Item, Place};

/// The sessions waiting in `next` that were not waiting in `prev`, each as
/// its name and what it waits for.
pub fn newly_waiting(prev: &Tree, next: &Tree) -> Vec<(String, String)> {
    let waited = |id: &str| {
        prev.sessions()
            .any(|(_, s)| s.id == id && matches!(s.state, State::Waiting(_)))
    };
    next.sessions()
        .filter_map(|(_, s)| match &s.state {
            State::Waiting(why) if !waited(&s.id) => Some((s.name.clone(), why.to_string())),
            _ => None,
        })
        .collect()
}

/// The content as the window last closed, coming back environment by
/// environment as each one's host first lists its sessions.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Restore {
    kept: Kept<String>,
    /// The environments whose listing the content came back with.
    listed: BTreeSet<EnvironmentId>,
    /// The windows as they last came back, so a change made since tells
    /// that the content is no longer the one kept.
    made: Option<Content<Tab>>,
}

impl Restore {
    pub(crate) fn new(kept: Kept<String>) -> Self {
        Self {
            kept,
            listed: BTreeSet::new(),
            made: None,
        }
    }

    /// What to keep for the next run while the windows are as they came
    /// back: the content kept, tabs of environments not listed yet with it.
    pub(crate) fn kept_while_unchanged(&self, windows: &Content<Tab>) -> Option<&Kept<String>> {
        (self.made.as_ref() == Some(windows)).then_some(&self.kept)
    }

    /// The environments the kept content has a tab of, `window`'s for a tab
    /// kept without one.
    fn environments(&self, window: &EnvironmentId) -> BTreeSet<EnvironmentId> {
        Content::from_kept(self.kept.clone(), Some)
            .shown()
            .into_iter()
            .filter_map(|name| from_kept(name, window))
            .map(|tab| tab.environment().clone())
            .collect()
    }
}

/// The sessions shown live here, or being attached to, are now `live`.
pub(crate) fn tell_live(
    view: &mut ContentView,
    live: BTreeSet<(EnvironmentId, String)>,
) -> Vec<Effect> {
    view.live = live;
    Vec::new()
}

/// Every environment's listing looked at anew, when it or the sessions live
/// here changed since it last was.
pub(crate) fn listed(state: &mut ViewState) -> Vec<Effect> {
    let environments: Vec<EnvironmentId> = state.hosts.each().map(|(e, _)| e.clone()).collect();
    let mut effects = Vec::new();
    for environment in environments {
        effects.extend(listed_in(state, &environment));
    }
    effects
}

/// Where `tab` is in what `hosts` list, none for a tab nothing listed
/// shows.
fn listed_at(hosts: &Hosts, tab: &Tab) -> Option<Place> {
    let listed = hosts
        .work(tab.environment())
        .and_then(|w| w.tree.listed.as_deref())?;
    match tab {
        Tab::Session(environment, id) => {
            nav::find(&listed.tree, id).map(|(p, _)| Place::at(environment, p))
        }
        Tab::Terminal(environment, id) => listed
            .terminals
            .iter()
            .find(|s| s.id == *id)
            .map(|s| Place::at(environment, s.cwd.clone())),
        Tab::Draft { .. } | Tab::Viewer(_, _) | Tab::Drawn { .. } => None,
    }
}

/// `environment`'s listing looked at anew, when it or the sessions live
/// here changed since it last was.
fn listed_in(state: &mut ViewState, environment: &EnvironmentId) -> Vec<Effect> {
    let Some(work) = state.hosts.work(environment) else {
        return Vec::new();
    };
    let Some(now) = work.tree.listed.as_deref() else {
        return Vec::new();
    };
    let view = &mut state.content;
    let live: BTreeSet<String> = view
        .live
        .iter()
        .filter(|(e, _)| e == environment)
        .map(|(_, id)| id.clone())
        .collect();
    let key = (work.seq, live.clone());
    let looked = view.looked.get(environment);
    let seen = looked.is_some();
    let anew = looked.is_none_or(|(seq, ..)| *seq != work.seq);
    if looked == Some(&key) {
        return Vec::new();
    }
    view.looked.insert(environment.clone(), key);
    let (sessions, kept_shells, next) = (&now.sessions, &now.terminals, &now.tree);
    let was = view
        .looked_at
        .insert(environment.clone(), next.clone())
        .unwrap_or_default();
    let mut effects = Vec::new();
    // A session started a moment ago, and shown, is placed among its
    // worktree's tabs once listed: below the worktree's top, it is shown
    // there.
    if let Some(Tab::Session(shown_in, id)) = view.active()
        && shown_in == environment
        && view.place.is_some()
        && let Some(p) = nav::find(next, id).map(|(p, _)| Place::at(environment, p))
        && view.place.as_ref() != Some(&p)
        && nav::find(&was, id).is_none()
    {
        view.place = Some(p);
    }
    let of_here =
        |tab: &Tab, id: &str| matches!(tab, Tab::Session(e, s) if e == environment && s == id);
    // A session shown and no longer listed is not in the tree looked at
    // next, though it may stay live here a while yet.
    let shown = view.windows.shown();
    for t in &shown {
        if let Tab::Session(e, id) = t
            && e == environment
            && nav::is_gone(sessions, id)
            && let Some((_, s)) = nav::find(&was, id)
        {
            view.unlisted
                .insert((environment.clone(), id.clone()), s.name.clone());
        }
    }
    view.unlisted.retain(|(e, id), _| {
        e != environment || (nav::is_gone(sessions, id) && shown.iter().any(|t| of_here(t, id)))
    });
    // A session gone from the list gives its windows up as a closed shell
    // does.
    let ended: Vec<String> = view
        .windows
        .shown()
        .into_iter()
        .filter_map(|t| match t {
            Tab::Session(e, id)
                if e == environment && nav::is_gone(sessions, id) && !live.contains(id) =>
            {
                Some(id.clone())
            }
            _ => None,
        })
        .collect();
    for id in &ended {
        let after = view
            .place
            .as_ref()
            .filter(|p| p.environment == *environment)
            .and_then(|p| {
                let left: Vec<u128> = kept_shells
                    .iter()
                    .filter(|s| s.cwd == p.worktree)
                    .map(|s| s.id)
                    .collect();
                nav::tab_after_closing(environment, next, p, &left)
            });
        // Said, or the window would vanish unexplained.
        if let Some(name) = view.unlisted.remove(&(environment.clone(), id.clone())) {
            effects.extend(status_line::notice(
                &mut state.chrome.status,
                format!("{name} ended"),
            ));
        }
        let gone = Tab::Session(environment.clone(), id.clone());
        close_showing(view, |t| *t == gone, after);
    }
    // A session no longer listed has nothing to show; its pane is closed,
    // and the images its console drew are not kept. Looked at only when the
    // host lists anew, so a pane started since the listing stays.
    if anew {
        effects.push(Effect::KeepPanes {
            environment: environment.clone(),
            sessions: sessions.iter().map(|s| s.id.clone()).collect(),
            terminals: kept_shells.iter().map(|s| s.id).collect(),
        });
    }
    // Told only of what changed since the window first read the sessions,
    // and only when the settings ask for it.
    if seen && state.config.settings.notify_waiting {
        let waiting = newly_waiting(&was, next);
        if !waiting.is_empty() {
            effects.push(Effect::NotifyWaiting(waiting));
        }
    }
    restore_listed(state, environment);
    state.content.restored = true;
    effects
}

/// The content as the window last closed comes back with what it shows that
/// is listed, now with `environment`'s tabs too, unless it was changed since
/// it came back or something was opened before it could.
fn restore_listed(state: &mut ViewState, environment: &EnvironmentId) {
    let view = &mut state.content;
    let Some(mut restore) = view.restore.take() else {
        return;
    };
    if !restore.listed.insert(environment.clone()) {
        view.restore = Some(restore);
        return;
    }
    let unchanged = restore
        .made
        .as_ref()
        .map_or(view.windows.is_empty(), |made| *made == view.windows);
    if !unchanged {
        return;
    }
    let own = &state.hosts.environment;
    let hosts = &state.hosts;
    let listed = &restore.listed;
    let was = view.active().cloned();
    view.windows = Content::from_kept(restore.kept.clone(), |name| {
        from_kept(&name, own)
            .filter(|tab| listed.contains(tab.environment()) && listed_at(hosts, tab).is_some())
    });
    restore.made = Some(view.windows.clone());
    let done = restore.environments(own).is_subset(&restore.listed);
    let now = view.active().cloned();
    if now != was
        && let Some(tab) = now
    {
        if let Tab::Session(environment, id) = &tab {
            state.tree.selected = Some(Item::Session(environment.clone(), id.clone()));
        }
        view.place = listed_at(hosts, &tab);
    }
    if !done {
        view.restore = Some(restore);
    }
}

#[cfg(test)]
mod tests {
    use adroit_model::{State, Tree, Wait};

    use super::*;
    use crate::testing::session;

    fn tree(states: &[(&str, State)]) -> Tree {
        let sessions = states
            .iter()
            .map(|(id, state)| session(id).cwd("/tmp").state(state.clone()));
        adroit_model::testing::tree().sessions(sessions).build()
    }

    #[test]
    fn only_a_session_that_starts_waiting_is_told_of() {
        let before = tree(&[("a", State::Working), ("b", State::Waiting(Wait::Input))]);
        let after = tree(&[
            ("a", State::Waiting(Wait::Permission)),
            ("b", State::Waiting(Wait::Input)),
            ("c", State::Waiting(Wait::Input)),
        ]);
        assert_eq!(
            newly_waiting(&before, &after),
            [
                ("a-name".to_string(), "permission prompt".to_string()),
                ("c-name".to_string(), "waiting for input".to_string()),
            ]
        );
        assert!(newly_waiting(&after, &after).is_empty());
    }
}
