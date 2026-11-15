//! Plain questions about the tree: where a session is, whether it is still
//! listed, the tree narrowed to a name, where a row works.

use std::collections::HashSet;
use std::path::Path;

use adroit_model::{Session, Tree};

use super::Item;
use crate::content::Tab;
use crate::picker::fuzzy::Query;
use crate::tree::Place;

/// Find a session by id, with the worktree it runs in.
pub fn find<'a>(tree: &'a Tree, id: &str) -> Option<(&'a Path, &'a Session)> {
    tree.find(id)
        .map(|(_, wt, s)| (wt.worktree.path.as_path(), s))
}

/// Whether the session `id` names is no longer listed.
pub fn is_gone(sessions: &[Session], id: &str) -> bool {
    !sessions.iter().any(|s| s.id == id)
}

/// Tabs kept for what they show, those of sessions no longer listed left out.
pub fn followed(sessions: &[Session], tabs: &HashSet<Tab>) -> HashSet<Tab> {
    tabs.iter()
        .filter(|tab| match tab {
            Tab::Session(_, id) => !is_gone(sessions, id),
            _ => true,
        })
        .cloned()
        .collect()
}

/// `tree` with only the sessions and terminals whose names match `text` as
/// the pickers match, and the worktrees and workspaces that still hold one;
/// a workspace or a worktree whose own name matches keeps everything beneath
/// it. A `text` that asks nothing keeps everything.
pub fn narrowed(tree: &Tree, text: &str) -> Tree {
    let query = Query::new(text);
    if query.is_empty() {
        return tree.clone();
    }
    let named = |name: &str| query.matches(name);
    let mut out = tree.clone();
    for ws in &mut out.workspaces {
        if named(&ws.name) {
            continue;
        }
        ws.worktrees.retain_mut(|w| {
            let own = w.worktree.branch.as_deref().is_some_and(named)
                || w.worktree
                    .path
                    .file_name()
                    .is_some_and(|f| named(&f.to_string_lossy()));
            if !own {
                w.sessions.retain(|s| named(&s.name));
                w.terminals.retain(|s| named(&s.name));
            }
            own || !w.sessions.is_empty() || !w.terminals.is_empty()
        });
    }
    out.workspaces
        .retain(|ws| named(&ws.name) || !ws.worktrees.is_empty());
    out
}

/// The worktree or directory an item works in: a session's own, the one a
/// shell was started in, a worktree itself, a workspace's main checkout or
/// directory.
pub fn place_of(tree: &Tree, item: &Item) -> Option<Place> {
    let at = |worktree: &Path| Place::at(item.environment(), worktree);
    match item {
        Item::Waiting(_, id) | Item::Session(_, id) => find(tree, id).map(|(p, _)| at(p)),
        Item::Terminal(_, id) => tree.terminal(*id).map(|(_, _, s)| at(&s.cwd)),
        Item::Worktree(_, path) | Item::Heading(_, path, _) | Item::File { worktree: path, .. } => {
            Some(at(path))
        }
        Item::Gathered(own) => place_of(tree, own),
        Item::Environment(_) => None,
        // Not the root itself: a bare repository, a submodule or a separate
        // git directory is named by its git directory.
        Item::Workspace(_, root)
        | Item::Across(_, root, _)
        | Item::GitHub(_, root, _)
        | Item::GitHubItem(_, root, ..)
        | Item::GitHubFailed(_, root, _) => {
            let ws = tree.workspaces.iter().find(|ws| &ws.root == root)?;
            let main = ws.worktrees.iter().find(|w| w.worktree.is_main());
            main.or(ws.worktrees.first()).map(|w| at(&w.worktree.path))
        }
    }
}

/// The sessions running in a worktree, for the tabs above the console.
pub fn sessions_in<'a>(tree: &'a Tree, place: &Place) -> Vec<&'a Session> {
    tree.worktrees()
        .filter(|w| w.worktree.path == place.worktree)
        .flat_map(|w| w.sessions.iter())
        .collect()
}
