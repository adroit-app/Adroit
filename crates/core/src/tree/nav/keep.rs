//! Keeping the selection on the row of what is shown as the rows and the
//! tabs change.

use adroit_model::{EnvironmentId, Tree};

use super::rows::listed_under;
use super::{Heading, Item, sessions_in};
use crate::content::Tab;
use crate::tree::Place;

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
    if let Some(row) = rows_of_same(selected).find(|row| list.contains(row)) {
        return Some(row);
    }
    row_of(list, tree, selected.environment(), shown, place)
}

/// The other rows of what `item` stands for: a session's under its
/// repository's heading, under its worktree's and in the inbox, a terminal's
/// under its repository's heading and under its worktree's.
fn rows_of_same(item: &Item) -> impl Iterator<Item = Item> + '_ {
    let own = match item.own() {
        Item::Waiting(environment, id) | Item::Session(environment, id) => {
            Some(Item::Session(environment.clone(), id.clone()))
        }
        terminal @ Item::Terminal(..) => Some(terminal.clone()),
        _ => None,
    };
    let waiting = match &own {
        Some(Item::Session(environment, id)) => {
            Some(Item::Waiting(environment.clone(), id.clone()))
        }
        _ => None,
    };
    own.clone()
        .map(|own| Item::Gathered(Box::new(own)))
        .into_iter()
        .chain(own)
        .chain(waiting)
        .filter(move |row| row != item)
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
            let own = Item::Session(environment.clone(), id.clone());
            vec![Item::Gathered(Box::new(own.clone())), own]
        }
        (Some(Tab::Terminal(environment, id)), None) => {
            let own = Item::Terminal(environment.clone(), *id);
            vec![Item::Gathered(Box::new(own.clone())), own]
        }
        _ => Vec::new(),
    };
    rows.extend(heading_of(tree, shown).into_iter().flatten());
    rows.push(Item::Worktree(environment.clone(), place.worktree.clone()));
    if let Some(ws) = tree.workspace_of(&place.worktree) {
        rows.push(Item::Workspace(environment.clone(), ws.root.clone()));
    }
    rows.into_iter().find(|item| list.contains(item))
}

/// The headings the rows of what `shown` shows sit under: its repository's,
/// then its worktree's.
fn heading_of(tree: &Tree, shown: Option<&Tab>) -> Option<[Item; 2]> {
    let (environment, ws, w, heading) = match shown? {
        Tab::Session(environment, id) => {
            let (ws, w, _) = tree.find(id)?;
            (environment, ws, w, Heading::Sessions)
        }
        Tab::Terminal(environment, id) => {
            let (ws, w, t) = tree.terminal(*id)?;
            (environment, ws, w, listed_under(t)?)
        }
        _ => return None,
    };
    Some([
        Item::Across(environment.clone(), ws.root.clone(), heading),
        Item::Heading(environment.clone(), w.worktree.path.clone(), heading),
    ])
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
