//! The list of the conversations the agents kept: which directory's it
//! lists, and what choosing a row does.

use std::path::{Path, PathBuf};

use adroit_model::{EnvironmentId, Past};
use adroit_protocol::{PastSight, WorkSnapshot};

use super::fuzzy::{Query, best};
use super::list::ListState;
use super::{close, short};
use crate::content::{self, Tab};
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::{self, nav};

/// The most conversations the list draws at once.
const ROWS: usize = 200;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resume {
    /// The environment of what the focused window showed when the list
    /// opened, whose agents' conversations it lists.
    pub environment: EnvironmentId,
    /// The directory the focused window worked in when the list opened.
    pub dir: Option<PathBuf>,
    /// Whether every directory's conversations are listed, rather than the
    /// directory's.
    pub every: bool,
    pub list: ListState,
}

/// A conversation the list draws, with the session of this window that
/// holds it, if one does.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub past: Past,
    pub session: Option<String>,
}

impl Resume {
    /// Which conversations the host is asked to list.
    pub fn sight(&self) -> PastSight {
        PastSight {
            dir: self.dir.clone().filter(|_| !self.every),
        }
    }

    /// List every directory's conversations, or the directory's again; with
    /// no directory, every directory's stay listed.
    pub(crate) fn toggle_every(&mut self) {
        if self.dir.is_some() {
            self.every = !self.every;
            self.list.cursor = 0;
        }
    }
}

/// The conversations listed for what the list shows, those its query
/// answers to, best first; none while they are being read. Each is matched
/// by its title and the directory it ran in as drawn, `home` written as `~`.
pub fn rows(list: &Resume, home: &Path, work: Option<&WorkSnapshot>) -> Option<Vec<Row>> {
    let pasts = work?.past.of(&list.sight())?;
    let query = Query::new(&list.list.query);
    let matched = best(
        pasts.iter().filter_map(|p| {
            let text = format!("{} {}", p.title, short(&p.cwd, home));
            query.rank_named(&text, &p.title).map(|rank| (rank, p))
        }),
        ROWS,
    );
    let sessions = work.map(|w| w.tree.sessions()).unwrap_or_default();
    Some(
        matched
            .into_iter()
            .map(|p| Row {
                past: p.clone(),
                session: sessions
                    .iter()
                    .find(|s| s.agent == p.agent && s.session_id == p.id)
                    .map(|s| s.id.clone()),
            })
            .collect(),
    )
}

/// The list opened on the directory the focused window works in: the one
/// its session runs in, or else its worktree; every directory's with
/// nothing open.
pub(super) fn open(state: &ViewState) -> Resume {
    let showing = state.content.showing();
    let environment = content::shown_environment(state);
    let work = state.hosts.work(&environment);
    let session_dir = match &showing.tab {
        Some(Tab::Session(_, id)) => nav::find(tree::listed(work), id).map(|(_, s)| s.cwd.clone()),
        _ => None,
    };
    let dir = session_dir.or_else(|| showing.place.map(|p| p.worktree));
    Resume {
        environment,
        every: dir.is_none(),
        dir,
        ..Resume::default()
    }
}

/// The row `at` chosen: the session that holds its conversation shown, or
/// else the conversation resumed in a new session. The list closes either
/// way.
pub(super) fn choose(state: &mut ViewState, at: usize) -> Vec<Effect> {
    let view = &state.picker;
    let chosen = match &view.open {
        Some(super::Picking::Resume(list)) => {
            rows(list, &view.home, state.hosts.work(&list.environment))
                .and_then(|mut r| (at < r.len()).then(|| r.swap_remove(at)))
                .map(|row| (list.environment.clone(), row))
        }
        _ => None,
    };
    let Some((environment, row)) = chosen else {
        return Vec::new();
    };
    close(&mut state.picker);
    match row.session {
        Some(id) => content::show_session(state, environment, id),
        None => vec![Effect::Resume {
            environment,
            past: row.past,
        }],
    }
}

#[cfg(test)]
mod tests;
