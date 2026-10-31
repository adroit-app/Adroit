//! What is picked from a list drawn over the window: a session to go to or
//! a directory to start one in, and a past conversation to resume. One is
//! open at a time, and holds the keys while it is.

use std::path::{Path, PathBuf};

use adroit_model::Key;
use adroit_protocol::{PastSight, PickSight, WorkSnapshot};

use crate::actions::Action;
use crate::content;
use crate::effect::Effect;
use crate::keymap::{Context, Step};
use crate::state::ViewState;
use crate::tree;

mod choices;
pub mod filer;
pub mod fuzzy;
pub mod resume;
#[cfg(test)]
mod tests;

pub use choices::*;
use filer::{Filer, Row};
use resume::Resume;

/// The most sessions, and the most directories, the picker going to a
/// session lists at once.
const PICKS: usize = 40;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PickerView {
    /// The home directory, which the paths drawn write as `~`.
    pub home: PathBuf,
    pub open: Option<Picking>,
}

/// The picker open.
#[derive(Debug, Clone, PartialEq)]
pub enum Picking {
    /// Go to a session, or start one.
    Go(Box<Picker>),
    /// Resume a conversation an agent kept.
    Resume(Resume),
}

impl Picking {
    /// What is typed to narrow the list, and the row the cursor is on.
    fn list(&mut self) -> (&mut String, &mut usize) {
        match self {
            Picking::Go(p) => (&mut p.query, &mut p.cursor),
            Picking::Resume(r) => (&mut r.query, &mut r.cursor),
        }
    }

    pub fn cursor(&self) -> usize {
        match self {
            Picking::Go(p) => p.cursor,
            Picking::Resume(r) => r.cursor,
        }
    }
}

impl PickerView {
    /// What the host keeps listed for the picker going to a session.
    pub fn sight(&self) -> Option<PickSight> {
        match &self.open {
            Some(Picking::Go(p)) => Some(PickSight {
                listed: p.filer.expanded().map(Path::to_path_buf).collect(),
            }),
            _ => None,
        }
    }

    /// Which conversations the host lists while they are picked from.
    pub fn past_sight(&self) -> Option<PastSight> {
        match &self.open {
            Some(Picking::Resume(r)) => Some(r.sight()),
            _ => None,
        }
    }
}

/// What a key does to a list, whichever picker draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKey {
    Close,
    Choose,
    Step(isize),
}

fn list_key(key: &Key) -> Option<ListKey> {
    let plain = !key.ctrl && !key.alt && !key.meta;
    let ctrl = key.ctrl && !key.alt && !key.meta;
    Some(match key.key.as_str() {
        "Escape" if plain => ListKey::Close,
        "c" if ctrl => ListKey::Close,
        "Enter" => ListKey::Choose,
        "ArrowDown" | "Tab" if !key.shift => ListKey::Step(1),
        "Tab" => ListKey::Step(-1),
        "n" | "j" if ctrl => ListKey::Step(1),
        "ArrowUp" => ListKey::Step(-1),
        "p" | "k" if ctrl => ListKey::Step(-1),
        _ => return None,
    })
}

/// The child directories of `dir` as the host listed them; none while it
/// has not.
fn listed_dirs(work: Option<&WorkSnapshot>, dir: &Path) -> Option<Vec<PathBuf>> {
    let entries = work?.tree.entries.get(dir)?;
    Some(
        entries
            .iter()
            .filter(|e| e.dir)
            .map(|e| e.path.clone())
            .collect(),
    )
}

/// What the picker going to a session lists, best first for what is typed.
pub fn go_rows(picker: &Picker, home: &Path, work: Option<&WorkSnapshot>) -> Vec<Choice> {
    let offered = picker.offered.clone().unwrap_or_default();
    let directories = candidates(
        &picker.running,
        &offered.recent,
        &offered.ghq,
        &offered.listed,
    );
    let sessions = sessions(tree::listed(work));
    choices(&sessions, &directories, &picker.query, home, PICKS)
}

/// The rows of the picker's directory tree, each open directory's as the
/// host listed them.
pub fn tree_rows(picker: &Picker, work: Option<&WorkSnapshot>) -> Vec<Row> {
    picker
        .filer
        .rows(|dir| listed_dirs(work, dir).unwrap_or_default())
}

/// Open the picker going to a session on the sessions and the directories
/// they run in; what else is offered follows once the host lists it. The
/// home directory starts open: nearly every pick in the tree begins inside
/// it.
pub(crate) fn open(state: &mut ViewState) -> Vec<Effect> {
    let work = state.host.work.as_ref();
    let running = tree::listed(work)
        .worktrees()
        .filter(|w| !w.sessions.is_empty())
        .map(|w| w.worktree.path.clone())
        .collect();
    let mut picker = Picker {
        running,
        ..Picker::new(Filer::new([state.picker.home.clone()]))
    };
    enter(&mut picker, work);
    state.picker.open = Some(Picking::Go(Box::new(picker)));
    Vec::new()
}

/// Open the list of past conversations.
pub(crate) fn open_resume(state: &mut ViewState) -> Vec<Effect> {
    state.picker.open = Some(Picking::Resume(resume::open(state)));
    Vec::new()
}

pub(crate) fn close(view: &mut PickerView) -> Vec<Effect> {
    view.open = None;
    Vec::new()
}

/// Narrow the list by `text` from now on, from the first row when it
/// changed.
pub(crate) fn query(view: &mut PickerView, text: String) -> Vec<Effect> {
    if let Some(open) = &mut view.open {
        let (query, cursor) = open.list();
        if *query != text {
            *query = text;
            *cursor = 0;
        }
    }
    Vec::new()
}

/// List every directory's conversations, or the directory's again.
pub(crate) fn every(view: &mut PickerView) -> Vec<Effect> {
    if let Some(Picking::Resume(r)) = &mut view.open {
        r.toggle_every();
    }
    Vec::new()
}

/// A key the picker's query field does not type with, or any key in its
/// directory tree.
pub(crate) fn press(state: &mut ViewState, key: &Key) -> Vec<Effect> {
    let ctrl = key.ctrl && !key.alt && !key.meta;
    let work = state.host.work.as_ref();
    let list = list_key(key);
    if list == Some(ListKey::Close) {
        return close(&mut state.picker);
    }
    let Some(open) = &mut state.picker.open else {
        return Vec::new();
    };
    match open {
        Picking::Go(p) if ctrl && key.key == "f" => {
            p.toggle_mode();
            return Vec::new();
        }
        Picking::Go(p) if p.mode == Mode::Tree => {
            let action = match state.chrome.keymap.resolve(&[Context::Filer], &[], key) {
                Step::Run(action) => action,
                Step::Wait | Step::Nothing => return Vec::new(),
            };
            return match tree_key(p, &action, work) {
                Some(dir) => start_in(state, dir),
                None => Vec::new(),
            };
        }
        Picking::Resume(r) if ctrl && key.key == "a" => {
            r.toggle_every();
            return Vec::new();
        }
        _ => {}
    }
    match list {
        Some(ListKey::Choose) => {
            let at = open.cursor();
            choose(state, at)
        }
        Some(ListKey::Step(delta)) => {
            let len = rows_len(&state.picker, work);
            if let Some(open) = &mut state.picker.open {
                let (_, cursor) = open.list();
                *cursor = step(*cursor, len, delta);
            }
            Vec::new()
        }
        Some(ListKey::Close) | None => Vec::new(),
    }
}

fn rows_len(view: &PickerView, work: Option<&WorkSnapshot>) -> usize {
    match &view.open {
        Some(Picking::Go(p)) => go_rows(p, &view.home, work).len(),
        Some(Picking::Resume(r)) => resume::rows(r, &view.home, work).map_or(0, |r| r.len()),
        None => 0,
    }
}

/// The filer's action in the directory tree; the directory chosen, when
/// one is.
fn tree_key(picker: &mut Picker, action: &Action, work: Option<&WorkSnapshot>) -> Option<PathBuf> {
    let rows = tree_rows(picker, work);
    let filer = &mut picker.filer;
    match action {
        Action::FilerDown => filer.down(&rows),
        Action::FilerUp => filer.up(&rows),
        Action::FilerInto => enter(picker, work),
        Action::FilerOut => filer.close(&rows),
        Action::FilerHidden => filer.toggle_hidden(),
        Action::FilerOpen => return filer.chosen(),
        _ => {}
    }
    None
}

/// Open the directory under the tree's cursor and move onto its first
/// child; into one the host has not listed yet, once it has.
fn enter(picker: &mut Picker, work: Option<&WorkSnapshot>) {
    let Some(at) = picker.filer.cursor().cloned() else {
        return;
    };
    let listed = listed_dirs(work, &at.path);
    picker.filer.open(|_| listed.clone().unwrap_or_default());
    picker.entering = listed.is_none().then_some(at);
}

/// The row `at` chosen, by its place among those drawn.
pub(crate) fn choose(state: &mut ViewState, at: usize) -> Vec<Effect> {
    let work = state.host.work.as_ref();
    let chosen = match &state.picker.open {
        Some(Picking::Go(p)) => go_rows(p, &state.picker.home, work).into_iter().nth(at),
        Some(Picking::Resume(_)) => return resume::choose(state, at),
        None => None,
    };
    match chosen {
        Some(Choice::Scratch) => {
            let scratch = state.content.scratch().to_path_buf();
            start_in(state, scratch)
        }
        Some(Choice::Directory(c)) => start_in(state, c.path),
        Some(Choice::Session(s)) => {
            close(&mut state.picker);
            content::show_session(state, s.id)
        }
        None => Vec::new(),
    }
}

/// A directory chosen: a new session's draft there, its first prompt to
/// write.
pub(crate) fn start_in(state: &mut ViewState, dir: PathBuf) -> Vec<Effect> {
    close(&mut state.picker);
    content::draft(
        &mut state.content,
        state.host.environment.clone(),
        dir,
        true,
    )
}

/// What the host offered taken into the list and the tree: the cursor stays
/// on the row it was on, not on whatever comes to sit at its place, and the
/// tree lists the roots offered after those it has, what is open in it
/// staying open.
pub(crate) fn take_offers(view: &mut PickerView, work: Option<&WorkSnapshot>) {
    let PickerView { home, open } = view;
    let Some(Picking::Go(picker)) = open else {
        return;
    };
    let offered = work.and_then(|w| w.pick.offers.as_ref());
    if picker.offered.as_ref() == offered {
        return;
    }
    let on = go_rows(picker, home, work).into_iter().nth(picker.cursor);
    picker.offered = offered.cloned();
    let rows = go_rows(picker, home, work);
    picker.cursor = on
        .and_then(|on| rows.iter().position(|c| *c == on))
        .unwrap_or(0);
    if let Some(offered) = offered {
        picker.filer.add_roots(offered.roots.iter().cloned());
    }
}

/// A directory opened before the host listed it is moved into once it
/// lists it, while the cursor is still on it.
pub(crate) fn take_listed(view: &mut PickerView, work: Option<&WorkSnapshot>) {
    let Some(Picking::Go(picker)) = &mut view.open else {
        return;
    };
    let Some(at) = picker.entering.clone() else {
        return;
    };
    let Some(listed) = listed_dirs(work, &at.path) else {
        return;
    };
    picker.entering = None;
    if picker.filer.cursor() == Some(&at) {
        picker.filer.open(|_| listed.clone());
    }
}
