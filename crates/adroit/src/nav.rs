//! The tree as one column of items: the workspaces, each worktree's headings
//! and what they hold, then the sessions waiting on the user.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use adroit_model::{
    InboxItem, Session, State, TerminalProgram, TerminalSession, Tree, WorktreeNode,
};

use crate::files::Entry;
use crate::tabs::Tab;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    /// A session waiting on the user, listed in the inbox.
    Waiting(String),
    /// A workspace, by the repository or directory it is.
    Workspace(PathBuf),
    Worktree(PathBuf),
    /// A heading of a worktree, holding the rows of one kind.
    Heading(PathBuf, Heading),
    Session(String),
    /// A shell or a Neovim adroit runs, by the daemon's id.
    Terminal(u128),
    /// A file or a directory of a worktree, under its Files heading.
    File {
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
    Shells,
    Editors,
}

impl Heading {
    pub const ALL: [Heading; 4] = [
        Heading::Files,
        Heading::Sessions,
        Heading::Shells,
        Heading::Editors,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Heading::Files => "Files",
            Heading::Sessions => "Sessions",
            Heading::Shells => "Shells",
            Heading::Editors => "Editors",
        }
    }
}

impl Item {
    /// The key used to remember whether a node is folded.
    pub fn fold_key(&self) -> Option<String> {
        match self {
            Item::Workspace(root) => Some(format!("ws:{}", root.display())),
            Item::Worktree(path) => Some(format!("wt:{}", path.display())),
            Item::Heading(path, heading) => Some(format!("{heading:?}:{}", path.display())),
            Item::Session(id) => Some(format!("session:{id}")),
            Item::File {
                worktree,
                path,
                dir: true,
            } => Some(format!("dir:{}:{}", worktree.display(), path.display())),
            _ => None,
        }
    }

    /// Whether the node shows the rows under it before it is unfolded: a
    /// worktree's files are read only once asked for.
    fn starts_open(&self) -> bool {
        !matches!(self, Item::Heading(_, Heading::Files) | Item::File { .. })
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

/// Every item the tree shows, top to bottom, the sessions waiting on the
/// user last. Children of a folded node are left out, and `read` lists a
/// directory's entries once it is open.
pub fn items(
    tree: &Tree,
    inbox: &[InboxItem],
    folds: &Folds,
    read: &dyn Fn(&Path) -> Vec<Entry>,
) -> Vec<Item> {
    let mut out = Vec::new();
    for ws in &tree.workspaces {
        let wi = Item::Workspace(ws.root.clone());
        let ws_open = folds.is_open(&wi);
        out.push(wi);
        if !ws_open {
            continue;
        }
        if ws.flat() {
            for w in &ws.worktrees {
                out.extend(headed(tree, w, folds, read));
            }
            continue;
        }
        for w in &ws.worktrees {
            let ti = Item::Worktree(w.worktree.path.clone());
            let t_open = folds.is_open(&ti);
            out.push(ti);
            if t_open {
                out.extend(headed(tree, w, folds, read));
            }
        }
    }
    out.extend(inbox.iter().map(|i| Item::Waiting(i.session.id.clone())));
    out
}

/// A worktree's headings, each followed by its rows while it is open.
fn headed(
    tree: &Tree,
    w: &WorktreeNode,
    folds: &Folds,
    read: &dyn Fn(&Path) -> Vec<Entry>,
) -> Vec<Item> {
    let mut out = Vec::new();
    for heading in Heading::ALL {
        let hi = Item::Heading(w.worktree.path.clone(), heading);
        let open = folds.is_open(&hi);
        out.push(hi);
        if !open {
            continue;
        }
        match heading {
            Heading::Files => {
                let root = &w.worktree.path;
                files_in(root, root, folds, read, &mut out);
            }
            _ => out.extend(
                under(tree, w, heading)
                    .into_iter()
                    .filter(|i| !folded_away(tree, folds, i)),
            ),
        }
    }
    out
}

/// Whether `item` is a started session whose starter's row is folded.
fn folded_away(tree: &Tree, folds: &Folds, item: &Item) -> bool {
    let Item::Session(id) = item else {
        return false;
    };
    find(tree, id)
        .and_then(|(_, s)| starter(tree, s))
        .is_some_and(|s| !folds.is_open(&Item::Session(s.id.clone())))
}

/// The entries of `dir` in `worktree`, each open directory followed by its
/// own.
fn files_in(
    worktree: &Path,
    dir: &Path,
    folds: &Folds,
    read: &dyn Fn(&Path) -> Vec<Entry>,
    out: &mut Vec<Item>,
) {
    for entry in read(dir) {
        let item = Item::File {
            worktree: worktree.to_path_buf(),
            path: entry.path.clone(),
            dir: entry.dir,
        };
        let open = folds.is_open(&item);
        out.push(item);
        if open {
            files_in(worktree, &entry.path, folds, read, out);
        }
    }
}

/// The heading a terminal running `program` is listed under: a command
/// beside the shells, as it would run in one.
fn listed_under(program: TerminalProgram) -> Heading {
    match program {
        TerminalProgram::Shell | TerminalProgram::Command => Heading::Shells,
        TerminalProgram::Neovim => Heading::Editors,
    }
}

/// What runs in a worktree, under one of its headings; Files holds no such
/// rows. Sessions are each followed by the sessions they started, wherever
/// those run; a session whose starter is listed is listed under it instead.
pub fn under(tree: &Tree, w: &WorktreeNode, heading: Heading) -> Vec<Item> {
    let runs = |heading| {
        w.terminals
            .iter()
            .filter(move |t| listed_under(t.program) == heading)
            .map(|t| Item::Terminal(t.id))
    };
    match heading {
        Heading::Sessions => {
            let mut rows = Vec::new();
            for session in w.sessions.iter().filter(|s| starter(tree, s).is_none()) {
                rows.push(Item::Session(session.id.clone()));
                rows.extend(started_by(tree, session).map(|s| Item::Session(s.id.clone())));
            }
            rows
        }
        Heading::Shells | Heading::Editors => runs(heading).collect(),
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
        Item::Workspace(_) | Item::Waiting(_) => 0,
        Item::Worktree(_) => 1,
        Item::Heading(path, _) => heading_at(path),
        Item::Session(id) => {
            let Some(session) = find(tree, id).map(|(_, s)| s) else {
                return 0;
            };
            match starter(tree, session) {
                Some(s) => find(tree, &s.id).map_or(0, |(p, _)| heading_at(&p.worktree)) + 2,
                None => tree
                    .find(id)
                    .map_or(0, |(_, w, _)| heading_at(&w.worktree.path) + 1),
            }
        }
        Item::Terminal(id) => tree
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

/// The session that started `session`, while it is listed in `tree`.
pub fn starter<'a>(tree: &'a Tree, session: &Session) -> Option<&'a Session> {
    let parent = session.parent?;
    tree.sessions()
        .map(|(_, s)| s)
        .find(|s| s.delegation == Some(parent))
}

/// The sessions `session` started that `tree` lists.
pub fn started_by<'a>(tree: &'a Tree, session: &Session) -> impl Iterator<Item = &'a Session> {
    let own = session.delegation;
    tree.sessions()
        .map(|(_, s)| s)
        .filter(move |s| own.is_some() && s.parent == own)
}

/// Where a session listed under its starter runs, in short, when its starter
/// runs elsewhere: the name of its worktree's directory, or of its own
/// directory within its starter's worktree. `None` when that is its name.
pub fn runs_at(tree: &Tree, session: &Session) -> Option<String> {
    let starter = starter(tree, session)?;
    if session.cwd == starter.cwd {
        return None;
    }
    let worktree = |s: &Session| tree.find(&s.id).map(|(_, w, _)| w.worktree.path.clone());
    let own = worktree(session);
    let place = if own.is_some() && own == worktree(starter) {
        session.cwd.clone()
    } else {
        own.unwrap_or_else(|| session.cwd.clone())
    };
    let name = place.file_name()?.to_string_lossy().into_owned();
    (name != session.name).then_some(name)
}

/// The row `item` sits under in `list`: the nearest above it one level up.
/// The sessions waiting on the user sit under none.
pub fn parent(list: &[Item], tree: &Tree, item: &Item) -> Option<Item> {
    let at = list.iter().position(|i| i == item)?;
    let level = depth(tree, item);
    if level == 0 {
        return None;
    }
    list[..at]
        .iter()
        .rev()
        .find(|i| !matches!(i, Item::Waiting(_)) && depth(tree, i) < level)
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

/// The next (or previous) waiting session after `current`, wrapping around.
pub fn next_waiting(inbox: &[InboxItem], current: Option<&str>, forward: bool) -> Option<String> {
    if inbox.is_empty() {
        return None;
    }
    let n = inbox.len();
    let at = current.and_then(|c| inbox.iter().position(|i| i.session.id == c));
    let i = match (at, forward) {
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
        (None, true) => 0,
        (None, false) => n - 1,
    };
    Some(inbox[i].session.id.clone())
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
        Some(Tab::Session(id)) => all.iter().position(|(_, s)| &s.id == id),
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
        Some(Tab::Session(id)) => tree.find(id).map(|(ws, _, _)| ws.root.clone()),
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
/// git, its tabs are of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub worktree: PathBuf,
}

/// Find a session by id, with the place it opens.
pub fn find<'a>(tree: &'a Tree, id: &str) -> Option<(Place, &'a Session)> {
    tree.find(id).map(|(_, wt, s)| {
        (
            Place {
                worktree: wt.worktree.path.clone(),
            },
            s,
        )
    })
}

/// The id a session listed as `id` before goes by now, when it has been
/// taken over and runs in adroit under an id of its own. `None` while it is still
/// listed as `id`, and once it is gone.
pub fn successor<'a>(sessions: &'a [Session], id: &str) -> Option<&'a str> {
    if sessions.iter().any(|s| s.id == id) {
        return None;
    }
    sessions
        .iter()
        .find(|s| s.session_id == id)
        .map(|s| s.id.as_str())
}

/// Whether the session `id` names is no longer listed, nor goes by another
/// id now.
pub fn is_gone(sessions: &[Session], id: &str) -> bool {
    !sessions.iter().any(|s| s.id == id) && successor(sessions, id).is_none()
}

/// Tabs kept for what they show, with each session's under the id it goes
/// by now, and those of sessions no longer listed left out.
pub fn followed(sessions: &[Session], tabs: &HashSet<Tab>) -> HashSet<Tab> {
    tabs.iter()
        .filter_map(|tab| match tab {
            Tab::Session(id) if sessions.iter().any(|s| &s.id == id) => Some(tab.clone()),
            Tab::Session(id) => successor(sessions, id).map(|n| Tab::Session(n.to_string())),
            _ => Some(tab.clone()),
        })
        .collect()
}

/// The row to select in place of `item` when the session it is for has moved
/// over to adroit: that session's row under its new id, in the inbox while
/// an inbox row is selected and the session still waits.
pub fn successor_row(sessions: &[Session], item: &Item) -> Option<Item> {
    let (Item::Session(id) | Item::Waiting(id)) = item else {
        return None;
    };
    let new = successor(sessions, id)?;
    let waits = sessions
        .iter()
        .any(|s| s.id == new && matches!(s.state, State::Waiting(_)));
    Some(if matches!(item, Item::Waiting(_)) && waits {
        Item::Waiting(new.to_string())
    } else {
        Item::Session(new.to_string())
    })
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
        Item::Waiting(id) => Some(Item::Session(id.clone())),
        Item::Session(id) => Some(Item::Waiting(id.clone())),
        _ => None,
    };
    if let Some(row) = same.filter(|row| list.contains(row)) {
        return Some(row);
    }
    row_of(list, tree, shown, place)
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

/// What the console shows once a tab of `place` has closed: its first
/// session, or else the first of `terminals`, the terminals left in the worktree.
pub fn tab_after_closing(tree: &Tree, place: &Place, terminals: &[u128]) -> Option<Tab> {
    sessions_in(tree, place)
        .first()
        .map(|s| Tab::Session(s.id.clone()))
        .or_else(|| terminals.first().map(|&id| Tab::Terminal(id)))
}

/// The row in `list` for what a tab shows: the session's, the shell's or the
/// viewed file's own row, or else the nearest row above it still shown, its
/// heading, worktree or workspace. `None` when none is listed.
pub fn row_of(list: &[Item], tree: &Tree, shown: Option<&Tab>, place: &Place) -> Option<Item> {
    let mut rows: Vec<Item> = match shown {
        Some(Tab::Session(id)) => vec![Item::Session(id.clone())],
        Some(Tab::Terminal(id)) => vec![Item::Terminal(*id)],
        Some(Tab::Viewer(path)) => vec![
            Item::File {
                worktree: place.worktree.clone(),
                path: path.clone(),
                dir: false,
            },
            Item::Heading(place.worktree.clone(), Heading::Files),
        ],
        _ => Vec::new(),
    };
    rows.extend(heading_of(tree, shown));
    rows.push(Item::Worktree(place.worktree.clone()));
    if let Some(ws) = tree.workspace_of(&place.worktree) {
        rows.push(Item::Workspace(ws.root.clone()));
    }
    rows.into_iter().find(|item| list.contains(item))
}

/// The heading the row of what `shown` shows sits under.
fn heading_of(tree: &Tree, shown: Option<&Tab>) -> Option<Item> {
    match shown? {
        Tab::Session(id) => {
            let (_, w, s) = tree.find(id)?;
            let w = starter(tree, s)
                .and_then(|s| tree.find(&s.id))
                .map_or(w, |(_, w, _)| w);
            Some(Item::Heading(w.worktree.path.clone(), Heading::Sessions))
        }
        Tab::Terminal(id) => {
            let (_, w, t) = tree.terminal(*id)?;
            Some(Item::Heading(
                w.worktree.path.clone(),
                listed_under(t.program),
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
                || matches!((item, tab), (Item::Waiting(w), Tab::Session(o)) if w == o)
                || Some(item) == row_of(list, tree, Some(tab), place).as_ref()
        }
    };
    on_old
        .then(|| row_of(list, tree, Some(now), place))
        .flatten()
}

/// The worktree or directory an item works in: a session's own, the one a
/// shell was started in, a worktree itself, a workspace's main checkout or
/// directory.
pub fn place_of(tree: &Tree, item: &Item) -> Option<Place> {
    match item {
        Item::Waiting(id) | Item::Session(id) => find(tree, id).map(|(p, _)| p),
        Item::Terminal(id) => tree.terminal(*id).map(|(_, _, s)| Place {
            worktree: s.cwd.clone(),
        }),
        Item::Worktree(path) | Item::Heading(path, _) | Item::File { worktree: path, .. } => {
            Some(Place {
                worktree: path.clone(),
            })
        }
        // Not the root itself: a bare repository, a submodule or a separate
        // git directory is named by its git directory.
        Item::Workspace(root) => {
            let ws = tree.workspaces.iter().find(|ws| &ws.root == root)?;
            let main = ws.worktrees.iter().find(|w| w.worktree.is_main());
            main.or(ws.worktrees.first()).map(|w| Place {
                worktree: w.worktree.path.clone(),
            })
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

/// Whether `item` has rows under it to fold: a session only while it heads
/// the sessions it started.
pub fn holds(item: &Item, tree: &Tree) -> bool {
    match item {
        Item::Session(id) => find(tree, id).is_some_and(|(_, s)| {
            starter(tree, s).is_none() && started_by(tree, s).next().is_some()
        }),
        _ => item.fold_key().is_some(),
    }
}

/// Whether opening `item` in place unfolds it instead of what [`opens`]
/// says: a folded row shows its rows first, as fern's `l` does, so a
/// worktree or a session, which otherwise open, can be unfolded again.
pub fn unfolds(item: &Item, tree: &Tree, folds: &Folds) -> bool {
    holds(item, tree) && !folds.is_open(item)
}

/// What opening `item` shows: a file by its kind, in its viewer or in
/// Neovim, the one already started on it if there is one; a worktree its
/// first session, or else a new one.
pub fn opens(item: &Item, tree: &Tree, terminals: &[TerminalSession]) -> Opens {
    match item {
        Item::Workspace(_) | Item::Heading(..) | Item::File { dir: true, .. } => Opens::Fold,
        Item::File {
            worktree,
            path,
            dir: false,
        } => {
            let at = Some(Place {
                worktree: worktree.clone(),
            });
            let editing = terminals
                .iter()
                .find(|t| t.file.as_ref() == Some(path))
                .map(|t| t.id);
            if crate::viewers::FileKind::of(path).has_viewer() {
                Opens::Shows(Tab::Viewer(path.clone()), at)
            } else if let Some(id) = editing {
                Opens::Shows(Tab::Terminal(id), at)
            } else {
                Opens::Edit(worktree.clone(), path.clone())
            }
        }
        Item::Waiting(id) | Item::Session(id) => {
            Opens::Shows(Tab::Session(id.clone()), place_of(tree, item))
        }
        Item::Terminal(id) => Opens::Shows(Tab::Terminal(*id), place_of(tree, item)),
        Item::Worktree(path) => {
            let p = Place {
                worktree: path.clone(),
            };
            match sessions_in(tree, &p).first() {
                Some(s) => Opens::Shows(Tab::Session(s.id.clone()), Some(p)),
                None => Opens::Draft(path.clone()),
            }
        }
    }
}

/// What a window shows of `item`: a file in its viewer or in any Neovim
/// started on it.
pub fn stands_for(item: &Item, terminals: &[TerminalSession]) -> Vec<Tab> {
    match item {
        Item::Waiting(id) | Item::Session(id) => vec![Tab::Session(id.clone())],
        Item::Terminal(id) => vec![Tab::Terminal(*id)],
        Item::File {
            path, dir: false, ..
        } => std::iter::once(Tab::Viewer(path.clone()))
            .chain(
                terminals
                    .iter()
                    .filter(|t| t.file.as_ref() == Some(path))
                    .map(|t| Tab::Terminal(t.id)),
            )
            .collect(),
        _ => Vec::new(),
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
        Item::File { path, .. } | Item::Worktree(path) | Item::Workspace(path) => {
            Some(Copied::Path(path.clone()))
        }
        Item::Waiting(id) | Item::Session(id) => {
            find(tree, id).map(|(_, s)| Copied::Name(s.name.clone()))
        }
        Item::Terminal(id) => terminals
            .iter()
            .find(|t| t.id == *id)
            .map(|t| Copied::Name(t.name.clone())),
        Item::Heading(..) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use adroit_model::{Kind, State, TerminalSession, Wait, Worktree, build, inbox};

    fn s(id: &str, cwd: &str, state: State) -> Session {
        Session {
            id: id.into(),
            name: id.into(),
            cwd: cwd.into(),
            kind: Kind::Own,
            state,
            ..Default::default()
        }
    }

    fn wt(path: &str, repo: &str, branch: &str) -> Worktree {
        Worktree {
            path: path.into(),
            repo: repo.into(),
            branch: Some(branch.into()),
        }
    }

    const OAUTH: &str = "/api/.claude/worktrees/oauth";

    /// `api` is a repository with sessions in its main checkout and in a
    /// worktree; `web` one with sessions only in its main checkout; `tmp` is
    /// outside git.
    fn tree() -> Tree {
        let found: HashMap<PathBuf, Option<Worktree>> = [
            ("/api", Some(wt("/api", "/api", "main"))),
            (OAUTH, Some(wt(OAUTH, "/api", "oauth"))),
            ("/web", Some(wt("/web", "/web", "main"))),
            ("/tmp", None),
        ]
        .into_iter()
        .map(|(p, w)| (PathBuf::from(p), w))
        .collect();
        build(
            &[
                s("a", "/api", State::Working),
                s("b", "/api", State::Waiting(Wait::Permission)),
                s("o", OAUTH, State::Idle),
                s("c", "/web", State::Idle),
                s("z", "/tmp", State::Waiting(Wait::Input)),
            ],
            &[],
            &found,
            &[],
            None,
        )
    }

    fn ws(root: &str) -> Item {
        Item::Workspace(root.into())
    }

    fn worktree(path: &str) -> Item {
        Item::Worktree(path.into())
    }

    fn session(id: &str) -> Item {
        Item::Session(id.into())
    }

    fn sessions_of(path: &str) -> Item {
        Item::Heading(path.into(), Heading::Sessions)
    }

    fn shells_of(path: &str) -> Item {
        Item::Heading(path.into(), Heading::Shells)
    }

    fn editors_of(path: &str) -> Item {
        Item::Heading(path.into(), Heading::Editors)
    }

    fn files_of(path: &str) -> Item {
        Item::Heading(path.into(), Heading::Files)
    }

    /// A worktree's headings with the rows under each, its files folded.
    fn group(path: &str, sessions: &[Item], shells: &[Item], editors: &[Item]) -> Vec<Item> {
        [
            &[files_of(path), sessions_of(path)][..],
            sessions,
            &[shells_of(path)],
            shells,
            &[editors_of(path)],
            editors,
        ]
        .concat()
    }

    /// The rows of `tree` with no file anywhere.
    fn rows(tree: &Tree, inbox: &[InboxItem], folds: &Folds) -> Vec<Item> {
        items(tree, inbox, folds, &|_| Vec::new())
    }

    fn file(worktree: &str, path: &str, dir: bool) -> Item {
        Item::File {
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
        let folded = items(&t, &[], &Folds::default(), &read_web);
        assert!(!folded.iter().any(|i| matches!(i, Item::File { .. })));
        let mut folds = Folds::default();
        folds.toggle(&files_of("/web"));
        let open = items(&t, &[], &folds, &read_web);
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
        let open = items(&t, &[], &folds, &read_web);
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
        let list = items(&t, &[], &folds, &read_web);
        let main = file("/web", "/web/src/main.rs", false);
        assert_eq!(
            parent(&list, &t, &main),
            Some(file("/web", "/web/src", true))
        );
        let readme = file("/web", "/web/README.md", false);
        assert_eq!(parent(&list, &t, &readme), Some(files_of("/web")));
        assert_eq!(parent(&list, &t, &files_of("/web")), Some(ws("/web")));
        assert_eq!(parent(&list, &t, &ws("/web")), None);
    }

    fn closed(rows: &[Item]) -> Folds {
        let mut folds = Folds::default();
        for row in rows {
            folds.set(row, false);
        }
        folds
    }

    fn at(path: &str) -> Place {
        Place {
            worktree: path.into(),
        }
    }

    #[test]
    fn each_workspace_lists_its_worktrees_headings_and_rows_then_the_inbox_comes_last() {
        let t = tree();
        let got = rows(&t, &inbox(&t), &Folds::default());
        let expected = [
            vec![ws("/api"), worktree("/api")],
            group("/api", &[session("a"), session("b")], &[], &[]),
            vec![worktree(OAUTH)],
            group(OAUTH, &[session("o")], &[], &[]),
            vec![ws("/tmp")],
            group("/tmp", &[session("z")], &[], &[]),
            vec![ws("/web")],
            group("/web", &[session("c")], &[], &[]),
            vec![Item::Waiting("b".into()), Item::Waiting("z".into())],
        ]
        .concat();
        assert_eq!(got, expected);
    }

    /// `a` runs in the main checkout of `api` and started `o`, which runs in
    /// the `oauth` worktree, and `w`, which runs in `web`.
    fn delegating_tree(starter_listed: bool) -> Tree {
        let found: HashMap<PathBuf, Option<Worktree>> = [
            ("/api", Some(wt("/api", "/api", "main"))),
            (OAUTH, Some(wt(OAUTH, "/api", "oauth"))),
            ("/web", Some(wt("/web", "/web", "main"))),
        ]
        .into_iter()
        .map(|(p, w)| (PathBuf::from(p), w))
        .collect();
        let starter = adroit_model::SessionId::new();
        let mut a = s("a", "/api", State::Working);
        a.delegation = Some(starter);
        let started = |id: &str, cwd: &str| Session {
            delegation: Some(adroit_model::SessionId::new()),
            parent: Some(starter),
            ..s(id, cwd, State::Working)
        };
        let mut sessions = vec![started("o", OAUTH), started("w", "/web")];
        if starter_listed {
            sessions.push(a);
        }
        build(&sessions, &[], &found, &[], None)
    }

    #[test]
    fn a_started_session_is_listed_under_its_starter_wherever_it_runs() {
        let t = delegating_tree(true);
        assert_eq!(
            rows(&t, &[], &Folds::default()),
            [
                vec![ws("/api"), worktree("/api")],
                group(
                    "/api",
                    &[session("a"), session("o"), session("w")],
                    &[],
                    &[]
                ),
                vec![worktree(OAUTH)],
                group(OAUTH, &[], &[], &[]),
                vec![ws("/web")],
                group("/web", &[], &[], &[]),
            ]
            .concat()
        );
        let o = find(&t, "o").unwrap().1;
        assert_eq!(starter(&t, o).map(|s| s.id.as_str()), Some("a"));
    }

    #[test]
    fn a_folded_session_hides_the_sessions_it_started() {
        let t = delegating_tree(true);
        let mut folds = Folds::default();
        folds.set(&session("a"), false);
        assert_eq!(
            rows(&t, &[], &folds),
            [
                vec![ws("/api"), worktree("/api")],
                group("/api", &[session("a")], &[], &[]),
                vec![worktree(OAUTH)],
                group(OAUTH, &[], &[], &[]),
                vec![ws("/web")],
                group("/web", &[], &[], &[]),
            ]
            .concat()
        );
    }

    #[test]
    fn a_started_session_running_elsewhere_names_the_worktree_s_directory_not_its_path() {
        let t = delegating_tree(true);
        let at = |id: &str| runs_at(&t, find(&t, id).unwrap().1);
        assert_eq!(at("o").as_deref(), Some("oauth"));
        assert_eq!(at("w").as_deref(), Some("web"));
        assert_eq!(at("a"), None);
    }

    #[test]
    fn a_started_session_named_after_where_it_runs_names_it_once() {
        let t = delegating_tree(true);
        let mut o = find(&t, "o").unwrap().1.clone();
        o.name = "oauth".into();
        assert_eq!(runs_at(&t, &o), None);
    }

    #[test]
    fn a_started_session_whose_starter_has_ended_is_listed_where_it_runs() {
        let t = delegating_tree(false);
        assert_eq!(
            rows(&t, &[], &Folds::default()),
            [
                vec![ws("/api"), worktree(OAUTH)],
                group(OAUTH, &[session("o")], &[], &[]),
                vec![ws("/web")],
                group("/web", &[session("w")], &[], &[]),
            ]
            .concat()
        );
    }

    #[test]
    fn after_a_tab_closes_the_worktree_s_first_session_is_shown() {
        assert_eq!(
            tab_after_closing(&tree(), &at("/api"), &[7]),
            Some(Tab::Session("a".into()))
        );
    }

    #[test]
    fn a_worktree_without_sessions_shows_another_shell_or_nothing() {
        assert_eq!(
            tab_after_closing(&tree(), &at("/nowhere"), &[7]),
            Some(Tab::Terminal(7))
        );
        assert_eq!(tab_after_closing(&tree(), &at("/nowhere"), &[]), None);
    }

    #[test]
    fn the_selection_moves_to_the_new_tab_s_row_only_from_the_old_one_s() {
        let t = tree();
        let list = rows(&t, &inbox(&t), &Folds::default());
        let shell = Tab::Terminal(1);
        let a = Tab::Session("a".into());
        let row_a = Some(session("a"));
        let switch = |selected: Option<Item>, before: Option<&Tab>| {
            row_after_switch(&list, &t, selected.as_ref(), before, &a, &at("/api"))
        };
        // On the shell's row, its worktree's, it moves to the session's.
        assert_eq!(switch(Some(worktree("/api")), Some(&shell)), row_a);
        // So it does from nothing, or from the old session's inbox row.
        assert_eq!(switch(None, None), row_a);
        let b = Tab::Session("b".into());
        assert_eq!(switch(Some(Item::Waiting("b".into())), Some(&b)), row_a);
        // Moved elsewhere by hand, it stays.
        assert_eq!(switch(Some(ws("/web")), Some(&shell)), None);
        // On a row no longer listed, it moves too.
        let gone = Tab::Session("gone".into());
        assert_eq!(switch(Some(session("gone")), Some(&gone)), row_a);
    }

    #[test]
    fn a_selected_row_that_leaves_gives_way_to_the_same_session_s_or_the_shown_tab_s() {
        let t = tree();
        let list = rows(&t, &inbox(&t), &Folds::default());
        let shown = Tab::Session("a".into());
        let kept = |row: Item| row_kept(&list, &t, Some(&row), Some(&shown), &at("/api"));
        // "a" no longer waits: its inbox row leaves for its own.
        assert_eq!(kept(Item::Waiting("a".into())), Some(session("a")));
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
                group(OAUTH, &[session("o")], &[], &[])
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
            [vec![ws("/web")], group("/web", &[session("c")], &[], &[])].concat()
        );
    }

    #[test]
    fn a_session_is_gone_when_neither_it_nor_a_successor_is_listed() {
        let t = tree();
        let sessions: Vec<Session> = t.sessions().map(|(_, s)| s.clone()).collect();
        assert!(!is_gone(&sessions, "a"));
        assert!(is_gone(&sessions, "nowhere"));
        let moved = Session {
            id: "job".into(),
            session_id: "uuid".into(),
            ..Default::default()
        };
        assert!(!is_gone(&[moved], "uuid"));
    }

    #[test]
    fn a_session_tab_selects_the_session_s_row() {
        let t = tree();
        let list = rows(&t, &inbox(&t), &Folds::default());
        assert_eq!(
            row_of(&list, &t, Some(&Tab::Session("b".into())), &at("/api")),
            Some(session("b"))
        );
    }

    #[test]
    fn a_viewer_selects_its_file_s_row_or_while_folded_away_the_files_heading() {
        let t = tree();
        let viewer = Tab::Viewer("/web/README.md".into());
        let folded = items(&t, &[], &Folds::default(), &read_web);
        assert_eq!(
            row_of(&folded, &t, Some(&viewer), &at("/web")),
            Some(files_of("/web"))
        );
        let mut folds = Folds::default();
        folds.toggle(&files_of("/web"));
        let open = items(&t, &[], &folds, &read_web);
        assert_eq!(
            row_of(&open, &t, Some(&viewer), &at("/web")),
            Some(file("/web", "/web/README.md", false))
        );
    }

    #[test]
    fn a_tab_without_a_session_selects_its_worktree() {
        let t = tree();
        let list = rows(&t, &[], &Folds::default());
        assert_eq!(row_of(&list, &t, None, &at("/api")), Some(worktree("/api")));
    }

    #[test]
    fn a_session_folded_away_selects_the_nearest_row_still_shown() {
        let t = tree();
        for row in [sessions_of("/api"), worktree("/api"), ws("/api")] {
            let list = rows(&t, &[], &closed(std::slice::from_ref(&row)));
            assert_eq!(
                row_of(&list, &t, Some(&Tab::Session("a".into())), &at("/api")),
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
            row_of(&list, &t, Some(&Tab::Session("z".into())), &at("/tmp")),
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
        let ib = inbox(&t);
        assert_eq!(next_waiting(&ib, None, true).as_deref(), Some("b"));
        assert_eq!(next_waiting(&ib, Some("b"), true).as_deref(), Some("z"));
        assert_eq!(next_waiting(&ib, Some("z"), true).as_deref(), Some("b"));
        assert_eq!(next_waiting(&ib, Some("b"), false).as_deref(), Some("z"));
    }

    #[test]
    fn the_next_session_goes_down_the_tree_and_wraps_around() {
        let t = tree();
        let shown = |id: &str| Some(Tab::Session(id.into()));
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
        let shell = Some(Tab::Terminal(1));
        let next = |path: &str, forward| next_session(&t, shell.as_ref(), Some(&at(path)), forward);
        assert_eq!(next("/api", true).as_deref(), Some("o"));
        assert_eq!(next("/api", false).as_deref(), Some("b"));
        assert_eq!(next(OAUTH, true).as_deref(), Some("z"));
        assert_eq!(next("/web", true).as_deref(), Some("a"));
        // A session gone stands where its tab is, as a shell's does.
        let gone = Some(Tab::Session("gone".into()));
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
        let shown = |id: &str| Some(Tab::Session(id.into()));
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
        let shown = Some(Tab::Session("c".into()));
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
        let shell = Some(Tab::Terminal(1));
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
        let shell = Some(Tab::Terminal(5));
        let next =
            |forward| next_workspace(&t, shell.as_ref(), Some(&at("/notes")), &none, forward);
        assert_eq!(next(true).as_deref(), Some("z"));
        assert_eq!(next(false).as_deref(), Some("a"));
        let shown = Some(Tab::Session("a".into()));
        let after = next_workspace(&t, shown.as_ref(), None, &none, true);
        assert_eq!(after.as_deref(), Some("z"));
    }

    #[test]
    fn with_one_workspace_the_next_is_the_session_last_shown_in_it() {
        let t = tree();
        let only = Tree {
            workspaces: vec![t.workspaces[0].clone()],
        };
        let shown = Some(Tab::Session("b".into()));
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
        let found = HashMap::from([(
            PathBuf::from("/src/w1"),
            Some(wt("/src/w1", "/src/api.git", "a")),
        )]);
        let t = build(&[s("a", "/src/w1", State::Idle)], &[], &found, &[], None);
        assert_eq!(place_of(&t, &ws("/src/api.git")), Some(at("/src/w1")));
    }

    const DEV: &str = "/api/.claude/worktrees/dev";

    fn sh(id: u128, name: &str, dir: &str) -> TerminalSession {
        TerminalSession {
            program: Default::default(),
            id,
            name: name.into(),
            cwd: dir.into(),
            ..Default::default()
        }
    }

    /// `tree()` with terminals: one beside `api`'s sessions, two in a worktree
    /// where no session runs, one in `tmp`, a workspace without a worktree
    /// row, and one in `notes`, outside git, where nothing else runs.
    fn shelled() -> Tree {
        let found: HashMap<PathBuf, Option<Worktree>> = [
            ("/api", Some(wt("/api", "/api", "main"))),
            (OAUTH, Some(wt(OAUTH, "/api", "oauth"))),
            (DEV, Some(wt(DEV, "/api", "dev"))),
            ("/web", Some(wt("/web", "/web", "main"))),
            ("/tmp", None),
            ("/notes", None),
        ]
        .into_iter()
        .map(|(p, w)| (PathBuf::from(p), w))
        .collect();
        let sessions: Vec<Session> = tree().sessions().map(|(_, s)| s.clone()).collect();
        build(
            &sessions,
            &[
                sh(1, "shell", "/api"),
                sh(2, "shell", DEV),
                sh(3, "shell-2", DEV),
                sh(4, "shell", "/tmp"),
                sh(5, "logs", "/notes"),
            ],
            &found,
            &[],
            None,
        )
    }

    fn shell(id: u128) -> Item {
        Item::Terminal(id)
    }

    #[test]
    fn each_shell_has_a_row_after_the_sessions_of_its_worktree() {
        let got = rows(&shelled(), &[], &Folds::default());
        assert_eq!(
            got,
            [
                vec![ws("/api"), worktree("/api")],
                group("/api", &[session("a"), session("b")], &[shell(1)], &[]),
                vec![worktree(DEV)],
                group(DEV, &[], &[shell(2), shell(3)], &[]),
                vec![worktree(OAUTH)],
                group(OAUTH, &[session("o")], &[], &[]),
                vec![ws("/notes")],
                group("/notes", &[], &[shell(5)], &[]),
                vec![ws("/tmp")],
                group("/tmp", &[session("z")], &[shell(4)], &[]),
                vec![ws("/web")],
                group("/web", &[session("c")], &[], &[]),
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
            [vec![ws("/notes")], group("/notes", &[], &[shell(5)], &[])].concat()
        );
        // A worktree named by the filter keeps every shell in it.
        let dev = rows(&narrowed(&t, "dev"), &[], &Folds::default());
        assert_eq!(
            dev,
            [
                vec![ws("/api"), worktree(DEV)],
                group(DEV, &[], &[shell(2), shell(3)], &[])
            ]
            .concat()
        );
        let named = rows(&narrowed(&t, "shell-2"), &[], &Folds::default());
        assert_eq!(
            named,
            [
                vec![ws("/api"), worktree(DEV)],
                group(DEV, &[], &[shell(3)], &[])
            ]
            .concat()
        );
    }

    #[test]
    fn a_shell_tab_selects_the_shell_s_row_or_the_nearest_one_shown() {
        let t = shelled();
        let tab = Tab::Terminal(2);
        let list = rows(&t, &[], &Folds::default());
        assert_eq!(row_of(&list, &t, Some(&tab), &at(DEV)), Some(shell(2)));
        let collapsed = closed(&[worktree(DEV)]);
        let folded = rows(&t, &[], &collapsed);
        assert_eq!(
            row_of(&folded, &t, Some(&tab), &at(DEV)),
            Some(worktree(DEV))
        );
    }

    #[test]
    fn the_selection_moves_between_a_session_s_row_and_a_shell_s_with_the_tab() {
        let t = shelled();
        let list = rows(&t, &[], &Folds::default());
        let a = Tab::Session("a".into());
        let one = Tab::Terminal(1);
        let switch = |selected: Item, before: &Tab, now: &Tab| {
            row_after_switch(&list, &t, Some(&selected), Some(before), now, &at("/api"))
        };
        assert_eq!(switch(session("a"), &a, &one), Some(shell(1)));
        assert_eq!(switch(shell(1), &one, &a), Some(session("a")));
        // A shell gone, its row with it, gives way to the tab shown next.
        assert_eq!(switch(shell(9), &Tab::Terminal(9), &a), Some(session("a")));
        assert_eq!(switch(ws("/web"), &one, &a), None);
    }

    #[test]
    fn a_selected_shell_that_ends_gives_way_to_the_shown_tab_s_row() {
        let t = shelled();
        let list = rows(&t, &[], &Folds::default());
        let shown = Tab::Session("a".into());
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
    fn neovim_sits_under_editors_apart_from_the_shells() {
        let mut nvim = sh(6, "nvim", DEV);
        nvim.program = TerminalProgram::Neovim;
        let t = shelled().with_terminals(&[sh(2, "shell", DEV), nvim]);
        let w = t.worktrees().find(|w| w.worktree.path == Path::new(DEV));
        let w = w.unwrap();
        assert_eq!(under(&t, w, Heading::Shells), [shell(2)]);
        assert_eq!(under(&t, w, Heading::Editors), [shell(6)]);
        let list = rows(&t, &[], &Folds::default());
        assert_eq!(row_above(&list, &shell(6)), Some(editors_of(DEV)));
    }

    #[test]
    fn a_command_sits_under_shells_in_the_order_started() {
        let mut claude = sh(7, "claude", DEV);
        claude.program = TerminalProgram::Command;
        let t = shelled().with_terminals(&[sh(2, "shell", DEV), claude]);
        let w = t.worktrees().find(|w| w.worktree.path == Path::new(DEV));
        let w = w.unwrap();
        assert_eq!(under(&t, w, Heading::Shells), [shell(2), shell(7)]);
        assert_eq!(under(&t, w, Heading::Editors), []);
        let list = rows(&t, &[], &Folds::default());
        assert_eq!(row_above(&list, &shell(7)), Some(shell(2)));
    }

    #[test]
    fn folding_a_heading_hides_its_rows_and_leaves_the_other_headings() {
        let list = rows(&shelled(), &[], &closed(&[shells_of(DEV)]));
        assert!(list.contains(&shells_of(DEV)) && list.contains(&editors_of(DEV)));
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
        folds.set(&Item::Terminal(3), true);
        assert!(!folds.is_open(&Item::Terminal(3)));
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
    fn a_started_session_sits_a_level_below_its_starter() {
        let t = delegating_tree(true);
        assert_eq!(depth(&t, &session("a")), 3);
        assert_eq!(depth(&t, &session("w")), 4);
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
        let shown = Some(Tab::Session("b".into()));
        let next = next_session(&t, shown.as_ref(), None, true);
        assert_eq!(next.as_deref(), Some("o"));
    }

    fn listed(id: &str, session_id: &str) -> Session {
        Session {
            id: id.into(),
            session_id: session_id.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_session_taken_over_is_found_under_its_new_id() {
        let sessions = [listed("a08bbcf2", "a08bbcf2-2cbf")];
        assert_eq!(successor(&sessions, "a08bbcf2-2cbf"), Some("a08bbcf2"));
    }

    #[test]
    fn a_session_still_listed_or_gone_has_no_successor() {
        let sessions = [listed("a08bbcf2-2cbf", "a08bbcf2-2cbf")];
        assert_eq!(successor(&sessions, "a08bbcf2-2cbf"), None);
        assert_eq!(successor(&sessions, "ffffffff-0000"), None);
    }

    #[test]
    fn tabs_kept_for_sessions_follow_them_and_go_with_them() {
        let sessions = [listed("a08bbcf2", "a08bbcf2-2cbf"), listed("b", "b")];
        let kept: HashSet<Tab> = [
            Tab::Session("a08bbcf2-2cbf".into()),
            Tab::Session("b".into()),
            Tab::Session("gone".into()),
            Tab::Terminal(1),
        ]
        .into();
        let expected: HashSet<Tab> = [
            Tab::Session("a08bbcf2".into()),
            Tab::Session("b".into()),
            Tab::Terminal(1),
        ]
        .into();
        assert_eq!(followed(&sessions, &kept), expected);
    }

    #[test]
    fn the_row_of_a_session_taken_over_follows_it() {
        let sessions = [listed("a08bbcf2", "a08bbcf2-2cbf")];
        let moved = Some(session("a08bbcf2"));
        assert_eq!(successor_row(&sessions, &session("a08bbcf2-2cbf")), moved);
        assert_eq!(
            successor_row(&sessions, &Item::Waiting("a08bbcf2-2cbf".into())),
            moved
        );
    }

    #[test]
    fn an_inbox_row_stays_in_the_inbox_while_its_moved_session_waits() {
        let sessions = [Session {
            state: State::Waiting(Wait::Input),
            ..listed("a08bbcf2", "a08bbcf2-2cbf")
        }];
        assert_eq!(
            successor_row(&sessions, &Item::Waiting("a08bbcf2-2cbf".into())),
            Some(Item::Waiting("a08bbcf2".into()))
        );
        assert_eq!(
            successor_row(&sessions, &session("a08bbcf2-2cbf")),
            Some(session("a08bbcf2"))
        );
    }

    #[test]
    fn a_row_not_for_a_moved_session_has_no_successor() {
        let sessions = [listed("a08bbcf2", "a08bbcf2-2cbf")];
        assert_eq!(successor_row(&sessions, &session("a08bbcf2")), None);
        assert_eq!(successor_row(&sessions, &ws("/api")), None);
    }
    fn editing(id: u128, path: &str) -> TerminalSession {
        TerminalSession {
            id,
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
            Opens::Shows(Tab::Session("o".into()), Some(at(OAUTH)))
        );
        assert_eq!(
            opens(&Item::Waiting("b".into()), &t, &[]),
            Opens::Shows(Tab::Session("b".into()), Some(at("/api")))
        );
    }

    #[test]
    fn a_worktree_s_row_opens_its_first_session_or_else_a_new_one() {
        let t = tree();
        assert_eq!(
            opens(&worktree(OAUTH), &t, &[]),
            Opens::Shows(Tab::Session("o".into()), Some(at(OAUTH)))
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
            Opens::Shows(Tab::Viewer("/api/logo.png".into()), Some(at("/api")))
        );
        assert_eq!(
            opens(&file("/api", "/api/a.rs", false), &t, &nvim),
            Opens::Shows(Tab::Terminal(9), Some(at("/api")))
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
        let t = delegating_tree(true);
        let folding = [
            ws("/api"),
            worktree(OAUTH),
            sessions_of("/api"),
            files_of("/api"),
            file("/api", "/api/src", true),
            session("a"),
        ];
        let mut folds = Folds::default();
        for item in &folding {
            folds.set(item, false);
        }
        for item in &folding {
            assert!(unfolds(item, &t, &folds), "{item:?}");
        }
    }

    #[test]
    fn an_unfolded_row_or_one_that_does_not_fold_opens_as_it_always_does() {
        let t = delegating_tree(true);
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
            Item::Waiting("a".into()),
            Item::Terminal(3),
            file("/api", "/api/a.rs", false),
        ] {
            assert!(!unfolds(&item, &t, &folds), "{item:?}");
        }
    }

    #[test]
    fn only_a_session_that_started_sessions_listed_under_it_has_rows_to_fold() {
        let t = delegating_tree(true);
        assert!(holds(&session("a"), &t));
        assert!(!holds(&session("o"), &t));
        assert!(!holds(&Item::Waiting("a".into()), &t));
        assert!(holds(&worktree(OAUTH), &t));
    }

    #[test]
    fn a_row_stands_for_what_windows_show_of_it_a_file_for_its_viewer_and_its_neovim() {
        let nvim = [editing(9, "/api/a.rs"), editing(10, "/api/b.rs")];
        assert_eq!(
            stands_for(&file("/api", "/api/a.rs", false), &nvim),
            [Tab::Viewer("/api/a.rs".into()), Tab::Terminal(9)]
        );
        assert_eq!(
            stands_for(&Item::Waiting("b".into()), &nvim),
            [Tab::Session("b".into())]
        );
        assert_eq!(stands_for(&Item::Terminal(3), &nvim), [Tab::Terminal(3)]);
        assert!(stands_for(&worktree("/api"), &nvim).is_empty());
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
            copied(&Item::Terminal(3), &t, &shells),
            Some(Copied::Name("build".into()))
        );
        assert_eq!(copied(&sessions_of("/api"), &t, &shells), None);
    }
}
