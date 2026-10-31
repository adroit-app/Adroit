//! The tree as one column of items: the workspaces, each worktree's headings
//! and what they hold, then the sessions waiting on the user.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use adroit_model::{
    EnvironmentId, InboxItem, Session, TerminalProgram, TerminalSession, Tree, WorktreeNode,
};

use crate::content::Tab;
use adroit_model::files::Entry;

/// A row of the tree, by the environment it belongs to and its own identity
/// there.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    /// An environment, heading its work while the window shows more than
    /// one.
    Environment(EnvironmentId),
    /// A session waiting on the user, listed in the inbox.
    Waiting(EnvironmentId, String),
    /// A workspace, by the repository or directory it is.
    Workspace(EnvironmentId, PathBuf),
    Worktree(EnvironmentId, PathBuf),
    /// A heading of a worktree, holding the rows of one kind.
    Heading(EnvironmentId, PathBuf, Heading),
    Session(EnvironmentId, String),
    /// A shell or a Neovim adroit runs, by the daemon's id.
    Terminal(EnvironmentId, u128),
    /// A file or a directory of a worktree, under its Files heading.
    File {
        environment: EnvironmentId,
        worktree: PathBuf,
        path: PathBuf,
        dir: bool,
    },
}

/// What a worktree's heading holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Heading {
    Files,
    Sessions,
    Terminals,
}

impl Heading {
    pub const ALL: [Heading; 3] = [Heading::Files, Heading::Sessions, Heading::Terminals];

    pub fn name(self) -> &'static str {
        match self {
            Heading::Files => "Files",
            Heading::Sessions => "Sessions",
            Heading::Terminals => "Terminals",
        }
    }
}

impl Item {
    pub fn environment(&self) -> &EnvironmentId {
        match self {
            Item::Environment(environment)
            | Item::Waiting(environment, _)
            | Item::Workspace(environment, _)
            | Item::Worktree(environment, _)
            | Item::Heading(environment, ..)
            | Item::Session(environment, _)
            | Item::Terminal(environment, _)
            | Item::File { environment, .. } => environment,
        }
    }

    /// The key used to remember whether a node is folded.
    pub fn fold_key(&self) -> Option<String> {
        let key = match self {
            Item::Environment(_) => "environment".into(),
            Item::Workspace(_, root) => format!("ws:{}", root.display()),
            Item::Worktree(_, path) => format!("wt:{}", path.display()),
            Item::Heading(_, path, heading) => format!("{heading:?}:{}", path.display()),
            Item::File {
                worktree,
                path,
                dir: true,
                ..
            } => format!("dir:{}:{}", worktree.display(), path.display()),
            _ => return None,
        };
        Some(format!("{}@{key}", self.environment()))
    }

    /// Whether the node shows the rows under it before it is unfolded: a
    /// worktree's files are read only once asked for.
    fn starts_open(&self) -> bool {
        !matches!(
            self,
            Item::Heading(_, _, Heading::Files) | Item::File { .. }
        )
    }
}

/// Which nodes of the tree are folded: those the user folded or unfolded,
/// each against how it starts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folds(HashSet<String>);

impl Folds {
    /// Whether `item` shows the rows under it. A row that does not fold has
    /// none to show.
    pub fn is_open(&self, item: &Item) -> bool {
        item.fold_key()
            .is_some_and(|k| item.starts_open() != self.0.contains(&k))
    }

    pub fn set(&mut self, item: &Item, open: bool) {
        let Some(key) = item.fold_key() else {
            return;
        };
        if open == item.starts_open() {
            self.0.remove(&key);
        } else {
            self.0.insert(key);
        }
    }

    pub fn toggle(&mut self, item: &Item) {
        let open = self.is_open(item);
        self.set(item, !open);
    }
}

/// Every item the tree of `environment` shows, top to bottom, the sessions
/// waiting on the user last. Children of a folded node are left out, and
/// `read` lists a directory's entries once it is open.
pub fn items(
    environment: &EnvironmentId,
    tree: &Tree,
    inbox: &[InboxItem],
    folds: &Folds,
    read: &dyn Fn(&Path) -> Vec<Entry>,
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
            continue;
        }
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
fn listed_under(terminal: &TerminalSession) -> Option<Heading> {
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
        Item::Worktree(_, _) => 1,
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

/// The item `delta` steps from `current`, clamped to the ends. With nothing
/// selected the first item is chosen.
pub fn step(items: &[Item], current: Option<&Item>, delta: isize) -> Option<Item> {
    if items.is_empty() {
        return None;
    }
    let at = current.and_then(|c| items.iter().position(|i| i == c));
    let next = match at {
        Some(i) => (i as isize + delta).clamp(0, items.len() as isize - 1) as usize,
        None => 0,
    };
    Some(items[next].clone())
}

/// The index `delta` places from `at` in a list of `len`, wrapping around.
pub fn wrap(at: usize, delta: isize, len: usize) -> usize {
    (at as isize + delta).rem_euclid(len as isize) as usize
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
        (Some(i), _) => wrap(i, if forward { 1 } else { -1 }, n),
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
        Some(i) if forward => i + 1,
        Some(i) => i + n - 1,
        None => {
            let worktree =
                place.and_then(|p| tree.worktrees().position(|w| w.worktree.path == p.worktree));
            let after = worktree.map_or(0, |wt| all.iter().filter(|(w, _)| *w <= wt).count());
            if forward { after } else { after + n - 1 }
        }
    };
    Some(all[i % n].1.id.clone())
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

/// Where the middle of the window is: the worktree, or the directory outside
/// git, its tabs are of, in the environment it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub environment: EnvironmentId,
    pub worktree: PathBuf,
}

impl Place {
    pub fn at(environment: &EnvironmentId, worktree: impl Into<PathBuf>) -> Place {
        Place {
            environment: environment.clone(),
            worktree: worktree.into(),
        }
    }
}

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

/// The row to select once `selected` is no longer in `list`: the same
/// session's other row, as when it stops waiting, or else the row of the tab
/// `shown` in `place`. `None` while `selected` is still listed.
pub fn row_kept(
    list: &[Item],
    tree: &Tree,
    selected: Option<&Item>,
    shown: Option<&Tab>,
    place: &Place,
) -> Option<Item> {
    let selected = selected?;
    if list.contains(selected) {
        return None;
    }
    let same = match selected {
        Item::Waiting(environment, id) => Some(Item::Session(environment.clone(), id.clone())),
        Item::Session(environment, id) => Some(Item::Waiting(environment.clone(), id.clone())),
        _ => None,
    };
    if let Some(row) = same.filter(|row| list.contains(row)) {
        return Some(row);
    }
    row_of(list, tree, selected.environment(), shown, place)
}

/// `tree` with only the sessions and terminals whose names hold `text`, any
/// case, and the worktrees and workspaces that still hold one; a workspace or a worktree
/// whose own name holds it keeps everything beneath it. An empty `text` keeps
/// everything.
pub fn narrowed(tree: &Tree, text: &str) -> Tree {
    if text.is_empty() {
        return tree.clone();
    }
    let want = text.to_lowercase();
    let named = |name: &str| name.to_lowercase().contains(&want);
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

/// What the console shows once a tab of `place` in `environment` has
/// closed: its first session, or else the first of `terminals`, the
/// terminals left in the worktree.
pub fn tab_after_closing(
    environment: &EnvironmentId,
    tree: &Tree,
    place: &Place,
    terminals: &[u128],
) -> Option<Tab> {
    sessions_in(tree, place)
        .first()
        .map(|s| Tab::Session(environment.clone(), s.id.clone()))
        .or_else(|| {
            terminals
                .first()
                .map(|&id| Tab::Terminal(environment.clone(), id))
        })
}

/// The row in `list` for what a tab shows: the session's or the shell's own
/// row, the file's a viewer or a Neovim started on it shows, or else the
/// nearest row above it still shown, its heading, worktree or workspace.
/// `None` when none is listed. The rows are those of `environment`, the tab's
/// and the place's.
pub fn row_of(
    list: &[Item],
    tree: &Tree,
    environment: &EnvironmentId,
    shown: Option<&Tab>,
    place: &Place,
) -> Option<Item> {
    let file = match shown {
        Some(Tab::Viewer(_, path)) => Some(path.clone()),
        Some(Tab::Terminal(_, id)) => tree.terminal(*id).and_then(|(_, _, t)| t.file.clone()),
        _ => None,
    };
    let mut rows: Vec<Item> = match (shown, file) {
        (_, Some(path)) => vec![
            Item::File {
                environment: environment.clone(),
                worktree: place.worktree.clone(),
                path,
                dir: false,
            },
            Item::Heading(environment.clone(), place.worktree.clone(), Heading::Files),
        ],
        (Some(Tab::Session(environment, id)), None) => {
            vec![Item::Session(environment.clone(), id.clone())]
        }
        (Some(Tab::Terminal(environment, id)), None) => {
            vec![Item::Terminal(environment.clone(), *id)]
        }
        _ => Vec::new(),
    };
    rows.extend(heading_of(tree, shown));
    rows.push(Item::Worktree(environment.clone(), place.worktree.clone()));
    if let Some(ws) = tree.workspace_of(&place.worktree) {
        rows.push(Item::Workspace(environment.clone(), ws.root.clone()));
    }
    rows.into_iter().find(|item| list.contains(item))
}

/// The heading the row of what `shown` shows sits under.
fn heading_of(tree: &Tree, shown: Option<&Tab>) -> Option<Item> {
    match shown? {
        Tab::Session(environment, id) => {
            let (_, w, _) = tree.find(id)?;
            Some(Item::Heading(
                environment.clone(),
                w.worktree.path.clone(),
                Heading::Sessions,
            ))
        }
        Tab::Terminal(environment, id) => {
            let (_, w, t) = tree.terminal(*id)?;
            Some(Item::Heading(
                environment.clone(),
                w.worktree.path.clone(),
                listed_under(t)?,
            ))
        }
        _ => None,
    }
}

/// The row to select once the console shows `now` instead of `before`, both in
/// `place`: the new tab's, when the selection was on the old tab's row, on a
/// row no longer listed, or on nothing. `None` leaves the selection where it was moved by hand.
pub fn row_after_switch(
    list: &[Item],
    tree: &Tree,
    selected: Option<&Item>,
    before: Option<&Tab>,
    now: &Tab,
    place: &Place,
) -> Option<Item> {
    let on_old = match (selected, before) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(item), Some(tab)) => {
            !list.contains(item)
                || matches!(
                    (item, tab),
                    (Item::Waiting(e, w), Tab::Session(f, o)) if (e, w) == (f, o)
                )
                || Some(item) == row_of(list, tree, tab.environment(), Some(tab), place).as_ref()
        }
    };
    on_old
        .then(|| row_of(list, tree, now.environment(), Some(now), place))
        .flatten()
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
        Item::Environment(_) => None,
        // Not the root itself: a bare repository, a submodule or a separate
        // git directory is named by its git directory.
        Item::Workspace(_, root) => {
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
/// first session, or else a new one.
pub fn opens(item: &Item, tree: &Tree, terminals: &[TerminalSession]) -> Opens {
    match item {
        Item::Environment(_)
        | Item::Workspace(_, _)
        | Item::Heading(..)
        | Item::File { dir: true, .. } => Opens::Fold,
        Item::File {
            environment,
            worktree,
            path,
            dir: false,
        } => {
            let at = Some(Place::at(environment, worktree.clone()));
            let editing = editing(terminals, path).next().map(|t| t.id);
            if crate::viewers::FileKind::of(path).has_viewer() {
                Opens::Shows(Tab::Viewer(environment.clone(), path.clone()), at)
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
        Item::Environment(_) | Item::Heading(..) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::here;
    use adroit_model::testing::{self, TreeBuilder};
    use adroit_model::{TerminalSession, Wait, inbox};

    #[test]
    fn the_same_path_in_two_environments_is_two_rows_folded_apart() {
        let here = Item::Worktree(EnvironmentId::new("here"), "/w".into());
        let away = Item::Worktree(EnvironmentId::new("away"), "/w".into());
        assert_ne!(here, away);
        let mut folds = Folds::default();
        folds.set(&here, false);
        assert!(!folds.is_open(&here));
        assert!(folds.is_open(&away));
    }

    #[test]
    fn the_same_session_in_two_environments_is_two_rows() {
        let here = EnvironmentId::new("here");
        let away = EnvironmentId::new("away");
        assert_ne!(
            Item::Session(here.clone(), "s".into()),
            Item::Session(away.clone(), "s".into())
        );
        assert_ne!(Item::Terminal(here, 1), Item::Terminal(away, 1));
    }

    const OAUTH: &str = "/api/.claude/worktrees/oauth";

    /// `api` is a repository with sessions in its main checkout and in a
    /// worktree; `web` one with sessions only in its main checkout; `tmp` is
    /// outside git.
    fn tree() -> Tree {
        repositories()
            .found("/tmp", None)
            .session(testing::session("a").cwd("/api").working())
            .session(testing::session("b").cwd("/api").waiting(Wait::Permission))
            .session(testing::session("o").cwd(OAUTH))
            .session(testing::session("c").cwd("/web"))
            .session(testing::session("z").cwd("/tmp").waiting(Wait::Input))
            .build()
    }

    /// The main checkouts of `api` and `web`, and `api`'s `oauth` worktree.
    fn repositories() -> TreeBuilder {
        testing::tree()
            .checkout("/api", "/api", "main")
            .checkout(OAUTH, "/api", "oauth")
            .checkout("/web", "/web", "main")
    }

    fn ws(root: &str) -> Item {
        Item::Workspace(here(), root.into())
    }

    fn worktree(path: &str) -> Item {
        Item::Worktree(here(), path.into())
    }

    fn session(id: &str) -> Item {
        Item::Session(here(), id.into())
    }

    fn sessions_of(path: &str) -> Item {
        Item::Heading(here(), path.into(), Heading::Sessions)
    }

    fn shells_of(path: &str) -> Item {
        Item::Heading(here(), path.into(), Heading::Terminals)
    }

    fn files_of(path: &str) -> Item {
        Item::Heading(here(), path.into(), Heading::Files)
    }

    /// A worktree's headings with the rows under each, its files folded.
    fn group(path: &str, sessions: &[Item], shells: &[Item]) -> Vec<Item> {
        [
            &[files_of(path), sessions_of(path)][..],
            sessions,
            &[shells_of(path)],
            shells,
        ]
        .concat()
    }

    /// The rows of `tree` with no file anywhere.
    fn rows(tree: &Tree, inbox: &[InboxItem], folds: &Folds) -> Vec<Item> {
        items(&here(), tree, inbox, folds, &|_| Vec::new())
    }

    fn file(worktree: &str, path: &str, dir: bool) -> Item {
        Item::File {
            environment: here(),
            worktree: worktree.into(),
            path: path.into(),
            dir,
        }
    }

    /// A worktree at `/web` holding `src/` with `main.rs` in it, and
    /// `README.md`.
    fn read_web(dir: &Path) -> Vec<Entry> {
        let entry = |path: &str, dir: bool| Entry {
            path: path.into(),
            dir,
        };
        match dir.to_str() {
            Some("/web") => vec![entry("/web/src", true), entry("/web/README.md", false)],
            Some("/web/src") => vec![entry("/web/src/main.rs", false)],
            _ => Vec::new(),
        }
    }

    #[test]
    fn a_worktree_s_files_are_listed_once_its_files_heading_is_unfolded() {
        let t = tree();
        let folded = items(&here(), &t, &[], &Folds::default(), &read_web);
        assert!(!folded.iter().any(|i| matches!(i, Item::File { .. })));
        let mut folds = Folds::default();
        folds.toggle(&files_of("/web"));
        let open = items(&here(), &t, &[], &folds, &read_web);
        let at = open.iter().position(|i| *i == files_of("/web")).unwrap();
        assert_eq!(
            open[at + 1..at + 3],
            [
                file("/web", "/web/src", true),
                file("/web", "/web/README.md", false)
            ]
        );
        assert_eq!(open[at + 3], sessions_of("/web"));
    }

    #[test]
    fn an_unfolded_directory_lists_its_entries_beneath_it_one_level_deeper() {
        let t = tree();
        let mut folds = Folds::default();
        folds.toggle(&files_of("/web"));
        folds.toggle(&file("/web", "/web/src", true));
        let open = items(&here(), &t, &[], &folds, &read_web);
        let at = open
            .iter()
            .position(|i| *i == file("/web", "/web/src", true))
            .unwrap();
        assert_eq!(open[at + 1], file("/web", "/web/src/main.rs", false));
        assert_eq!(depth(&t, &files_of("/web")), 1);
        assert_eq!(depth(&t, &file("/web", "/web/src", true)), 2);
        assert_eq!(depth(&t, &file("/web", "/web/src/main.rs", false)), 3);
    }

    #[test]
    fn a_row_s_parent_is_the_nearest_row_above_it_one_level_up() {
        let t = tree();
        let mut folds = Folds::default();
        folds.toggle(&files_of("/web"));
        folds.toggle(&file("/web", "/web/src", true));
        let list = items(&here(), &t, &[], &folds, &read_web);
        let main = file("/web", "/web/src/main.rs", false);
        assert_eq!(
            parent(&list, |i| depth(&t, i), &main),
            Some(file("/web", "/web/src", true))
        );
        let readme = file("/web", "/web/README.md", false);
        assert_eq!(
            parent(&list, |i| depth(&t, i), &readme),
            Some(files_of("/web"))
        );
        assert_eq!(
            parent(&list, |i| depth(&t, i), &files_of("/web")),
            Some(ws("/web"))
        );
        assert_eq!(parent(&list, |i| depth(&t, i), &ws("/web")), None);
    }

    fn closed(rows: &[Item]) -> Folds {
        let mut folds = Folds::default();
        for row in rows {
            folds.set(row, false);
        }
        folds
    }

    fn at(path: &str) -> Place {
        Place::at(&here(), path)
    }

    #[test]
    fn each_workspace_lists_its_worktrees_headings_and_rows_then_the_inbox_comes_last() {
        let t = tree();
        let got = rows(&t, &inbox(&t), &Folds::default());
        let expected = [
            vec![ws("/api"), worktree("/api")],
            group("/api", &[session("a"), session("b")], &[]),
            vec![worktree(OAUTH)],
            group(OAUTH, &[session("o")], &[]),
            vec![ws("/tmp")],
            group("/tmp", &[session("z")], &[]),
            vec![ws("/web")],
            group("/web", &[session("c")], &[]),
            vec![
                Item::Waiting(here(), "b".into()),
                Item::Waiting(here(), "z".into()),
            ],
        ]
        .concat();
        assert_eq!(got, expected);
    }

    /// `a` runs in the main checkout of `api` and opened `o`, which runs in
    /// the `oauth` worktree, and `w`, which runs in `web`.
    fn opening_tree(opener_listed: bool) -> Tree {
        let opener = adroit_model::SessionId::new();
        let opened = |id: &str, cwd: &str| {
            testing::session(id)
                .cwd(cwd)
                .working()
                .delegation(adroit_model::SessionId::new())
                .parent(opener)
        };
        let t = repositories()
            .session(opened("o", OAUTH))
            .session(opened("w", "/web"));
        let a = testing::session("a")
            .cwd("/api")
            .working()
            .delegation(opener);
        if opener_listed { t.session(a) } else { t }.build()
    }

    #[test]
    fn an_opened_session_is_listed_where_it_runs_whether_the_one_that_opened_it_is_listed_or_not() {
        let elsewhere = [
            vec![worktree(OAUTH)],
            group(OAUTH, &[session("o")], &[]),
            vec![ws("/web")],
            group("/web", &[session("w")], &[]),
        ]
        .concat();
        assert_eq!(
            rows(&opening_tree(true), &[], &Folds::default()),
            [
                vec![ws("/api"), worktree("/api")],
                group("/api", &[session("a")], &[]),
                elsewhere.clone(),
            ]
            .concat()
        );
        assert_eq!(
            rows(&opening_tree(false), &[], &Folds::default()),
            [vec![ws("/api")], elsewhere].concat()
        );
    }

    #[test]
    fn after_a_tab_closes_the_worktree_s_first_session_is_shown() {
        assert_eq!(
            tab_after_closing(&here(), &tree(), &at("/api"), &[7]),
            Some(Tab::Session(here(), "a".into()))
        );
    }

    #[test]
    fn a_worktree_without_sessions_shows_another_shell_or_nothing() {
        assert_eq!(
            tab_after_closing(&here(), &tree(), &at("/nowhere"), &[7]),
            Some(Tab::Terminal(here(), 7))
        );
        assert_eq!(
            tab_after_closing(&here(), &tree(), &at("/nowhere"), &[]),
            None
        );
    }

    #[test]
    fn the_selection_moves_to_the_new_tab_s_row_only_from_the_old_one_s() {
        let t = tree();
        let list = rows(&t, &inbox(&t), &Folds::default());
        let shell = Tab::Terminal(here(), 1);
        let a = Tab::Session(here(), "a".into());
        let row_a = Some(session("a"));
        let switch = |selected: Option<Item>, before: Option<&Tab>| {
            row_after_switch(&list, &t, selected.as_ref(), before, &a, &at("/api"))
        };
        // On the shell's row, its worktree's, it moves to the session's.
        assert_eq!(switch(Some(worktree("/api")), Some(&shell)), row_a);
        // So it does from nothing, or from the old session's inbox row.
        assert_eq!(switch(None, None), row_a);
        let b = Tab::Session(here(), "b".into());
        assert_eq!(
            switch(Some(Item::Waiting(here(), "b".into())), Some(&b)),
            row_a
        );
        // Moved elsewhere by hand, it stays.
        assert_eq!(switch(Some(ws("/web")), Some(&shell)), None);
        // On a row no longer listed, it moves too.
        let gone = Tab::Session(here(), "gone".into());
        assert_eq!(switch(Some(session("gone")), Some(&gone)), row_a);
    }

    #[test]
    fn a_selected_row_that_leaves_gives_way_to_the_same_session_s_or_the_shown_tab_s() {
        let t = tree();
        let list = rows(&t, &inbox(&t), &Folds::default());
        let shown = Tab::Session(here(), "a".into());
        let kept = |row: Item| row_kept(&list, &t, Some(&row), Some(&shown), &at("/api"));
        // "a" no longer waits: its inbox row leaves for its own.
        assert_eq!(kept(Item::Waiting(here(), "a".into())), Some(session("a")));
        // A row still listed stays.
        assert_eq!(kept(ws("/web")), None);
        // A session gone gives way to the shown tab's row.
        assert_eq!(kept(session("gone")), Some(session("a")));
    }

    #[test]
    fn a_filter_keeps_the_sessions_whose_names_match_and_the_rows_above_them() {
        let t = tree();
        let list = rows(&narrowed(&t, "O"), &[], &Folds::default());
        // The worktree oauth is named with an o, and keeps all it holds.
        assert_eq!(
            list,
            [
                vec![ws("/api"), worktree(OAUTH)],
                group(OAUTH, &[session("o")], &[])
            ]
            .concat()
        );
    }

    #[test]
    fn an_empty_filter_keeps_everything_and_one_matching_nothing_keeps_nothing() {
        let t = tree();
        assert_eq!(narrowed(&t, ""), t);
        assert_eq!(rows(&narrowed(&t, "nothing"), &[], &Folds::default()), []);
    }

    #[test]
    fn a_filter_naming_a_workspace_keeps_all_beneath_it() {
        let t = tree();
        let web = rows(&narrowed(&t, "web"), &[], &Folds::default());
        assert_eq!(
            web,
            [vec![ws("/web")], group("/web", &[session("c")], &[])].concat()
        );
    }

    #[test]
    fn a_session_is_gone_once_it_is_not_listed() {
        let t = tree();
        let sessions: Vec<Session> = t.sessions().map(|(_, s)| s.clone()).collect();
        assert!(!is_gone(&sessions, "a"));
        assert!(is_gone(&sessions, "nowhere"));
    }

    #[test]
    fn a_session_tab_selects_the_session_s_row() {
        let t = tree();
        let list = rows(&t, &inbox(&t), &Folds::default());
        assert_eq!(
            row_of(
                &list,
                &t,
                &here(),
                Some(&Tab::Session(here(), "b".into())),
                &at("/api")
            ),
            Some(session("b"))
        );
    }

    #[test]
    fn a_viewer_selects_its_file_s_row_or_while_folded_away_the_files_heading() {
        let t = tree();
        let viewer = Tab::Viewer(here(), "/web/README.md".into());
        let folded = items(&here(), &t, &[], &Folds::default(), &read_web);
        assert_eq!(
            row_of(&folded, &t, &here(), Some(&viewer), &at("/web")),
            Some(files_of("/web"))
        );
        let mut folds = Folds::default();
        folds.toggle(&files_of("/web"));
        let open = items(&here(), &t, &[], &folds, &read_web);
        assert_eq!(
            row_of(&open, &t, &here(), Some(&viewer), &at("/web")),
            Some(file("/web", "/web/README.md", false))
        );
    }

    #[test]
    fn a_tab_without_a_session_selects_its_worktree() {
        let t = tree();
        let list = rows(&t, &[], &Folds::default());
        assert_eq!(
            row_of(&list, &t, &here(), None, &at("/api")),
            Some(worktree("/api"))
        );
    }

    #[test]
    fn a_session_folded_away_selects_the_nearest_row_still_shown() {
        let t = tree();
        for row in [sessions_of("/api"), worktree("/api"), ws("/api")] {
            let list = rows(&t, &[], &closed(std::slice::from_ref(&row)));
            assert_eq!(
                row_of(
                    &list,
                    &t,
                    &here(),
                    Some(&Tab::Session(here(), "a".into())),
                    &at("/api")
                ),
                Some(row)
            );
        }
    }

    #[test]
    fn a_session_of_a_flat_workspace_folded_away_selects_the_workspace() {
        let t = tree();
        let collapsed = closed(&[ws("/tmp")]);
        let list = rows(&t, &[], &collapsed);
        assert_eq!(
            row_of(
                &list,
                &t,
                &here(),
                Some(&Tab::Session(here(), "z".into())),
                &at("/tmp")
            ),
            Some(ws("/tmp"))
        );
    }

    #[test]
    fn a_collapsed_node_hides_its_children() {
        let t = tree();
        let collapsed = closed(&[worktree("/api")]);
        let got = rows(&t, &[], &collapsed);
        assert!(got.contains(&worktree("/api")));
        assert!(!got.contains(&session("a")));
        assert!(got.contains(&session("o")));
    }

    #[test]
    fn stepping_clamps_at_both_ends() {
        let list = vec![ws("/w"), session("x")];
        assert_eq!(step(&list, None, 1), Some(ws("/w")));
        assert_eq!(step(&list, Some(&ws("/w")), 1), Some(session("x")));
        assert_eq!(step(&list, Some(&session("x")), 1), Some(session("x")));
        assert_eq!(step(&list, Some(&ws("/w")), -1), Some(ws("/w")));
        assert_eq!(step(&[], None, 1), None);
    }

    #[test]
    fn wrapping_goes_round_both_ends() {
        assert_eq!(wrap(2, 1, 3), 0);
        assert_eq!(wrap(0, -1, 3), 2);
        assert_eq!(wrap(1, 4, 3), 2);
    }

    #[test]
    fn the_next_waiting_session_wraps_around() {
        let t = tree();
        let ib: Vec<_> = inbox(&t).into_iter().map(|i| (here(), i)).collect();
        let next = |current: Option<&str>, forward| {
            next_waiting(&ib, current.map(|id| (&ib[0].0, id)), forward).map(|(_, id)| id)
        };
        assert_eq!(next(None, true).as_deref(), Some("b"));
        assert_eq!(next(Some("b"), true).as_deref(), Some("z"));
        assert_eq!(next(Some("z"), true).as_deref(), Some("b"));
        assert_eq!(next(Some("b"), false).as_deref(), Some("z"));
    }

    #[test]
    fn the_same_session_id_waiting_in_two_environments_is_two_stops() {
        let t = tree();
        let there = EnvironmentId::new("there");
        let ib: Vec<_> = [here(), there.clone()]
            .into_iter()
            .flat_map(|e| inbox(&t).into_iter().map(move |i| (e.clone(), i)))
            .collect();
        assert_eq!(
            next_waiting(&ib, Some((&here(), "z")), true),
            Some((there.clone(), "b".into()))
        );
        assert_eq!(
            next_waiting(&ib, Some((&there, "b")), false),
            Some((here(), "z".into()))
        );
    }

    #[test]
    fn the_next_session_goes_down_the_tree_and_wraps_around() {
        let t = tree();
        let shown = |id: &str| Some(Tab::Session(here(), id.into()));
        let next = |id: &str, forward| next_session(&t, shown(id).as_ref(), None, forward);
        for (from, forward, to) in [
            ("a", true, "b"),
            ("b", true, "o"),
            ("o", true, "z"),
            ("z", true, "c"),
            ("c", true, "a"),
            ("a", false, "c"),
            ("o", false, "b"),
        ] {
            assert_eq!(next(from, forward).as_deref(), Some(to), "{from} {forward}");
        }
    }

    #[test]
    fn a_tab_without_a_session_stands_after_the_sessions_of_its_worktree() {
        let t = tree();
        let shell = Some(Tab::Terminal(here(), 1));
        let next = |path: &str, forward| next_session(&t, shell.as_ref(), Some(&at(path)), forward);
        assert_eq!(next("/api", true).as_deref(), Some("o"));
        assert_eq!(next("/api", false).as_deref(), Some("b"));
        assert_eq!(next(OAUTH, true).as_deref(), Some("z"));
        assert_eq!(next("/web", true).as_deref(), Some("a"));
        // A session gone stands where its tab is, as a shell's does.
        let gone = Some(Tab::Session(here(), "gone".into()));
        let after = next_session(&t, gone.as_ref(), Some(&at("/api")), true);
        assert_eq!(after.as_deref(), Some("o"));
    }

    #[test]
    fn with_nothing_shown_the_next_session_is_the_first_and_the_previous_the_last() {
        let t = tree();
        assert_eq!(next_session(&t, None, None, true).as_deref(), Some("a"));
        assert_eq!(next_session(&t, None, None, false).as_deref(), Some("c"));
        // So it is from a place no session runs in.
        let nowhere = Some(at("/nowhere"));
        let draft = next_session(&t, None, nowhere.as_ref(), true);
        assert_eq!(draft.as_deref(), Some("a"));
        assert_eq!(next_session(&Tree::default(), None, None, true), None);
    }

    #[test]
    fn the_next_workspace_opens_its_first_session_and_wraps_around() {
        let t = tree();
        let shown = |id: &str| Some(Tab::Session(here(), id.into()));
        let none = HashMap::new();
        let next = |id: &str, forward| next_workspace(&t, shown(id).as_ref(), None, &none, forward);
        for (from, forward, to) in [
            ("a", true, "z"),
            ("o", true, "z"),
            ("z", true, "c"),
            ("c", true, "a"),
            ("a", false, "c"),
            ("o", false, "c"),
            ("c", false, "z"),
        ] {
            assert_eq!(next(from, forward).as_deref(), Some(to), "{from} {forward}");
        }
    }

    #[test]
    fn the_next_workspace_opens_the_session_last_shown_there() {
        let t = tree();
        let shown = Some(Tab::Session(here(), "c".into()));
        let last: HashMap<PathBuf, String> = [(PathBuf::from("/api"), "o".to_string())].into();
        let next = next_workspace(&t, shown.as_ref(), None, &last, true);
        assert_eq!(next.as_deref(), Some("o"));
        // A session last shown that has gone gives way to the first.
        let last: HashMap<PathBuf, String> = [(PathBuf::from("/api"), "gone".to_string())].into();
        let next = next_workspace(&t, shown.as_ref(), None, &last, true);
        assert_eq!(next.as_deref(), Some("a"));
    }

    #[test]
    fn a_tab_without_a_session_goes_from_the_workspace_of_its_worktree() {
        let t = tree();
        let shell = Some(Tab::Terminal(here(), 1));
        let none = HashMap::new();
        let next = |path: &str, forward| {
            next_workspace(&t, shell.as_ref(), Some(&at(path)), &none, forward)
        };
        assert_eq!(next(OAUTH, true).as_deref(), Some("z"));
        assert_eq!(next("/web", true).as_deref(), Some("a"));
        assert_eq!(next("/web", false).as_deref(), Some("z"));
        // With nothing shown, the first workspace is next and the last before.
        assert_eq!(
            next_workspace(&t, None, None, &none, true).as_deref(),
            Some("a")
        );
        assert_eq!(
            next_workspace(&t, None, None, &none, false).as_deref(),
            Some("c")
        );
        let nowhere = Some(at("/nowhere"));
        let draft = next_workspace(&t, None, nowhere.as_ref(), &none, true);
        assert_eq!(draft.as_deref(), Some("a"));
        assert_eq!(
            next_workspace(&Tree::default(), None, None, &none, true),
            None
        );
    }

    #[test]
    fn a_workspace_with_no_session_is_passed_over() {
        // `notes` holds only a shell.
        let t = shelled();
        let none = HashMap::new();
        let shell = Some(Tab::Terminal(here(), 5));
        let next =
            |forward| next_workspace(&t, shell.as_ref(), Some(&at("/notes")), &none, forward);
        assert_eq!(next(true).as_deref(), Some("z"));
        assert_eq!(next(false).as_deref(), Some("a"));
        let shown = Some(Tab::Session(here(), "a".into()));
        let after = next_workspace(&t, shown.as_ref(), None, &none, true);
        assert_eq!(after.as_deref(), Some("z"));
    }

    #[test]
    fn with_one_workspace_the_next_is_the_session_last_shown_in_it() {
        let t = tree();
        let only = Tree {
            workspaces: vec![t.workspaces[0].clone()],
        };
        let shown = Some(Tab::Session(here(), "b".into()));
        let last: HashMap<PathBuf, String> = [(PathBuf::from("/api"), "b".to_string())].into();
        let next = next_workspace(&only, shown.as_ref(), None, &last, true);
        assert_eq!(next.as_deref(), Some("b"));
    }

    #[test]
    fn a_session_opens_its_worktree_with_the_sessions_there_as_tabs() {
        let t = tree();
        let place = place_of(&t, &session("a")).unwrap();
        assert_eq!(place, at("/api"));
        let tabs: Vec<_> = sessions_in(&t, &place)
            .iter()
            .map(|s| s.id.clone())
            .collect();
        assert_eq!(tabs, vec!["a", "b"]);
    }

    #[test]
    fn a_workspace_works_in_its_main_checkout_or_its_own_directory() {
        let t = tree();
        assert_eq!(place_of(&t, &ws("/api")), Some(at("/api")));
        assert_eq!(place_of(&t, &ws("/tmp")), Some(at("/tmp")));
        let root = |item: Item| {
            let place = place_of(&t, &item)?;
            t.workspace_of(&place.worktree).map(|w| w.root.clone())
        };
        assert_eq!(root(session("o")), Some(PathBuf::from("/api")));
        assert_eq!(root(worktree(OAUTH)), Some(PathBuf::from("/api")));
    }

    #[test]
    fn a_workspace_without_a_main_checkout_works_in_its_first_worktree() {
        // A bare repository, a submodule or a separate git directory is
        // named by its git directory, which no session should start in.
        let t = testing::tree()
            .checkout("/src/w1", "/src/api.git", "a")
            .session(testing::session("a").cwd("/src/w1"))
            .build();
        assert_eq!(place_of(&t, &ws("/src/api.git")), Some(at("/src/w1")));
    }

    const DEV: &str = "/api/.claude/worktrees/dev";

    fn sh(id: u128, name: &str, dir: &str) -> TerminalSession {
        TerminalSession {
            name: name.into(),
            ..testing::shell(id, dir)
        }
    }

    /// `tree()` with terminals: one beside `api`'s sessions, two in a worktree
    /// where no session runs, one in `tmp`, a workspace without a worktree
    /// row, and one in `notes`, outside git, where nothing else runs.
    fn shelled() -> Tree {
        repositories()
            .checkout(DEV, "/api", "dev")
            .found("/tmp", None)
            .found("/notes", None)
            .sessions(tree().sessions().map(|(_, s)| s.clone()))
            .terminal(sh(1, "shell", "/api"))
            .terminal(sh(2, "shell", DEV))
            .terminal(sh(3, "shell-2", DEV))
            .terminal(sh(4, "shell", "/tmp"))
            .terminal(sh(5, "logs", "/notes"))
            .build()
    }

    fn shell(id: u128) -> Item {
        Item::Terminal(here(), id)
    }

    #[test]
    fn each_shell_has_a_row_after_the_sessions_of_its_worktree() {
        let got = rows(&shelled(), &[], &Folds::default());
        assert_eq!(
            got,
            [
                vec![ws("/api"), worktree("/api")],
                group("/api", &[session("a"), session("b")], &[shell(1)]),
                vec![worktree(DEV)],
                group(DEV, &[], &[shell(2), shell(3)]),
                vec![worktree(OAUTH)],
                group(OAUTH, &[session("o")], &[]),
                vec![ws("/notes")],
                group("/notes", &[], &[shell(5)]),
                vec![ws("/tmp")],
                group("/tmp", &[session("z")], &[shell(4)]),
                vec![ws("/web")],
                group("/web", &[session("c")], &[]),
            ]
            .concat()
        );
    }

    #[test]
    fn folding_a_worktree_or_a_workspace_hides_its_shells() {
        let t = shelled();
        let folded = |row: Item| {
            let collapsed = closed(&[row]);
            rows(&t, &[], &collapsed)
        };
        let list = folded(worktree(DEV));
        assert!(list.contains(&worktree(DEV)) && !list.contains(&shell(2)));
        assert!(!folded(ws("/tmp")).contains(&shell(4)));
    }

    #[test]
    fn a_filter_keeps_the_shells_whose_names_match_as_it_keeps_sessions() {
        let t = shelled();
        let list = rows(&narrowed(&t, "LOG"), &[], &Folds::default());
        assert_eq!(
            list,
            [vec![ws("/notes")], group("/notes", &[], &[shell(5)])].concat()
        );
        // A worktree named by the filter keeps every shell in it.
        let dev = rows(&narrowed(&t, "dev"), &[], &Folds::default());
        assert_eq!(
            dev,
            [
                vec![ws("/api"), worktree(DEV)],
                group(DEV, &[], &[shell(2), shell(3)])
            ]
            .concat()
        );
        let named = rows(&narrowed(&t, "shell-2"), &[], &Folds::default());
        assert_eq!(
            named,
            [
                vec![ws("/api"), worktree(DEV)],
                group(DEV, &[], &[shell(3)])
            ]
            .concat()
        );
    }

    #[test]
    fn a_shell_tab_selects_the_shell_s_row_or_the_nearest_one_shown() {
        let t = shelled();
        let tab = Tab::Terminal(here(), 2);
        let list = rows(&t, &[], &Folds::default());
        assert_eq!(
            row_of(&list, &t, &here(), Some(&tab), &at(DEV)),
            Some(shell(2))
        );
        let collapsed = closed(&[worktree(DEV)]);
        let folded = rows(&t, &[], &collapsed);
        assert_eq!(
            row_of(&folded, &t, &here(), Some(&tab), &at(DEV)),
            Some(worktree(DEV))
        );
    }

    #[test]
    fn the_selection_moves_between_a_session_s_row_and_a_shell_s_with_the_tab() {
        let t = shelled();
        let list = rows(&t, &[], &Folds::default());
        let a = Tab::Session(here(), "a".into());
        let one = Tab::Terminal(here(), 1);
        let switch = |selected: Item, before: &Tab, now: &Tab| {
            row_after_switch(&list, &t, Some(&selected), Some(before), now, &at("/api"))
        };
        assert_eq!(switch(session("a"), &a, &one), Some(shell(1)));
        assert_eq!(switch(shell(1), &one, &a), Some(session("a")));
        // A shell gone, its row with it, gives way to the tab shown next.
        assert_eq!(
            switch(shell(9), &Tab::Terminal(here(), 9), &a),
            Some(session("a"))
        );
        assert_eq!(switch(ws("/web"), &one, &a), None);
    }

    #[test]
    fn a_selected_shell_that_ends_gives_way_to_the_shown_tab_s_row() {
        let t = shelled();
        let list = rows(&t, &[], &Folds::default());
        let shown = Tab::Session(here(), "a".into());
        let kept = row_kept(&list, &t, Some(&shell(9)), Some(&shown), &at("/api"));
        assert_eq!(kept, Some(session("a")));
    }

    #[test]
    fn a_shell_opens_the_directory_it_runs_in() {
        let t = shelled();
        assert_eq!(place_of(&t, &shell(2)), Some(at(DEV)));
        assert_eq!(place_of(&t, &shell(5)), Some(at("/notes")));
        assert_eq!(place_of(&t, &shell(9)), None);
    }

    #[test]
    fn a_neovim_on_no_file_sits_under_shells_and_one_on_a_file_has_no_row_of_its_own() {
        let mut nvim = sh(6, "nvim", DEV);
        nvim.program = TerminalProgram::Neovim;
        let on_file = TerminalSession {
            cwd: DEV.into(),
            ..editing(7, &format!("{DEV}/a.rs"))
        };
        let t = shelled().with_terminals(&[sh(2, "shell", DEV), nvim, on_file]);
        let w = t.worktrees().find(|w| w.worktree.path == Path::new(DEV));
        let w = w.unwrap();
        assert_eq!(under(&here(), w, Heading::Terminals), [shell(2), shell(6)]);
        let list = rows(&t, &[], &Folds::default());
        assert_eq!(row_above(&list, &shell(6)), Some(shell(2)));
        assert!(!list.contains(&shell(7)));
    }

    #[test]
    fn a_neovim_on_a_file_selects_the_file_s_row_or_while_folded_away_the_files_heading() {
        let t = tree().with_terminals(&[TerminalSession {
            cwd: "/web".into(),
            ..editing(9, "/web/README.md")
        }]);
        let nvim = Tab::Terminal(here(), 9);
        let folded = items(&here(), &t, &[], &Folds::default(), &read_web);
        assert_eq!(
            row_of(&folded, &t, &here(), Some(&nvim), &at("/web")),
            Some(files_of("/web"))
        );
        let mut folds = Folds::default();
        folds.toggle(&files_of("/web"));
        let open = items(&here(), &t, &[], &folds, &read_web);
        assert_eq!(
            row_of(&open, &t, &here(), Some(&nvim), &at("/web")),
            Some(file("/web", "/web/README.md", false))
        );
    }

    #[test]
    fn a_command_sits_under_shells_in_the_order_started() {
        let mut claude = sh(7, "claude", DEV);
        claude.program = TerminalProgram::Command;
        let t = shelled().with_terminals(&[sh(2, "shell", DEV), claude]);
        let w = t.worktrees().find(|w| w.worktree.path == Path::new(DEV));
        let w = w.unwrap();
        assert_eq!(under(&here(), w, Heading::Terminals), [shell(2), shell(7)]);
        let list = rows(&t, &[], &Folds::default());
        assert_eq!(row_above(&list, &shell(7)), Some(shell(2)));
    }

    #[test]
    fn folding_a_heading_hides_its_rows_and_leaves_the_other_headings() {
        let list = rows(&shelled(), &[], &closed(&[shells_of(DEV)]));
        assert!(list.contains(&shells_of(DEV)) && list.contains(&sessions_of(DEV)));
        assert!(!list.contains(&shell(2)));
    }

    #[test]
    fn a_fold_toggled_twice_is_open_again_and_a_row_that_does_not_fold_is_never_open() {
        let mut folds = Folds::default();
        assert!(folds.is_open(&sessions_of("/api")));
        folds.toggle(&sessions_of("/api"));
        assert!(!folds.is_open(&sessions_of("/api")));
        folds.toggle(&sessions_of("/api"));
        assert!(folds.is_open(&sessions_of("/api")));
        folds.set(&Item::Terminal(here(), 3), true);
        assert!(!folds.is_open(&Item::Terminal(here(), 3)));
    }

    #[test]
    fn a_flat_workspace_holds_its_headings_one_level_higher_than_a_worktree_does() {
        let t = shelled();
        assert_eq!(depth(&t, &ws("/api")), 0);
        assert_eq!(depth(&t, &worktree(DEV)), 1);
        assert_eq!(depth(&t, &shells_of(DEV)), 2);
        assert_eq!(depth(&t, &shell(2)), 3);
        assert_eq!(depth(&t, &shells_of("/tmp")), 1);
        assert_eq!(depth(&t, &shell(4)), 2);
        assert_eq!(depth(&t, &session("z")), 2);
    }

    #[test]
    fn an_opened_session_sits_as_deep_as_any_session_where_it_runs() {
        let t = opening_tree(true);
        assert_eq!(depth(&t, &session("a")), 3);
        assert_eq!(depth(&t, &session("o")), 3);
        assert_eq!(depth(&t, &session("w")), 2);
    }

    #[test]
    fn a_closed_row_gives_way_to_the_one_above_it() {
        let list = rows(&shelled(), &[], &Folds::default());
        assert_eq!(row_above(&list, &shell(3)), Some(shell(2)));
        assert_eq!(row_above(&list, &shell(2)), Some(shells_of(DEV)));
        assert_eq!(row_above(&list, &ws("/api")), None);
        assert_eq!(row_above(&list, &shell(9)), None);
    }

    #[test]
    fn shells_are_passed_over_going_from_session_to_session() {
        let t = shelled();
        let shown = Some(Tab::Session(here(), "b".into()));
        let next = next_session(&t, shown.as_ref(), None, true);
        assert_eq!(next.as_deref(), Some("o"));
    }

    fn listed(id: &str, session_id: &str) -> Session {
        testing::session(id).session_id(session_id).build()
    }

    #[test]
    fn tabs_kept_for_sessions_go_with_them() {
        let sessions = [listed("a08bbcf2", "a08bbcf2-2cbf"), listed("b", "b")];
        let kept: HashSet<Tab> = [
            Tab::Session(here(), "a08bbcf2".into()),
            Tab::Session(here(), "b".into()),
            Tab::Session(here(), "gone".into()),
            Tab::Terminal(here(), 1),
        ]
        .into();
        let expected: HashSet<Tab> = [
            Tab::Session(here(), "a08bbcf2".into()),
            Tab::Session(here(), "b".into()),
            Tab::Terminal(here(), 1),
        ]
        .into();
        assert_eq!(followed(&sessions, &kept), expected);
    }
    fn editing(id: u128, path: &str) -> TerminalSession {
        TerminalSession {
            program: TerminalProgram::Neovim,
            file: Some(path.into()),
            ..sh(id, "nvim", "/api")
        }
    }

    #[test]
    fn a_session_s_row_opens_the_session_in_its_worktree() {
        let t = tree();
        assert_eq!(
            opens(&session("o"), &t, &[]),
            Opens::Shows(Tab::Session(here(), "o".into()), Some(at(OAUTH)))
        );
        assert_eq!(
            opens(&Item::Waiting(here(), "b".into()), &t, &[]),
            Opens::Shows(Tab::Session(here(), "b".into()), Some(at("/api")))
        );
    }

    #[test]
    fn a_worktree_s_row_opens_its_first_session_or_else_a_new_one() {
        let t = tree();
        assert_eq!(
            opens(&worktree(OAUTH), &t, &[]),
            Opens::Shows(Tab::Session(here(), "o".into()), Some(at(OAUTH)))
        );
        assert_eq!(
            opens(&worktree("/none"), &t, &[]),
            Opens::Draft("/none".into())
        );
    }

    #[test]
    fn a_file_opens_in_its_viewer_or_in_the_neovim_already_on_it_or_a_new_one() {
        let t = tree();
        let nvim = [editing(9, "/api/a.rs")];
        assert_eq!(
            opens(&file("/api", "/api/logo.png", false), &t, &nvim),
            Opens::Shows(
                Tab::Viewer(here(), "/api/logo.png".into()),
                Some(at("/api"))
            )
        );
        assert_eq!(
            opens(&file("/api", "/api/a.rs", false), &t, &nvim),
            Opens::Shows(Tab::Terminal(here(), 9), Some(at("/api")))
        );
        assert_eq!(
            opens(&file("/api", "/api/b.rs", false), &t, &nvim),
            Opens::Edit("/api".into(), "/api/b.rs".into())
        );
    }

    #[test]
    fn a_workspace_a_heading_and_a_directory_fold_rather_than_open() {
        let t = tree();
        let dir = Item::File {
            environment: here(),
            worktree: "/api".into(),
            path: "/api/src".into(),
            dir: true,
        };
        for item in [ws("/api"), sessions_of("/api"), dir] {
            assert_eq!(opens(&item, &t, &[]), Opens::Fold, "{item:?}");
        }
    }

    #[test]
    fn a_folded_row_unfolds_before_it_opens_whatever_kind_it_is() {
        let folding = [
            ws("/api"),
            worktree(OAUTH),
            sessions_of("/api"),
            files_of("/api"),
            file("/api", "/api/src", true),
        ];
        let mut folds = Folds::default();
        for item in &folding {
            folds.set(item, false);
        }
        for item in &folding {
            assert!(unfolds(item, &folds), "{item:?}");
        }
    }

    #[test]
    fn an_unfolded_row_or_one_that_does_not_fold_opens_as_it_always_does() {
        let mut folds = Folds::default();
        folds.set(&files_of("/api"), true);
        folds.set(&session("o"), false);
        for item in [
            ws("/api"),
            worktree(OAUTH),
            sessions_of("/api"),
            files_of("/api"),
            session("a"),
            session("o"),
            Item::Waiting(here(), "a".into()),
            Item::Terminal(here(), 3),
            file("/api", "/api/a.rs", false),
        ] {
            assert!(!unfolds(&item, &folds), "{item:?}");
        }
    }

    #[test]
    fn a_session_has_no_rows_to_fold() {
        assert!(!holds(&session("a")));
        assert!(!holds(&Item::Waiting(here(), "a".into())));
        assert!(holds(&worktree(OAUTH)));
    }

    #[test]
    fn a_row_stands_for_what_windows_show_of_it_a_file_for_its_viewer_and_its_neovim() {
        let nvim = [editing(9, "/api/a.rs"), editing(10, "/api/b.rs")];
        assert_eq!(
            stands_for(&file("/api", "/api/a.rs", false), &nvim),
            [
                Tab::Viewer(here(), "/api/a.rs".into()),
                Tab::Terminal(here(), 9)
            ]
        );
        assert_eq!(
            stands_for(&Item::Waiting(here(), "b".into()), &nvim),
            [Tab::Session(here(), "b".into())]
        );
        assert_eq!(
            stands_for(&Item::Terminal(here(), 3), &nvim),
            [Tab::Terminal(here(), 3)]
        );
        assert!(stands_for(&worktree("/api"), &nvim).is_empty());
    }

    #[test]
    fn a_file_is_edited_while_a_neovim_started_on_it_runs_and_not_for_one_started_elsewhere() {
        let mut elsewhere = sh(3, "nvim", "/api");
        elsewhere.program = TerminalProgram::Neovim;
        let running = [editing(9, "/api/a.rs"), elsewhere];
        assert!(edited(&file("/api", "/api/a.rs", false), &running));
        assert!(!edited(&file("/api", "/api/b.rs", false), &running));
        assert!(!edited(&file("/api", "/api", true), &running));
        assert!(!edited(&Item::Terminal(here(), 9), &running));
    }

    #[test]
    fn a_file_or_worktree_row_copies_its_path_and_what_runs_its_name() {
        let t = tree();
        let shells = [sh(3, "build", "/api")];
        assert_eq!(
            copied(&file("/api", "/api/a.rs", false), &t, &shells),
            Some(Copied::Path("/api/a.rs".into()))
        );
        assert_eq!(
            copied(&worktree(OAUTH), &t, &shells),
            Some(Copied::Path(OAUTH.into()))
        );
        assert_eq!(
            copied(&session("o"), &t, &shells),
            Some(Copied::Name("o".into()))
        );
        assert_eq!(
            copied(&Item::Terminal(here(), 3), &t, &shells),
            Some(Copied::Name("build".into()))
        );
        assert_eq!(copied(&sessions_of("/api"), &t, &shells), None);
    }
}
