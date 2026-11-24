//! The tree as the window holds it: the rows built from what each
//! environment's work host lists, which nodes are folded, the row selected,
//! the filter narrowing the rows, and what of the tree each host is asked to
//! keep read.

pub mod clipboard;
pub mod nav;
mod place;

pub use place::*;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use adroit_model::files::Entry;
use adroit_model::github::{Listing, Repository};
use adroit_model::{EnvironmentId, InboxItem, State, TerminalSession, Tree, inbox};
use adroit_protocol::{Request, Seq, TreeSight, TreeWork, WorkSnapshot};

use crate::attention::Changes;
use crate::chrome::status_line::{self, StatusLine};
use crate::completion::{Completions, Viewed};
use crate::content::{Showing, Tab};
use crate::effect::Effect;
use crate::host::Hosts;
use nav::{Folds, Heading, Item};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TreeView {
    pub selected: Option<Item>,
    pub folds: Folds,
    /// What the rows are narrowed to.
    pub filter: String,
    /// Whether the filter's field has the keys.
    pub filtering: bool,
    /// Whether every worktree git knows of is listed, those where nothing
    /// runs too.
    pub every_worktree: bool,
    /// Whether the files whose names start with `.` are listed.
    pub show_hidden: bool,
    /// What the tree last put on the system clipboard.
    pub clipboard: Option<clipboard::Held>,
    /// The files and directories marked, in the order they were marked.
    pub marks: Vec<Item>,
    /// The pastes whose targets were taken, to be asked about in turn.
    pub clashes: std::collections::VecDeque<clipboard::Clash>,
    /// The sessions finished that the user has seen since.
    pub completions: Completions,
    /// When the window saw each session's state last change.
    pub changes: Changes,
    built: Built,
    /// The rows the selection was last kept among.
    kept_among: u64,
    /// The tab the selection last followed.
    followed: Option<Tab>,
    /// What was made, selected once it is listed unless the selection has
    /// moved off the row it was on when it was made.
    landing: Option<(Option<Item>, Item)>,
    /// The row gone into, whose first row is selected once it is listed,
    /// as a directory's are once the host has read it.
    into: Option<Item>,
    /// The error the status line was last told the sessions cannot be
    /// listed for.
    failing: Option<String>,
}

/// What [`build`] makes of the trees the hosts listed and of what the user
/// set. Only `build` writes it, so it is never out of step with what it was
/// built from.
#[derive(Debug, Clone, Default, PartialEq)]
struct Built {
    /// Each environment's tree narrowed to what the filter holds.
    narrowed: BTreeMap<EnvironmentId, Tree>,
    /// The sessions waiting on the user, of every environment's whole tree.
    waiting: Vec<(EnvironmentId, InboxItem)>,
    /// The sessions waiting on the user that the filter keeps.
    narrowed_waiting: Vec<(EnvironmentId, InboxItem)>,
    /// Every row, top to bottom, the sessions waiting on the user last.
    rows: Vec<Item>,
    /// The worktrees whose files the rows list, by their environment.
    marked: BTreeSet<(EnvironmentId, PathBuf)>,
    /// The directories whose entries the rows list, by their environment.
    listed: BTreeSet<(EnvironmentId, PathBuf)>,
    /// The listings on GitHub the rows hold open, by their environment.
    github: BTreeSet<(EnvironmentId, PathBuf, Listing)>,
    /// What the rows were last built from.
    from: Option<BuiltFrom>,
    /// Counts the times the rows were built anew.
    times: u64,
}

/// What the rows are built from: each environment by the patch its work is
/// at, the filter, the folds and whether hidden files are listed.
type BuiltFrom = (Vec<(EnvironmentId, Option<Seq>)>, String, Folds, bool);

/// A key on the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Down,
    Up,
    First,
    Last,
    /// Fold the row selected when it is open, or else the row it sits under,
    /// going there.
    Fold,
    /// Unfold the row selected and go to the first row under it, once it is
    /// listed, as fern's `l` does on a directory.
    Into,
    /// Show or hide the files whose names start with `.`.
    Hidden,
}

impl TreeView {
    /// The tree of `environment` narrowed to what the filter holds, empty for
    /// an environment the rows hold nothing of.
    pub fn narrowed(&self, environment: &EnvironmentId) -> &Tree {
        static NONE: std::sync::LazyLock<Tree> = std::sync::LazyLock::new(Tree::default);
        self.built.narrowed.get(environment).unwrap_or(&NONE)
    }

    /// Every row, top to bottom, the sessions waiting on the user last.
    pub fn rows(&self) -> &[Item] {
        &self.built.rows
    }

    /// The sessions waiting on the user, of every environment's whole tree.
    pub fn waiting(&self) -> &[(EnvironmentId, InboxItem)] {
        &self.built.waiting
    }

    /// The sessions waiting on the user that the filter keeps.
    pub fn narrowed_waiting(&self) -> &[(EnvironmentId, InboxItem)] {
        &self.built.narrowed_waiting
    }

    /// How deep `item` sits among the rows: an environment's heading and the
    /// sessions waiting on the user at the top, an environment's rows a level
    /// below its heading. The rows are of the narrowed trees, where a
    /// workspace can be flat that is not in full.
    pub fn depth(&self, item: &Item) -> usize {
        match item {
            Item::Environment(_) | Item::Waiting(_, _) => 0,
            _ => nav::depth(self.narrowed(item.environment()), item) + 1,
        }
    }

    /// How many sessions of `environment` wait on the user, whatever the
    /// filter keeps.
    pub fn waiting_in(&self, environment: &EnvironmentId) -> usize {
        self.built
            .waiting
            .iter()
            .filter(|(e, _)| e == environment)
            .count()
    }

    /// The environment the tree is at: the selected row's, or else `shown`,
    /// the one the window shows.
    pub fn environment_at(&self, shown: &EnvironmentId) -> EnvironmentId {
        self.selected
            .as_ref()
            .map_or(shown, Item::environment)
            .clone()
    }

    /// The row `item` sits under among the rows.
    pub fn parent(&self, item: &Item) -> Option<Item> {
        nav::parent(&self.built.rows, |i| self.depth(i), item)
    }
}

/// Where the content's focused window is, as the header writes it: its
/// workspace, by name and whether it is a scratch one, its worktree, and
/// what the window shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Crumbs {
    pub workspace: Option<(String, bool)>,
    pub worktree: PathBuf,
    pub target: String,
}

/// What the tree's facts are while the host has told none.
fn facts(work: Option<&WorkSnapshot>) -> &TreeWork {
    static NONE: std::sync::LazyLock<TreeWork> = std::sync::LazyLock::new(TreeWork::default);
    work.map_or(&NONE, |w| &w.tree)
}

/// The tree as last listed.
pub fn listed(work: Option<&WorkSnapshot>) -> &Tree {
    facts(work).tree()
}

/// The terminals as last listed.
pub fn terminals(work: Option<&WorkSnapshot>) -> &[TerminalSession] {
    facts(work).terminals()
}

/// What the terminal host took with it, waiting to be brought back, as
/// last listed.
pub fn interrupted(work: Option<&WorkSnapshot>) -> &[adroit_model::Interrupted] {
    facts(work).interrupted()
}

/// What sessions propose to pass on as the user's words, as last listed.
pub fn relays(work: Option<&WorkSnapshot>) -> &[adroit_model::Relay] {
    facts(work).relays()
}

/// The repository at `root` as last read of GitHub, if it is on GitHub.
pub fn github<'a>(work: Option<&'a WorkSnapshot>, root: &Path) -> Option<&'a Repository> {
    facts(work).github.get(root)
}

/// The page on GitHub of the item numbered `number` of `listing` in the
/// repository at `root`, as last read.
pub fn github_page(
    work: Option<&WorkSnapshot>,
    root: &Path,
    listing: Listing,
    number: u64,
) -> Option<String> {
    github(work, root)?
        .item(listing, number)
        .map(|i| i.url.clone())
}

pub(crate) fn select(view: &mut TreeView, row: Option<Item>) -> Vec<Effect> {
    view.selected = row;
    Vec::new()
}

pub(crate) fn fold(view: &mut TreeView, row: &Item, open: bool) -> Vec<Effect> {
    view.folds.set(row, open);
    Vec::new()
}

pub(crate) fn toggle(view: &mut TreeView, row: &Item) -> Vec<Effect> {
    view.folds.toggle(row);
    Vec::new()
}

pub(crate) fn filter(view: &mut TreeView, text: String) -> Vec<Effect> {
    view.filter = text;
    Vec::new()
}

pub(crate) fn filtering(view: &mut TreeView, on: bool) -> Vec<Effect> {
    view.filtering = on;
    Vec::new()
}

/// Every worktree listed, or only those where something runs, looked at
/// afresh.
pub(crate) fn every_worktree(view: &mut TreeView, hosts: &crate::host::Hosts) -> Vec<Effect> {
    view.every_worktree = !view.every_worktree;
    let told = if view.every_worktree {
        "Every worktree is listed, those without sessions too"
    } else {
        "Only the worktrees with sessions are listed"
    };
    let mut effects = hosts.ask_each(Request::Relist);
    effects.push(Effect::Notice(told.into()));
    effects
}

pub(crate) fn press(view: &mut TreeView, key: Key) -> Vec<Effect> {
    let rows = &view.built.rows;
    let current = view.selected.as_ref();
    let next = match key {
        Key::Down => nav::step(rows, current, 1),
        Key::Up => nav::step(rows, current, -1),
        Key::First => rows.first().cloned(),
        Key::Last => rows.last().cloned(),
        Key::Fold => {
            let Some(row) = current.cloned() else {
                return Vec::new();
            };
            if view.folds.is_open(&row) && nav::holds(&row) {
                view.folds.set(&row, false);
                return Vec::new();
            }
            match view.parent(&row) {
                Some(up) => {
                    view.folds.set(&up, false);
                    Some(up)
                }
                None => return Vec::new(),
            }
        }
        Key::Hidden => {
            view.show_hidden = !view.show_hidden;
            return Vec::new();
        }
        Key::Into => {
            if let Some(row) = current.filter(|r| nav::holds(r)) {
                let row = row.clone();
                view.folds.set(&row, true);
                view.into = Some(row);
            }
            return Vec::new();
        }
    };
    view.selected = next;
    Vec::new()
}

/// The rows built anew from the trees the host of each environment listed,
/// when one of them, the filter or the folds changed since they were last
/// built: each environment's under its heading, then
/// the sessions waiting on the user in all of them. A directory's entries
/// come from what its environment's host listed of it.
pub(crate) fn build(view: &mut TreeView, hosts: &Hosts) {
    for (environment, host) in hosts.each() {
        view.folds
            .rest(environment, facts(host.work.as_ref()).tree());
    }
    let from = (
        hosts
            .each()
            .map(|(e, h)| (e.clone(), h.work.as_ref().map(|w| w.seq)))
            .collect(),
        view.filter.clone(),
        view.folds.clone(),
        view.show_hidden,
    );
    if view.built.from.as_ref() == Some(&from) {
        return;
    }
    view.built.from = Some(from);
    view.built.narrowed.clear();
    view.built.waiting.clear();
    view.built.narrowed_waiting.clear();
    let asked = RefCell::new(BTreeSet::new());
    let mut rows = Vec::new();
    for (environment, host) in hosts.each() {
        let facts = facts(host.work.as_ref());
        let narrowed = nav::narrowed(facts.tree(), &view.filter);
        let of = |tree: &Tree| inbox(tree).into_iter().map(|i| (environment.clone(), i));
        view.built.waiting.extend(of(facts.tree()));
        view.built.narrowed_waiting.extend(of(&narrowed));
        let heading = Item::Environment(environment.clone());
        let open = view.folds.is_open(&heading);
        rows.push(heading);
        if open {
            let read = |dir: &Path| {
                asked
                    .borrow_mut()
                    .insert((environment.clone(), dir.to_path_buf()));
                let mut entries = facts.entries.get(dir).cloned().unwrap_or_default();
                if !view.show_hidden {
                    entries.retain(|e| !adroit_model::files::is_hidden(&e.path));
                }
                entries
            };
            rows.extend(nav::items(
                environment,
                &narrowed,
                &view.folds,
                &read as &dyn Fn(&Path) -> Vec<Entry>,
                &facts.github,
            ));
        }
        view.built.narrowed.insert(environment.clone(), narrowed);
    }
    rows.extend(
        view.built
            .narrowed_waiting
            .iter()
            .map(|(environment, i)| Item::Waiting(environment.clone(), i.session.id.clone())),
    );
    view.built.marked = rows
        .iter()
        .filter_map(|row| match row {
            Item::Heading(environment, path, Heading::Files) if view.folds.is_open(row) => {
                Some((environment.clone(), path.clone()))
            }
            _ => None,
        })
        .collect();
    view.built.listed = asked.into_inner();
    view.built.github = rows
        .iter()
        .filter_map(|row| match row {
            Item::GitHub(environment, root, listing) if view.folds.is_open(row) => {
                Some((environment.clone(), root.clone(), *listing))
            }
            _ => None,
        })
        .collect();
    if view.built.rows != rows {
        view.built.rows = rows;
        view.built.times += 1;
    }
}

/// The row gone into gives the selection to the first row under it once
/// that is listed, unless the selection has moved off it meanwhile.
pub(crate) fn go_into(view: &mut TreeView) {
    let Some(row) = view.into.clone() else {
        return;
    };
    if view.selected.as_ref() != Some(&row) {
        view.into = None;
        return;
    }
    let rows = &view.built.rows;
    let first = rows
        .iter()
        .position(|r| *r == row)
        .and_then(|at| rows.get(at + 1))
        .filter(|next| view.parent(next).as_ref() == Some(&row));
    if let Some(first) = first {
        view.selected = Some(first.clone());
        view.into = None;
    }
}

/// `path` was made in `worktree` of `environment`: the directories down to
/// it open, and it is selected once listed.
pub(crate) fn made(
    view: &mut TreeView,
    environment: &EnvironmentId,
    worktree: &Path,
    path: &Path,
    dir: bool,
) -> Vec<Effect> {
    let file = |path: &Path, dir: bool| Item::File {
        environment: environment.clone(),
        worktree: worktree.to_path_buf(),
        path: path.to_path_buf(),
        dir,
    };
    view.folds.set(
        &Item::Heading(environment.clone(), worktree.to_path_buf(), Heading::Files),
        true,
    );
    for up in path
        .ancestors()
        .skip(1)
        .take_while(|up| up.starts_with(worktree) && *up != worktree)
    {
        view.folds.set(&file(up, true), true);
    }
    view.landing = Some((view.selected.clone(), file(path, dir)));
    Vec::new()
}

/// What was made is selected once it is listed, unless the selection has
/// moved meanwhile.
pub(crate) fn land(view: &mut TreeView) {
    let Some((from, to)) = view.landing.take() else {
        return;
    };
    if view.selected != from {
        return;
    }
    // What is pasted is landed on by its path alone: a path the clipboard
    // named does not say whether it is a directory.
    if let Some(row) = view.built.rows.iter().find(|row| same_entry(row, &to)) {
        view.selected = Some(row.clone());
    } else {
        view.landing = Some((from, to));
    }
}

/// Whether `row` is the entry `to` is, or the same path of the same
/// worktree when both are files.
fn same_entry(row: &Item, to: &Item) -> bool {
    match (row, to) {
        (
            Item::File {
                environment,
                worktree,
                path,
                ..
            },
            Item::File {
                environment: e,
                worktree: w,
                path: p,
                ..
            },
        ) => environment == e && worktree == w && path == p,
        _ => row == to,
    }
}

/// The directory a new entry made from `row` goes in, with the environment
/// and the worktree it is of: the directory selected, the one the file
/// selected is in, or the top of the worktree from its Files heading.
/// `None` for a row outside the files.
pub(crate) fn entries_dir(row: &Item) -> Option<(EnvironmentId, PathBuf, PathBuf)> {
    match row {
        Item::Heading(environment, worktree, Heading::Files) => {
            Some((environment.clone(), worktree.clone(), worktree.clone()))
        }
        Item::File {
            environment,
            worktree,
            path,
            dir,
        } => {
            let dir = if *dir {
                path.clone()
            } else {
                path.parent()
                    .map_or_else(|| worktree.clone(), Path::to_path_buf)
            };
            Some((environment.clone(), worktree.clone(), dir))
        }
        _ => None,
    }
}

/// A selected row that leaves the rows, as a waiting one does once its
/// session stops waiting, gives way to another rather than to nothing.
/// Only rows built anew are looked at: a row selected ahead of being
/// listed, as a session just started is, stays selected until it is.
pub(crate) fn keep_selection(view: &mut TreeView, showing: &Showing, work: Option<&WorkSnapshot>) {
    if view.kept_among == view.built.times {
        return;
    }
    view.kept_among = view.built.times;
    let Some(place) = showing.place.clone() else {
        return;
    };
    let row = nav::row_kept(
        &view.built.rows,
        listed(work),
        view.selected.as_ref(),
        showing.tab.as_ref(),
        &place,
    );
    if row.is_some() {
        view.selected = row;
    }
}

/// The selection follows the focused window to what it now shows, however
/// it came to show it, unless the selection was moved elsewhere by hand.
pub(crate) fn follow_switch(view: &mut TreeView, showing: &Showing, work: Option<&WorkSnapshot>) {
    let now = showing.tab.clone();
    if now == view.followed {
        return;
    }
    let before = std::mem::replace(&mut view.followed, now.clone());
    let Some(now) = now else {
        return;
    };
    let tree = listed(work);
    // The worktree the window is in follows what it shows.
    let place = place_of_tab(tree, terminals(work), &now).or_else(|| showing.place.clone());
    let Some(place) = place else {
        return;
    };
    let row = nav::row_after_switch(
        &view.built.rows,
        tree,
        view.selected.as_ref(),
        before.as_ref(),
        &now,
        &place,
    );
    if let Some(row) = row.filter(|row| view.selected.as_ref() != Some(row)) {
        view.selected = Some(row);
    }
}

/// The status line says why the sessions cannot be listed while they
/// cannot, and takes it back once they can.
pub(crate) fn tell_failing(
    view: &mut TreeView,
    status: &mut StatusLine,
    work: Option<&WorkSnapshot>,
) -> Vec<Effect> {
    let now = facts(work).failing.clone();
    if now == view.failing {
        return Vec::new();
    }
    let last = std::mem::replace(&mut view.failing, now.clone());
    status_line::failing(status, last.as_deref(), now.as_deref())
}

/// Typing into the filter stops once the keys leave the sidebar or a menu
/// is opened, so the keys a menu names run what they name; what it narrowed
/// to stays.
pub(crate) fn leave_filter(view: &mut TreeView, left: bool) -> Vec<Effect> {
    if !(left && view.filtering) {
        return Vec::new();
    }
    view.filtering = false;
    vec![Effect::FilterFocus(false)]
}

/// A finished session or terminal is seen once the focused window shows it
/// while the user is in the window.
pub(crate) fn see_completions(
    view: &mut TreeView,
    shown: Option<&Tab>,
    focused: bool,
    work: Option<&WorkSnapshot>,
) {
    let shown = match (shown, focused) {
        (Some(Tab::Session(_, id)), true) => Some(Viewed::Session(id.as_str())),
        (Some(Tab::Terminal(_, id)), true) => Some(Viewed::Terminal(*id)),
        _ => None,
    };
    let next = view.completions.updated(listed(work), shown);
    if next != view.completions {
        view.completions = next;
    }
}

/// What the host of `environment` keeps read for the tree.
pub(crate) fn sight(view: &TreeView, environment: &EnvironmentId) -> TreeSight {
    let of = |paths: &BTreeSet<(EnvironmentId, PathBuf)>| {
        paths
            .iter()
            .filter(|(e, _)| e == environment)
            .map(|(_, path)| path.clone())
            .collect()
    };
    TreeSight {
        every_worktree: view.every_worktree,
        marked: of(&view.built.marked),
        listed: of(&view.built.listed),
        github: view
            .built
            .github
            .iter()
            .filter(|(e, ..)| e == environment)
            .map(|(_, root, listing)| (root.clone(), *listing))
            .collect(),
    }
}

/// The worktree `tab`'s thing is in, as the tree lists it.
pub fn place_of_tab(tree: &Tree, terminals: &[TerminalSession], tab: &Tab) -> Option<Place> {
    let at = |worktree: &std::path::Path| Place::at(tab.environment(), worktree);
    match tab {
        Tab::Session(_, id) => nav::find(tree, id).map(|(p, _)| at(p)),
        Tab::Terminal(_, id) => terminals.iter().find(|s| s.id == *id).map(|s| at(&s.cwd)),
        Tab::Draft { worktree, .. } => Some(at(worktree)),
        Tab::Viewer(_, path) => {
            adroit_model::innermost(tree.worktrees().map(|w| &w.worktree.path), path).map(|w| at(w))
        }
        Tab::Drawn { session, .. } => nav::find(tree, session).map(|(p, _)| at(p)),
        Tab::GitHub { repository, .. } => Some(at(repository)),
    }
}

/// The title of the pull request or issue `tab` shows, as last listed.
pub fn github_title(work: Option<&WorkSnapshot>, tab: &Tab) -> Option<String> {
    match tab {
        Tab::GitHub {
            repository,
            listing,
            number,
            ..
        } => github(work, repository)
            .and_then(|r| r.item(*listing, *number))
            .map(|i| i.title.clone()),
        _ => None,
    }
}

/// Where the focused window is, while it shows something.
pub fn crumbs(showing: &Showing, work: Option<&WorkSnapshot>) -> Option<Crumbs> {
    let tab = showing.tab.as_ref()?;
    let place = showing.place.as_ref()?;
    let tree = listed(work);
    let workspace = tree
        .workspace_of(&place.worktree)
        .map(|w| (w.name.clone(), w.scratch));
    let target = match tab {
        Tab::Session(_, id) => nav::find(tree, id)
            .map(|(_, s)| s.name.clone())
            .unwrap_or_else(|| id.clone()),
        Tab::Terminal(_, n) => terminals(work)
            .iter()
            .find(|s| s.id == *n)
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        Tab::Draft { .. } => "new session".into(),
        Tab::Viewer(_, path) => path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        Tab::Drawn { width, height, .. } => format!("Image {width} × {height}"),
        Tab::GitHub { number, .. } => {
            github_title(work, tab).unwrap_or_else(|| format!("#{number}"))
        }
    };
    Some(Crumbs {
        workspace,
        worktree: place.worktree.clone(),
        target,
    })
}

/// How many sessions work now.
pub fn working(work: Option<&WorkSnapshot>) -> usize {
    listed(work)
        .sessions()
        .filter(|(_, s)| s.state == State::Working)
        .count()
}

#[cfg(test)]
mod tests;
