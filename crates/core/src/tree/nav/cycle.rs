//! Going from one row, waiting session, session or workspace to the next.

use std::collections::HashMap;
use std::path::PathBuf;

use adroit_model::{EnvironmentId, InboxItem, Session, Tree};

use super::Item;
use crate::content::Tab;
use crate::cursor;
use crate::tree::Place;

pub use crate::cursor::wrap;

/// The item `delta` steps from `current`, clamped to the ends. With nothing
/// selected the first item is chosen.
pub fn step(items: &[Item], current: Option<&Item>, delta: isize) -> Option<Item> {
    if items.is_empty() {
        return None;
    }
    let at = current.and_then(|c| items.iter().position(|i| i == c));
    let next = at.map_or(0, |i| cursor::clamp(i, delta, items.len()));
    Some(items[next].clone())
}

/// The next (or previous) waiting session after `current`, wrapping around,
/// each by its environment and its id there.
pub fn next_waiting(
    inbox: &[(EnvironmentId, InboxItem)],
    current: Option<(&EnvironmentId, &str)>,
    forward: bool,
) -> Option<(EnvironmentId, String)> {
    if inbox.is_empty() {
        return None;
    }
    let n = inbox.len();
    let at = current.and_then(|(environment, id)| {
        inbox
            .iter()
            .position(|(e, i)| e == environment && i.session.id == id)
    });
    let i = match (at, forward) {
        (Some(i), _) => cursor::wrap(i, if forward { 1 } else { -1 }, n),
        (None, true) => 0,
        (None, false) => n - 1,
    };
    let (environment, it) = &inbox[i];
    Some((environment.clone(), it.session.id.clone()))
}

/// The session after (or before) the one `shown`, over every session in the
/// sidebar's order, folded away or not, wrapping around. A tab that shows no
/// session stands after the sessions of its worktree, `place`; with nothing
/// shown, the first session is next and the last one before.
pub fn next_session(
    tree: &Tree,
    shown: Option<&Tab>,
    place: Option<&Place>,
    forward: bool,
) -> Option<String> {
    let all: Vec<(usize, &Session)> = tree
        .worktrees()
        .enumerate()
        .flat_map(|(w, node)| node.sessions.iter().map(move |s| (w, s)))
        .collect();
    if all.is_empty() {
        return None;
    }
    let n = all.len();
    let at = match shown {
        Some(Tab::Session(_, id)) => all.iter().position(|(_, s)| &s.id == id),
        _ => None,
    };
    let i = match at {
        Some(i) => cursor::wrap(i, if forward { 1 } else { -1 }, n),
        None => {
            let worktree =
                place.and_then(|p| tree.worktrees().position(|w| w.worktree.path == p.worktree));
            let after = worktree.map_or(0, |wt| all.iter().filter(|(w, _)| *w <= wt).count());
            cursor::wrap(after, if forward { 0 } else { -1 }, n)
        }
    };
    Some(all[i].1.id.clone())
}

/// A session of the workspace after (or before) the one shown, in the
/// tree's order, passing over workspaces with no session and wrapping
/// around: the session `last` shown there, by the workspace's root, or else
/// its first. A tab that shows no session goes from the workspace of its
/// worktree, `place`; with neither, the first workspace is next and the last
/// one before.
pub fn next_workspace(
    tree: &Tree,
    shown: Option<&Tab>,
    place: Option<&Place>,
    last: &HashMap<PathBuf, String>,
    forward: bool,
) -> Option<String> {
    let all = &tree.workspaces;
    let n = all.len();
    let session = match shown {
        Some(Tab::Session(_, id)) => tree.find(id).map(|(ws, _, _)| ws.root.clone()),
        _ => None,
    };
    let at = session
        .and_then(|root| all.iter().position(|ws| ws.root == root))
        .or_else(|| {
            let p = place?;
            all.iter()
                .position(|ws| ws.worktrees.iter().any(|w| w.worktree.path == p.worktree))
        });
    let order: Vec<usize> = match (at, forward) {
        (Some(i), true) => (1..=n).map(|d| (i + d) % n).collect(),
        (Some(i), false) => (1..=n).map(|d| (i + n - d) % n).collect(),
        (None, true) => (0..n).collect(),
        (None, false) => (0..n).rev().collect(),
    };
    let ws = order
        .into_iter()
        .map(|i| &all[i])
        .find(|ws| ws.sessions().next().is_some())?;
    let remembered = last
        .get(&ws.root)
        .filter(|id| ws.sessions().any(|s| &s.id == *id));
    remembered
        .cloned()
        .or_else(|| ws.sessions().next().map(|s| s.id.clone()))
}
