//! What a listing of the sessions means for what the window shows: looked at
//! each time the host lists them anew, and whenever which sessions are live
//! here changes, as a session ending changes what it means.

use std::collections::BTreeSet;

use adroit_model::{State, Tree};

use super::{ContentView, Tab, close_showing};
use crate::chrome::status_line;
use crate::effect::Effect;
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

/// The sessions shown live here, or being attached to, are now `live`.
pub(crate) fn tell_live(view: &mut ContentView, live: BTreeSet<String>) -> Vec<Effect> {
    view.live = live;
    Vec::new()
}

/// The listing looked at anew, when it or the sessions live here changed
/// since it last was.
pub(crate) fn listed(state: &mut ViewState) -> Vec<Effect> {
    let Some(work) = state.host.work.as_ref() else {
        return Vec::new();
    };
    let Some(now) = work.tree.listed.as_deref() else {
        return Vec::new();
    };
    let view = &mut state.content;
    let key = (work.seq, view.live.clone());
    let anew = view
        .looked
        .as_ref()
        .is_none_or(|(seq, ..)| *seq != work.seq);
    if view.looked.as_ref() == Some(&key) {
        return Vec::new();
    }
    view.looked = Some(key);
    let (sessions, kept_shells, next) = (&now.sessions, &now.terminals, &now.tree);
    let was = std::mem::replace(&mut view.looked_at, next.clone());
    let mut effects = Vec::new();
    // A session started a moment ago, and shown, is placed among its
    // worktree's tabs once listed: below the worktree's top, it is shown
    // there.
    if let Some(Tab::Session(id)) = view.active()
        && view.place.is_some()
        && let Some((p, _)) = nav::find(next, id)
        && view.place.as_ref() != Some(&p)
        && nav::find(&was, id).is_none()
    {
        view.place = Some(p);
    }
    // A session gone from the list gives its windows up as a closed shell
    // does.
    let ended: Vec<String> = view
        .windows
        .shown()
        .into_iter()
        .filter_map(|t| match t {
            Tab::Session(id) if nav::is_gone(sessions, id) && !view.live.contains(id) => {
                Some(id.clone())
            }
            _ => None,
        })
        .collect();
    for id in &ended {
        let after = view.place.as_ref().and_then(|p| {
            let left: Vec<u128> = kept_shells
                .iter()
                .filter(|s| s.cwd == p.worktree)
                .map(|s| s.id)
                .collect();
            nav::tab_after_closing(next, p, &left)
        });
        // Said, or the window would vanish unexplained.
        if let Some((_, s)) = nav::find(&was, id) {
            effects.extend(status_line::notice(
                &mut state.chrome.status,
                format!("{} ended", s.name),
            ));
        }
        let gone = Tab::Session(id.clone());
        close_showing(view, |t| *t == gone, after);
    }
    // A session no longer listed has nothing to show; its pane is closed,
    // and the images its console drew are not kept. Looked at only when the
    // host lists anew, so a pane started since the listing stays.
    if anew {
        effects.push(Effect::KeepPanes {
            sessions: sessions.iter().map(|s| s.id.clone()).collect(),
            terminals: kept_shells.iter().map(|s| s.id).collect(),
        });
    }
    // The tab shown when the window last closed comes back once what it
    // shows is listed, unless another was opened meanwhile.
    if let Some(kept) = view.restore.take()
        && view.active().is_none()
    {
        let at = match &kept {
            Tab::Session(id) => nav::find(next, id).map(|(p, _)| p),
            Tab::Terminal(id) => kept_shells.iter().find(|s| s.id == *id).map(|s| Place {
                worktree: s.cwd.clone(),
            }),
            Tab::Draft { .. } | Tab::Viewer(_) => None,
        };
        if let Some(p) = at {
            if let Tab::Session(id) = &kept {
                state.tree.selected = Some(Item::Session(id.clone()));
            }
            view.place = Some(p);
            view.windows.open(kept);
        }
    }
    // Told only of what changed since the window first read the sessions,
    // and only when the settings ask for it.
    if view.restored && state.config.settings.notify_waiting {
        let waiting = newly_waiting(&was, next);
        if !waiting.is_empty() {
            effects.push(Effect::NotifyWaiting(waiting));
        }
    }
    view.restored = true;
    effects
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use adroit_model::{Session, State, Tree, Wait, build};

    use super::*;

    fn tree(states: &[(&str, State)]) -> Tree {
        let sessions: Vec<Session> = states
            .iter()
            .map(|(id, state)| Session {
                id: id.to_string(),
                name: format!("{id}-name"),
                cwd: "/tmp".into(),
                state: state.clone(),
                ..Default::default()
            })
            .collect();
        build(&sessions, &[], &HashMap::new(), &[], None)
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
