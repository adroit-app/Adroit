//! The sheets of the details panel.

use std::path::PathBuf;

use adroit_claude::git::{Change, Commit, DiffLine};
use adroit_model::{Kind, Session, State};

/// What the details panel says about a session, as label and value, at
/// `now_ms` since the Unix epoch.
pub fn facts(s: &Session, now_ms: u64) -> Vec<(&'static str, String)> {
    let state = match &s.state {
        State::Waiting(why) => format!("waiting · {why}"),
        other => other.word().to_string(),
    };
    let runs = match (s.kind, s.reads_only()) {
        (Kind::Own, false) => "in adroit",
        (Kind::Own, true) => "in adroit, reading only",
        (Kind::Interactive, false) => "in a terminal",
        (Kind::Interactive, true) => "in a terminal, reading only",
    };
    let mut out = vec![("State", state), ("Runs", runs.to_string())];
    if let Some(started) = s.started_ms {
        out.push(("Started", ago(now_ms.saturating_sub(started))));
    }
    out
}

/// `ms` milliseconds back, in the largest whole unit.
fn ago(ms: u64) -> String {
    let min = ms / 60_000;
    match min {
        0 => "just now".into(),
        1..60 => format!("{min} min ago"),
        60..1440 => format!("{} h ago", min / 60),
        _ => match min / 1440 {
            1 => "1 day ago".into(),
            d => format!("{d} days ago"),
        },
    }
}

/// What the details panel shows.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sheet {
    /// The session shown, and where it works.
    Overview,
    /// The files changed in the worktree.
    Files,
    /// The worktree's recent commits.
    Log,
    /// The worktree's diff against its last commit.
    Diff,
}

impl Sheet {
    pub const ALL: [Sheet; 4] = [Sheet::Overview, Sheet::Files, Sheet::Log, Sheet::Diff];

    pub fn title(self) -> &'static str {
        match self {
            Sheet::Overview => "Overview",
            Sheet::Files => "Files",
            Sheet::Log => "Log",
            Sheet::Diff => "Diff",
        }
    }

    /// The sheet `delta` places along, wrapping around.
    pub fn step(self, delta: isize) -> Sheet {
        let at = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[crate::nav::wrap(at, delta, Self::ALL.len())]
    }

    /// Whether the sheet is read from git.
    pub fn reads_git(self) -> bool {
        self != Sheet::Overview
    }
}

/// What git said for a sheet, and which worktree it was about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitView {
    pub sheet: Sheet,
    pub worktree: PathBuf,
    pub data: GitData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitData {
    Files(Vec<Change>),
    Log(Vec<Commit>),
    Diff(Vec<DiffLine>),
    Error(String),
}

/// How many commits the log sheet shows.
pub const LOG_COMMITS: usize = 50;

/// Ask git for what a sheet shows; the overview is not git's.
pub async fn load(sheet: Sheet, worktree: PathBuf) -> Option<GitView> {
    use adroit_claude::git;
    let data = match sheet {
        Sheet::Overview => return None,
        Sheet::Files => git::changes(&worktree).await.map(GitData::Files),
        Sheet::Log => git::history(&worktree, LOG_COMMITS).await.map(GitData::Log),
        Sheet::Diff => git::diff(&worktree).await.map(GitData::Diff),
    }
    .unwrap_or_else(|e| GitData::Error(e.to_string()));
    Some(GitView {
        sheet,
        worktree,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use adroit_model::{Session, State, Wait};

    #[test]
    fn a_session_is_told_by_its_state_how_it_runs_and_when_it_started() {
        let s = Session {
            name: "demo/fix-x".into(),
            kind: Kind::Own,
            state: State::Waiting(Wait::Permission),
            started_ms: Some(1_000_000),
            ..Default::default()
        };
        let got = facts(&s, 1_000_000 + 12 * 60_000 + 5_000);
        assert_eq!(
            got,
            [
                ("State", "waiting · permission prompt".to_string()),
                ("Runs", "in adroit".to_string()),
                ("Started", "12 min ago".to_string()),
            ]
        );
    }

    #[test]
    fn a_reader_says_so_and_an_unknown_start_is_left_out() {
        let s = Session {
            name: "demo/look".into(),
            reader: true,
            kind: Kind::Interactive,
            state: State::Idle,
            ..Default::default()
        };
        let got = facts(&s, 0);
        assert_eq!(
            got,
            [
                ("State", "idle".to_string()),
                ("Runs", "in a terminal, reading only".to_string()),
            ]
        );
    }

    #[test]
    fn how_long_ago_reads_in_the_largest_whole_unit() {
        assert_eq!(ago(30_000), "just now");
        assert_eq!(ago(90_000), "1 min ago");
        assert_eq!(ago(3 * 3_600_000 + 1), "3 h ago");
        assert_eq!(ago(2 * 86_400_000), "2 days ago");
    }

    #[test]
    fn sheets_step_around_in_both_directions() {
        assert_eq!(Sheet::Overview.step(1), Sheet::Files);
        assert_eq!(Sheet::Diff.step(1), Sheet::Overview);
        assert_eq!(Sheet::Overview.step(-1), Sheet::Diff);
    }
}
