//! What a right click on a sidebar row, a tab or a link of the details
//! panel's overview offers: the actions its keys would run there.

use adroit_model::{EnvironmentId, Session};

use crate::actions::{Action, Group};
use crate::content::Tab;
use crate::tree::nav::{Heading, Item};

/// What a menu was opened on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Row(Item),
    /// The `+` of a row: what starts work where the row is, and nothing
    /// else its menu offers.
    Start(Item),
    Tab(Tab),
    /// Row `at` of the overview's links, and whether it is a file.
    Link {
        at: usize,
        file: bool,
    },
}

/// Where the environment of a menu's heading works, as far as its menu
/// goes: the window knows how its connection stands, the rules do not.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Reach {
    /// On this computer, with no work machine to reach.
    #[default]
    Here,
    /// On a work machine; `retryable` while the window waits to try it
    /// again.
    Machine { retryable: bool },
}

/// `action` as a right-click menu offers it, by what its row in the table
/// of actions calls it there.
fn offer(action: Action) -> (Action, &'static str) {
    let label = action.spec().row.map_or(action.spec().name, |r| r.label);
    (action, label)
}

/// The actions offered on `target`, each with what the menu calls it.
/// `session` is the session the row or tab is for, when it is for one;
/// `own` is the window's own environment, which it never removes; `reach`
/// says where the environment of a heading works.
pub fn entries(
    target: &Target,
    session: Option<&Session>,
    own: &EnvironmentId,
    reach: Reach,
) -> Vec<(Action, &'static str)> {
    let here = [Action::SessionNew, Action::ShellOpen, Action::NeovimOpen].map(offer);
    let ending = session.map(|_| offer(Action::SessionStop));
    let rename = session.map(|_| offer(Action::SessionRename));
    // The key says where a row opens, as in fern.
    let opens = [
        Action::CursorGo,
        Action::CursorSplit,
        Action::CursorVsplit,
        Action::CursorTab,
    ]
    .map(offer);
    // What is copied, and what going to a row that folds does, depend on
    // the row, so they are named here rather than in the table.
    let copy_path = (Action::CursorCopy, "Copy path");
    let copy_name = (Action::CursorCopy, "Copy name");
    let fold = (Action::CursorGo, "Fold or unfold");
    let close = offer(Action::CursorClose);
    let makes = [Action::FileNew, Action::DirectoryNew].map(offer);
    let close_tab = offer(Action::TabClose);
    let stop_terminal = offer(Action::TerminalStop);
    match target {
        Target::Row(Item::Gathered(row)) => {
            entries(&Target::Row((**row).clone()), session, own, reach)
        }
        Target::Row(Item::Session(_, _) | Item::Waiting(_, _)) => opens
            .into_iter()
            .chain(rename)
            .chain([copy_name])
            .chain(here)
            .chain([close])
            .chain(ending)
            .collect(),
        Target::Row(Item::Terminal(_, _)) => opens
            .into_iter()
            .chain([copy_name])
            .chain(here)
            .chain([close, stop_terminal])
            .collect(),
        Target::Row(Item::File { dir: false, .. }) => opens
            .into_iter()
            .chain([copy_path])
            .chain(here)
            .chain(makes)
            .chain([close])
            .collect(),
        Target::Row(Item::Worktree(_, _)) => opens
            .into_iter()
            .chain([copy_path])
            .chain(here)
            .chain([offer(Action::WorktreeClose)])
            .collect(),
        Target::Row(Item::Workspace(_, _)) => [fold, copy_path]
            .into_iter()
            .chain(here)
            .chain([offer(Action::WorkspaceRemove)])
            .collect(),
        Target::Row(Item::File { dir: true, .. }) => [fold, copy_path]
            .into_iter()
            .chain(here)
            .chain(makes)
            .collect(),
        Target::Row(Item::Heading(_, _, Heading::Files)) => {
            [fold].into_iter().chain(here).chain(makes).collect()
        }
        Target::Row(Item::Across(..) | Item::Heading(..) | Item::GitHub(..)) => {
            [fold].into_iter().chain(here).collect()
        }
        Target::Row(Item::GitHubItem(..)) => {
            [(Action::CursorGo, "Show"), offer(Action::CursorBrowse)]
                .into_iter()
                .chain(here)
                .collect()
        }
        Target::Row(Item::GitHubFailed(..)) => here.into_iter().collect(),
        Target::Row(Item::Environment(environment)) => {
            let machine = matches!(reach, Reach::Machine { .. });
            let retry = matches!(reach, Reach::Machine { retryable: true });
            here.into_iter()
                .chain([
                    offer(Action::WorkspaceAdd),
                    offer(Action::SettingsOpen),
                    offer(Action::EnvironmentRename),
                    fold,
                ])
                .chain(retry.then(|| offer(Action::EnvironmentRetry)))
                .chain(machine.then(|| offer(Action::EnvironmentPair)))
                .chain((environment != own).then(|| offer(Action::EnvironmentRemove)))
                .collect()
        }
        Target::Start(Item::Environment(_)) => here
            .into_iter()
            .chain([offer(Action::WorkspaceAdd)])
            .collect(),
        Target::Start(_) => here.into_iter().collect(),
        Target::Tab(Tab::Session(_, _)) => [close_tab]
            .into_iter()
            .chain(rename)
            .chain(ending)
            .collect(),
        Target::Tab(Tab::Terminal(_, _)) => vec![close_tab, stop_terminal],
        Target::Tab(Tab::Draft { .. }) => Vec::new(),
        Target::Tab(Tab::Viewer(_, _) | Tab::Drawn { .. } | Tab::GitHub { .. }) => vec![close_tab],
        Target::Link { file, .. } => [Action::CursorGo, Action::CursorCopy]
            .map(offer)
            .into_iter()
            .chain(file.then(|| offer(Action::CursorReveal)))
            .collect(),
    }
}

/// [`entries`] in the groups the menu draws apart, each entry in the group
/// its row in the table of actions puts it in.
pub fn groups(
    target: &Target,
    session: Option<&Session>,
    own: &EnvironmentId,
    reach: Reach,
) -> Vec<Vec<(Action, &'static str)>> {
    let group = |a: &Action| a.spec().row.map_or(Group::Go, |r| r.group);
    let mut out: Vec<Vec<(Action, &'static str)>> = Vec::new();
    for entry in entries(target, session, own, reach) {
        match out.last_mut() {
            Some(g) if g.last().is_some_and(|(a, _)| group(a) == group(&entry.0)) => g.push(entry),
            _ => out.push(vec![entry]),
        }
    }
    out
}

#[cfg(test)]
mod tests;
