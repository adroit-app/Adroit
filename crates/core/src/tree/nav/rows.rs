//! Building the rows the tree shows, and where each sits among them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use adroit_model::files::Entry;
use adroit_model::github::{Listing, Repository};
use adroit_model::{
    EnvironmentId, InboxItem, TerminalProgram, TerminalSession, Tree, WorktreeNode,
};

use super::{Folds, Heading, Item};

/// Every item the tree of `environment` shows, top to bottom, the sessions
/// waiting on the user last. Children of a folded node are left out, `read`
/// lists a directory's entries once it is open, and `github` holds what is
/// open on GitHub of each repository on it, by its root.
pub fn items(
    environment: &EnvironmentId,
    tree: &Tree,
    inbox: &[InboxItem],
    folds: &Folds,
    read: &dyn Fn(&Path) -> Vec<Entry>,
    github: &BTreeMap<PathBuf, Repository>,
) -> Vec<Item> {
    let mut out = Vec::new();
    for ws in &tree.workspaces {
        let wi = Item::Workspace(environment.clone(), ws.root.clone());
        let ws_open = folds.is_open(&wi);
        out.push(wi);
        if !ws_open {
            continue;
        }
        if ws.flat() {
            for w in &ws.worktrees {
                out.extend(headed(environment, w, folds, read));
            }
            if ws.git {
                out.extend(on_github(environment, &ws.root, github, folds));
            }
            continue;
        }
        for heading in [Heading::Sessions, Heading::Terminals] {
            let hi = Item::Across(environment.clone(), ws.root.clone(), heading);
            let open = folds.is_open(&hi);
            out.push(hi);
            if open {
                out.extend(
                    ws.worktrees
                        .iter()
                        .flat_map(|w| under(environment, w, heading))
                        .map(|row| Item::Gathered(Box::new(row))),
                );
            }
        }
        out.extend(on_github(environment, &ws.root, github, folds));
        for w in &ws.worktrees {
            let ti = Item::Worktree(environment.clone(), w.worktree.path.clone());
            let t_open = folds.is_open(&ti);
            out.push(ti);
            if t_open {
                out.extend(headed(environment, w, folds, read));
            }
        }
    }
    out.extend(
        inbox
            .iter()
            .map(|i| Item::Waiting(environment.clone(), i.session.id.clone())),
    );
    out
}

/// The headings of what is open on GitHub of the repository at `root`, each
/// followed by its rows while it is open: what was last read, or why it
/// could not be. None for a repository not on GitHub.
fn on_github(
    environment: &EnvironmentId,
    root: &Path,
    github: &BTreeMap<PathBuf, Repository>,
    folds: &Folds,
) -> Vec<Item> {
    let Some(repository) = github.get(root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for listing in Listing::ALL {
        let heading = Item::GitHub(environment.clone(), root.to_path_buf(), listing);
        let open = folds.is_open(&heading);
        out.push(heading);
        if !open {
            continue;
        }
        let root = root.to_path_buf();
        match repository.read(listing) {
            None => {}
            Some(Ok(items)) => {
                out.extend(items.iter().map(|i| {
                    Item::GitHubItem(environment.clone(), root.clone(), listing, i.number)
                }))
            }
            Some(Err(_)) => out.push(Item::GitHubFailed(environment.clone(), root, listing)),
        }
    }
    out
}

/// A worktree's headings, each followed by its rows while it is open.
fn headed(
    environment: &EnvironmentId,
    w: &WorktreeNode,
    folds: &Folds,
    read: &dyn Fn(&Path) -> Vec<Entry>,
) -> Vec<Item> {
    let mut out = Vec::new();
    for heading in Heading::ALL {
        let hi = Item::Heading(environment.clone(), w.worktree.path.clone(), heading);
        let open = folds.is_open(&hi);
        out.push(hi);
        if !open {
            continue;
        }
        match heading {
            Heading::Files => {
                let root = &w.worktree.path;
                files_in(environment, root, root, folds, read, &mut out);
            }
            _ => out.extend(under(environment, w, heading)),
        }
    }
    out
}

/// The entries of `dir` in `worktree`, each open directory followed by its
/// own.
fn files_in(
    environment: &EnvironmentId,
    worktree: &Path,
    dir: &Path,
    folds: &Folds,
    read: &dyn Fn(&Path) -> Vec<Entry>,
    out: &mut Vec<Item>,
) {
    for entry in read(dir) {
        let item = Item::File {
            environment: environment.clone(),
            worktree: worktree.to_path_buf(),
            path: entry.path.clone(),
            dir: entry.dir,
        };
        let open = folds.is_open(&item);
        out.push(item);
        if open {
            files_in(environment, worktree, &entry.path, folds, read, out);
        }
    }
}

/// The heading `terminal` is listed under: every program run in a terminal
/// beside the shells, save a Neovim started on a file, which the file's own
/// row stands for.
pub(super) fn listed_under(terminal: &TerminalSession) -> Option<Heading> {
    (terminal.program != TerminalProgram::Neovim || terminal.file.is_none())
        .then_some(Heading::Terminals)
}

/// What runs in a worktree of `environment`, under one of its headings;
/// Files holds no such rows.
pub fn under(environment: &EnvironmentId, w: &WorktreeNode, heading: Heading) -> Vec<Item> {
    match heading {
        Heading::Sessions => w
            .sessions
            .iter()
            .map(|s| Item::Session(environment.clone(), s.id.clone()))
            .collect(),
        Heading::Terminals => w
            .terminals
            .iter()
            .filter(|t| listed_under(t) == Some(heading))
            .map(|t| Item::Terminal(environment.clone(), t.id))
            .collect(),
        Heading::Files => Vec::new(),
    }
}

/// How deep `item` sits in `tree`, counted from its workspace's row: a
/// flat workspace holds its checkout's headings straight under it.
pub fn depth(tree: &Tree, item: &Item) -> usize {
    let heading_at = |worktree: &Path| {
        let flat = tree.workspace_of(worktree).is_none_or(|ws| ws.flat());
        if flat { 1 } else { 2 }
    };
    match item {
        Item::Environment(_) | Item::Workspace(_, _) | Item::Waiting(_, _) => 0,
        Item::Worktree(_, _) | Item::Across(..) | Item::GitHub(..) => 1,
        Item::GitHubItem(..) | Item::GitHubFailed(..) => 2,
        Item::Gathered(_) => 2,
        Item::Heading(_, path, _) => heading_at(path),
        Item::Session(_, id) => tree
            .find(id)
            .map_or(0, |(_, w, _)| heading_at(&w.worktree.path) + 1),
        Item::Terminal(_, id) => tree
            .terminal(*id)
            .map_or(0, |(_, w, _)| heading_at(&w.worktree.path) + 1),
        Item::File { worktree, path, .. } => {
            let below = path
                .strip_prefix(worktree)
                .map_or(1, |p| p.components().count());
            heading_at(worktree) + below
        }
    }
}

/// The row `item` sits under in `list`: the nearest above it one level up,
/// each row as deep as `depth` says. The sessions waiting on the user sit
/// under none.
pub fn parent(list: &[Item], depth: impl Fn(&Item) -> usize, item: &Item) -> Option<Item> {
    let at = list.iter().position(|i| i == item)?;
    let level = depth(item);
    if level == 0 {
        return None;
    }
    list[..at]
        .iter()
        .rev()
        .find(|i| !matches!(i, Item::Waiting(_, _)) && depth(i) < level)
        .cloned()
}

/// The row above `item` in `list`, which stands where it stood once it is
/// gone: a shell's is always of its own worktree. `None` for the first row
/// and for one not listed.
pub fn row_above(list: &[Item], item: &Item) -> Option<Item> {
    let at = list.iter().position(|i| i == item)?;
    at.checked_sub(1).map(|i| list[i].clone())
}
