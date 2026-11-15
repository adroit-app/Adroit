//! The banner over the window: what it says of each environment's daemon the
//! window cannot work with, and what it offers to bring back of what the
//! terminal host took.

use std::collections::{BTreeMap, BTreeSet};

use adroit_model::{BroughtBack, EnvironmentId, Interrupted};

use crate::chrome::status_line;
use crate::effect::Effect;
use crate::host::Hosts;
use crate::picker;
use crate::settings::{RestoreSessions, Settings};
use crate::state::ViewState;

/// What the banner says of each environment's daemon the window cannot
/// work with, and those put away.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Banner {
    notes: BTreeMap<EnvironmentId, String>,
    closed: BTreeSet<EnvironmentId>,
    /// By the environment, what the terminal host took with it that the
    /// banner was put away on.
    later: BTreeMap<EnvironmentId, BTreeSet<String>>,
}

impl Banner {
    /// The environment whose daemon the banner speaks of while it is shown,
    /// and what it says.
    pub fn shown(&self) -> Option<(&EnvironmentId, &str)> {
        self.notes
            .iter()
            .find(|(environment, _)| !self.closed.contains(*environment))
            .map(|(environment, note)| (environment, note.as_str()))
    }
}

/// How `environment`'s daemon stands, as the banner says it; one put away
/// stays away until that daemon is restarted.
pub(crate) fn daemon_standing(
    state: &mut ViewState,
    environment: EnvironmentId,
    note: Option<String>,
) -> Vec<Effect> {
    let notes = &mut state.overlays.banner.notes;
    match note {
        Some(note) => notes.insert(environment, note),
        None => notes.remove(&environment),
    };
    Vec::new()
}

pub(crate) fn daemon_restarted(
    state: &mut ViewState,
    environment: EnvironmentId,
    note: Option<String>,
) -> Vec<Effect> {
    state.overlays.banner.closed.remove(&environment);
    daemon_standing(state, environment, note)
}

/// Forget what the banner says of `environment`'s daemon, one the window
/// no longer shows.
pub(crate) fn forget_daemon(state: &mut ViewState, environment: &EnvironmentId) {
    let banner = &mut state.overlays.banner;
    banner.notes.remove(environment);
    banner.closed.remove(environment);
}

/// The environment whose terminal host took what the banner offers to
/// bring back, and what it offers: while `settings` ask, what `hosts` list
/// as waiting that `banner` was not put away on.
pub fn interrupted_banner<'a>(
    settings: &Settings,
    hosts: &'a Hosts,
    banner: &Banner,
) -> Option<(EnvironmentId, Vec<&'a Interrupted>)> {
    if settings.restore_sessions != RestoreSessions::Ask {
        return None;
    }
    hosts.each().find_map(|(environment, _)| {
        let waiting = picker::interrupted::waiting(hosts, environment);
        let later = banner.later.get(environment);
        let new = waiting
            .iter()
            .any(|i| later.is_none_or(|later| !later.contains(&i.id)));
        new.then(|| (environment.clone(), waiting.iter().collect()))
    })
}

fn banner_of(state: &ViewState) -> Option<(EnvironmentId, Vec<String>)> {
    interrupted_banner(&state.config.settings, &state.hosts, &state.overlays.banner).map(
        |(environment, waiting)| {
            (
                environment,
                waiting.into_iter().map(|i| i.id.clone()).collect(),
            )
        },
    )
}

/// Bring back, or let go of, everything the banner offers.
pub(crate) fn bring_back_all(state: &mut ViewState, let_go: bool) -> Vec<Effect> {
    let Some((environment, ids)) = banner_of(state) else {
        return Vec::new();
    };
    vec![if let_go {
        Effect::LetGo { environment, ids }
    } else {
        Effect::BringBack { environment, ids }
    }]
}

/// Put the banner offering what the terminal host took away, until it takes
/// more.
pub(crate) fn interrupted_later(state: &mut ViewState) -> Vec<Effect> {
    if let Some((environment, ids)) = banner_of(state) {
        state
            .overlays
            .banner
            .later
            .insert(environment, ids.into_iter().collect());
    }
    Vec::new()
}

/// What came of bringing back what the terminal host took: each refused,
/// with why.
pub(crate) fn brought_back(state: &mut ViewState, outcomes: &[BroughtBack]) -> Vec<Effect> {
    let refused: Vec<String> = outcomes
        .iter()
        .filter_map(|o| {
            o.refused
                .as_ref()
                .map(|why| format!("{} did not come back: {why}", o.name))
        })
        .collect();
    if refused.is_empty() {
        return Vec::new();
    }
    status_line::notice(&mut state.chrome.status, refused.join("; "))
}

/// Put away the banner shown, leaving another environment's to show.
pub(crate) fn close_banner(state: &mut ViewState) -> Vec<Effect> {
    let banner = &mut state.overlays.banner;
    if let Some(environment) = banner.shown().map(|(environment, _)| environment.clone()) {
        banner.closed.insert(environment);
    }
    Vec::new()
}
