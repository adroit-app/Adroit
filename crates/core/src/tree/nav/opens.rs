//! What opening or copying a row comes to.

use std::path::{Path, PathBuf};

use adroit_model::{TerminalSession, Tree};

use super::{Folds, Item, find, place_of, sessions_in};
use crate::content::Tab;
use crate::tree::Place;

/// What opening a row of the tree comes to, whichever key says where.
#[derive(Debug, Clone, PartialEq)]
pub enum Opens {
    /// The row folds or unfolds.
    Fold,
    /// A window shows this, of this worktree.
    Shows(Tab, Option<Place>),
    /// A Neovim to start on the file, in the worktree.
    Edit(PathBuf, PathBuf),
    /// A session not started yet, in the worktree.
    Draft(PathBuf),
    /// A file of a work machine that only a viewer of this computer would
    /// show, which cannot read it.
    Unavailable,
}

/// Whether `item` has rows under it to fold.
pub fn holds(item: &Item) -> bool {
    item.fold_key().is_some()
}

/// Whether opening `item` in place unfolds it instead of what [`opens`]
/// says: a folded row shows its rows first, as fern's `l` does, so a
/// worktree, which otherwise opens, can be unfolded again.
pub fn unfolds(item: &Item, folds: &Folds) -> bool {
    holds(item) && !folds.is_open(item)
}

/// What opening `item` shows: a file by its kind, in its viewer or in
/// Neovim, the one already started on it if there is one; a worktree its
/// first session, or else a new one. A Markdown document a Neovim is on opens
/// that Neovim, whose window switches to the viewer. A pull request or an
/// issue shows in a tab of its own, in its repository's checkout. A file of an environment
/// not `local`, which the window's viewers cannot read, opens in Neovim when
/// it is text and nowhere when it is not.
pub fn opens(item: &Item, tree: &Tree, terminals: &[TerminalSession], local: bool) -> Opens {
    match item {
        Item::Environment(_)
        | Item::Workspace(_, _)
        | Item::Across(..)
        | Item::Heading(..)
        | Item::GitHub(..)
        | Item::GitHubFailed(..)
        | Item::File { dir: true, .. } => Opens::Fold,
        Item::GitHubItem(environment, root, listing, number) => Opens::Shows(
            Tab::GitHub {
                environment: environment.clone(),
                repository: root.clone(),
                listing: *listing,
                number: *number,
            },
            place_of(tree, item),
        ),
        Item::File {
            environment,
            worktree,
            path,
            dir: false,
        } => {
            let at = Some(Place::at(environment, worktree.clone()));
            let editing = editing(terminals, path).next().map(|t| t.id);
            let kind = crate::viewers::FileKind::of(path);
            if let (Some(id), crate::viewers::FileKind::Markdown) = (editing, kind) {
                Opens::Shows(Tab::Terminal(environment.clone(), id), at)
            } else if kind.has_viewer() && local {
                Opens::Shows(Tab::Viewer(environment.clone(), path.clone()), at)
            } else if !crate::viewers::is_text(path) {
                Opens::Unavailable
            } else if let Some(id) = editing {
                Opens::Shows(Tab::Terminal(environment.clone(), id), at)
            } else {
                Opens::Edit(worktree.clone(), path.clone())
            }
        }
        Item::Waiting(environment, id) | Item::Session(environment, id) => Opens::Shows(
            Tab::Session(environment.clone(), id.clone()),
            place_of(tree, item),
        ),
        Item::Terminal(environment, id) => Opens::Shows(
            Tab::Terminal(environment.clone(), *id),
            place_of(tree, item),
        ),
        Item::Gathered(own) => opens(own, tree, terminals, local),
        Item::Worktree(environment, path) => {
            let p = Place::at(environment, path.clone());
            match sessions_in(tree, &p).first() {
                Some(s) => Opens::Shows(Tab::Session(environment.clone(), s.id.clone()), Some(p)),
                None => Opens::Draft(path.clone()),
            }
        }
    }
}

/// The Neovims of `terminals` started on the file at `path`.
fn editing<'a>(
    terminals: &'a [TerminalSession],
    path: &'a Path,
) -> impl Iterator<Item = &'a TerminalSession> {
    terminals
        .iter()
        .filter(move |t| t.file.as_deref() == Some(path))
}

/// What a window shows of `item`: a file in its viewer or in any Neovim
/// started on it.
pub fn stands_for(item: &Item, terminals: &[TerminalSession]) -> Vec<Tab> {
    match item {
        Item::Gathered(own) => stands_for(own, terminals),
        Item::Waiting(environment, id) | Item::Session(environment, id) => {
            vec![Tab::Session(environment.clone(), id.clone())]
        }
        Item::Terminal(environment, id) => vec![Tab::Terminal(environment.clone(), *id)],
        Item::File {
            environment,
            path,
            dir: false,
            ..
        } => std::iter::once(Tab::Viewer(environment.clone(), path.clone()))
            .chain(editing(terminals, path).map(|t| Tab::Terminal(environment.clone(), t.id)))
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether `item` is a file a Neovim started on it has open, whether or not
/// a window shows that Neovim.
pub fn edited(item: &Item, terminals: &[TerminalSession]) -> bool {
    match item {
        Item::File {
            path, dir: false, ..
        } => editing(terminals, path).next().is_some(),
        _ => false,
    }
}

/// What copying a row gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Copied {
    Path(PathBuf),
    Name(String),
}

/// What copying `item` gives: the path of a file, a worktree or a
/// workspace, the name of what runs.
pub fn copied(item: &Item, tree: &Tree, terminals: &[TerminalSession]) -> Option<Copied> {
    match item {
        Item::File { path, .. } | Item::Worktree(_, path) | Item::Workspace(_, path) => {
            Some(Copied::Path(path.clone()))
        }
        Item::Waiting(_, id) | Item::Session(_, id) => {
            find(tree, id).map(|(_, s)| Copied::Name(s.name.clone()))
        }
        Item::Terminal(_, id) => terminals
            .iter()
            .find(|t| t.id == *id)
            .map(|t| Copied::Name(t.name.clone())),
        Item::Gathered(own) => copied(own, tree, terminals),
        Item::Environment(_)
        | Item::Across(..)
        | Item::Heading(..)
        | Item::GitHub(..)
        | Item::GitHubItem(..)
        | Item::GitHubFailed(..) => None,
    }
}
