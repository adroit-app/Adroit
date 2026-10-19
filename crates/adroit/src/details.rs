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
    /// The files the working copy changes, each opening its diff.
    Changes,
    /// The recent commits, or jj's stack, each opening the files it changed.
    Log,
}

impl Sheet {
    pub const ALL: [Sheet; 3] = [Sheet::Overview, Sheet::Changes, Sheet::Log];

    pub fn title(self) -> &'static str {
        match self {
            Sheet::Overview => "Overview",
            Sheet::Changes => "Changes",
            Sheet::Log => "Log",
        }
    }

    /// The sheet `delta` places along, wrapping around.
    pub fn step(self, delta: isize) -> Sheet {
        let at = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[crate::nav::wrap(at, delta, Self::ALL.len())]
    }
}

/// Where changes are read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rev {
    /// What is being worked on: git's worktree, or jj's `@`.
    WorkingCopy,
    /// A commit, or a jj change, by its [`Commit::id`].
    Commit(String),
}

/// What a level of a sheet shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spot {
    Files(Rev),
    Diff(Rev, Change),
    Log,
}

/// What was read for a spot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing {
    Files(Vec<Change>),
    Log(Vec<Commit>),
    Diff(Vec<DiffLine>),
    Error(String),
}

impl Listing {
    /// Whether it lists rows to select and open.
    pub fn is_list(&self) -> bool {
        matches!(self, Listing::Files(_) | Listing::Log(_))
    }

    /// What tells each row apart, for a list; nothing for the rest.
    fn keys(&self) -> Vec<&str> {
        match self {
            Listing::Files(files) => files.iter().map(|f| f.path.as_str()).collect(),
            Listing::Log(commits) => commits.iter().map(|c| c.id.as_str()).collect(),
            Listing::Diff(_) | Listing::Error(_) => Vec::new(),
        }
    }
}

/// What was read for a spot, and which worktree it was about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub spot: Spot,
    pub worktree: PathBuf,
    pub listing: Listing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Opened {
    Commit(Commit),
    File(Change),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cursor {
    key: String,
    at: usize,
}

/// How far into a sheet the panel has gone: the rows opened, and the row
/// selected in what is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drill {
    sheet: Sheet,
    opened: Vec<Opened>,
    cursor: Option<Cursor>,
}

impl Drill {
    pub fn new(sheet: Sheet) -> Self {
        Drill {
            sheet,
            opened: Vec::new(),
            cursor: None,
        }
    }

    pub fn sheet(&self) -> Sheet {
        self.sheet
    }

    /// What to read for what is shown; the overview is not read.
    pub fn spot(&self) -> Option<Spot> {
        let rev = |c: &Commit| Rev::Commit(c.id.clone());
        Some(match (self.sheet, self.opened.as_slice()) {
            (Sheet::Overview, _) => return None,
            (Sheet::Changes, []) => Spot::Files(Rev::WorkingCopy),
            (Sheet::Changes, [Opened::File(f), ..]) => Spot::Diff(Rev::WorkingCopy, f.clone()),
            (Sheet::Log, []) => Spot::Log,
            (Sheet::Log, [Opened::Commit(c)]) => Spot::Files(rev(c)),
            (Sheet::Log, [Opened::Commit(c), Opened::File(f), ..]) => Spot::Diff(rev(c), f.clone()),
            _ => return None,
        })
    }

    /// What each opened row is called, outermost first.
    pub fn crumbs(&self) -> Vec<String> {
        self.opened
            .iter()
            .map(|o| match o {
                Opened::Commit(c) if c.subject.is_empty() => {
                    format!("{} (no description)", c.short)
                }
                Opened::Commit(c) => format!("{} {}", c.short, c.subject),
                Opened::File(f) => f.shown(),
            })
            .collect()
    }

    /// The row of `listing` selected.
    pub fn selected(&self, listing: &Listing) -> Option<usize> {
        let keys = listing.keys();
        if keys.is_empty() {
            return None;
        }
        Some(match &self.cursor {
            None => 0,
            Some(c) => keys
                .iter()
                .position(|k| *k == c.key)
                .unwrap_or(c.at.min(keys.len() - 1)),
        })
    }

    fn select(&mut self, listing: &Listing, at: usize) {
        self.cursor = listing.keys().get(at).map(|k| Cursor {
            key: k.to_string(),
            at,
        });
    }

    /// Move the selection `delta` rows, stopping at either end.
    pub fn step(&mut self, listing: &Listing, delta: isize) {
        let Some(at) = self.selected(listing) else {
            return;
        };
        let last = listing.keys().len() - 1;
        let to = at.saturating_add_signed(delta).min(last);
        self.select(listing, to);
    }

    pub fn first(&mut self, listing: &Listing) {
        self.select(listing, 0);
    }

    pub fn last(&mut self, listing: &Listing) {
        self.select(listing, listing.keys().len().saturating_sub(1));
    }

    /// Open the selected row; whether there was one to open.
    pub fn open(&mut self, listing: &Listing) -> bool {
        let Some(at) = self.selected(listing) else {
            return false;
        };
        let opened = match listing {
            Listing::Files(files) => Opened::File(files[at].clone()),
            Listing::Log(commits) => Opened::Commit(commits[at].clone()),
            Listing::Diff(_) | Listing::Error(_) => return false,
        };
        self.opened.push(opened);
        self.cursor = None;
        true
    }

    /// Open row `at`; whether there was one.
    pub fn open_at(&mut self, listing: &Listing, at: usize) -> bool {
        if at >= listing.keys().len() {
            return false;
        }
        self.select(listing, at);
        self.open(listing)
    }

    /// Go back out to the list the shown row was opened from, with it
    /// selected; whether there was one.
    pub fn back(&mut self) -> bool {
        let Some(opened) = self.opened.pop() else {
            return false;
        };
        let key = match opened {
            Opened::Commit(c) => c.id,
            Opened::File(f) => f.path,
        };
        // Where it was is not kept; should it be gone meanwhile, the list's
        // first row stands in.
        self.cursor = Some(Cursor { key, at: 0 });
        true
    }

    /// Take in what was read again for the spot shown: the selection follows
    /// its row, or stays where it was when the row is gone. A row opened that
    /// is gone, its revision unknown now or its file changed no more, is
    /// stepped back from; whether it was.
    pub fn refreshed(&mut self, listing: &Listing) -> bool {
        let gone = match listing {
            Listing::Error(_) => true,
            Listing::Diff(lines) => lines.is_empty(),
            Listing::Files(_) | Listing::Log(_) => false,
        };
        if gone && !self.opened.is_empty() {
            return self.back();
        }
        if self.cursor.is_some()
            && let Some(at) = self.selected(listing)
        {
            self.select(listing, at);
        }
        false
    }
}

/// How many commits the log sheet shows.
pub const LOG_COMMITS: usize = 50;

/// Read what `spot` shows of `worktree`: through jj in a jj repository,
/// through git otherwise or when jj cannot be run.
pub async fn load(spot: Spot, worktree: PathBuf) -> View {
    use adroit_claude::{Error, git, jj};
    let mut listing = None;
    if jj::is_jj(&worktree) {
        let rev = |r: &Rev| match r {
            Rev::WorkingCopy => "@".to_string(),
            Rev::Commit(id) => id.clone(),
        };
        let read = match &spot {
            Spot::Files(r) => jj::files(&worktree, &rev(r)).await.map(Listing::Files),
            Spot::Diff(r, c) => jj::file_diff(&worktree, &rev(r), c)
                .await
                .map(Listing::Diff),
            Spot::Log => jj::history(&worktree, LOG_COMMITS).await.map(Listing::Log),
        };
        // Without jj, a colocated repository still reads through git.
        if !matches!(read, Err(Error::Spawn { .. })) {
            listing = Some(read);
        }
    }
    let read = match listing {
        Some(read) => read,
        None => {
            let rev = |r: &Rev| match r {
                Rev::WorkingCopy => None,
                Rev::Commit(id) => Some(id.clone()),
            };
            match &spot {
                Spot::Files(Rev::WorkingCopy) => git::changes(&worktree).await.map(Listing::Files),
                Spot::Files(Rev::Commit(id)) => {
                    git::commit_files(&worktree, id).await.map(Listing::Files)
                }
                Spot::Diff(r, c) => git::file_diff(&worktree, rev(r).as_deref(), c)
                    .await
                    .map(Listing::Diff),
                Spot::Log => git::history(&worktree, LOG_COMMITS).await.map(Listing::Log),
            }
        }
    };
    View {
        spot,
        worktree,
        listing: read.unwrap_or_else(|e| Listing::Error(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use adroit_claude::git::DiffKind;

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
        assert_eq!(Sheet::Overview.step(1), Sheet::Changes);
        assert_eq!(Sheet::Log.step(1), Sheet::Overview);
        assert_eq!(Sheet::Overview.step(-1), Sheet::Log);
    }

    fn change(status: &str, path: &str) -> Change {
        Change {
            status: status.into(),
            path: path.into(),
            from: None,
        }
    }

    fn files(paths: &[&str]) -> Listing {
        Listing::Files(paths.iter().map(|p| change("M", p)).collect())
    }

    fn commit(id: &str, subject: &str) -> Commit {
        Commit {
            id: format!("{id}-full"),
            short: id.into(),
            subject: subject.into(),
            ..Default::default()
        }
    }

    fn log(ids: &[&str]) -> Listing {
        Listing::Log(ids.iter().map(|id| commit(id, "")).collect())
    }

    fn diff() -> Listing {
        Listing::Diff(vec![DiffLine {
            kind: DiffKind::Added,
            text: "+x".into(),
        }])
    }

    #[test]
    fn each_level_of_a_sheet_asks_for_what_it_shows() {
        let wc = Rev::WorkingCopy;
        let at = |id: &str| Rev::Commit(format!("{id}-full"));
        let mut changes = Drill::new(Sheet::Changes);
        assert_eq!(changes.spot(), Some(Spot::Files(wc.clone())));
        assert!(changes.open(&files(&["a.rs", "b.rs"])));
        assert_eq!(changes.spot(), Some(Spot::Diff(wc, change("M", "a.rs"))));

        let mut stack = Drill::new(Sheet::Log);
        assert_eq!(stack.spot(), Some(Spot::Log));
        stack.step(&log(&["x", "y"]), 1);
        assert!(stack.open(&log(&["x", "y"])));
        assert_eq!(stack.spot(), Some(Spot::Files(at("y"))));
        assert!(stack.open(&files(&["c.rs"])));
        assert_eq!(stack.spot(), Some(Spot::Diff(at("y"), change("M", "c.rs"))));

        assert_eq!(Drill::new(Sheet::Overview).spot(), None);
    }

    #[test]
    fn a_diff_opens_nothing_further_and_the_top_has_nothing_to_go_back_to() {
        let mut d = Drill::new(Sheet::Changes);
        assert!(!d.back());
        assert!(d.open(&files(&["a.rs"])));
        assert!(!d.open(&diff()));
        assert!(!d.open(&Listing::Error("gone".into())));
        assert!(d.back());
        assert!(!d.back());
    }

    #[test]
    fn going_back_selects_the_row_that_was_opened() {
        let list = log(&["x", "y", "z"]);
        let mut d = Drill::new(Sheet::Log);
        d.step(&list, 2);
        assert!(d.open(&list));
        assert!(d.open(&files(&["a.rs", "b.rs"])));
        assert!(d.back());
        assert_eq!(d.selected(&files(&["a.rs", "b.rs"])), Some(0));
        assert!(d.back());
        assert_eq!(d.selected(&list), Some(2));
    }

    #[test]
    fn the_selection_starts_on_the_first_row_and_stops_at_either_end() {
        let list = files(&["a", "b", "c"]);
        let mut d = Drill::new(Sheet::Changes);
        assert_eq!(d.selected(&list), Some(0));
        for (delta, want) in [(1, 1), (1, 2), (1, 2), (-5, 0), (-1, 0)] {
            d.step(&list, delta);
            assert_eq!(d.selected(&list), Some(want), "after {delta}");
        }
        d.last(&list);
        assert_eq!(d.selected(&list), Some(2));
        d.first(&list);
        assert_eq!(d.selected(&list), Some(0));
        assert_eq!(d.selected(&files(&[])), None);
        assert_eq!(d.selected(&diff()), None);
    }

    #[test]
    fn a_row_clicked_opens_at_once() {
        let mut d = Drill::new(Sheet::Changes);
        assert!(d.open_at(&files(&["a.rs", "b.rs"]), 1));
        assert_eq!(
            d.spot(),
            Some(Spot::Diff(Rev::WorkingCopy, change("M", "b.rs")))
        );
        assert!(!Drill::new(Sheet::Changes).open_at(&files(&["a.rs"]), 3));
    }

    #[test]
    fn a_refreshed_list_keeps_the_selected_row_or_the_place_it_was_at() {
        let mut d = Drill::new(Sheet::Changes);
        d.step(&files(&["a", "b", "c"]), 1);
        // `b` moved down: it stays selected.
        let moved = files(&["new", "a", "b", "c"]);
        assert!(!d.refreshed(&moved));
        assert_eq!(d.selected(&moved), Some(2));
        // `b` is gone: the row now where it was is selected.
        let gone = files(&["new", "a", "c"]);
        assert!(!d.refreshed(&gone));
        assert_eq!(d.selected(&gone), Some(2));
        // Rows fewer than where it was: the last one.
        let fewer = files(&["new"]);
        assert!(!d.refreshed(&fewer));
        assert_eq!(d.selected(&fewer), Some(0));
    }

    #[test]
    fn a_drilled_level_whose_item_is_gone_steps_back() {
        for (gone, why) in [
            (Listing::Error("no such revision".into()), "an error"),
            (Listing::Diff(Vec::new()), "an empty diff"),
        ] {
            let mut d = Drill::new(Sheet::Log);
            assert!(d.open(&log(&["x"])));
            assert!(d.open(&files(&["a.rs"])));
            assert!(d.refreshed(&gone), "{why}");
            assert!(matches!(d.spot(), Some(Spot::Files(_))), "{why}");
        }
    }

    #[test]
    fn an_empty_change_s_file_list_and_the_top_level_stay_put() {
        let mut d = Drill::new(Sheet::Log);
        assert!(d.open(&log(&["x"])));
        assert!(!d.refreshed(&files(&[])));
        assert!(matches!(d.spot(), Some(Spot::Files(_))));
        let mut top = Drill::new(Sheet::Changes);
        assert!(!top.refreshed(&Listing::Error("not a repository".into())));
        assert_eq!(top.spot(), Some(Spot::Files(Rev::WorkingCopy)));
    }

    #[test]
    fn the_crumbs_name_each_opened_level() {
        let mut d = Drill::new(Sheet::Log);
        assert!(d.crumbs().is_empty());
        assert!(d.open(&Listing::Log(vec![commit("wmmo", "keep the reader")])));
        let mut renamed = change("R", "new.rs");
        renamed.from = Some("old.rs".into());
        assert!(d.open(&Listing::Files(vec![renamed])));
        assert_eq!(d.crumbs(), ["wmmo keep the reader", "old.rs -> new.rs"]);
        let mut bare = Drill::new(Sheet::Log);
        assert!(bare.open(&Listing::Log(vec![commit("zzzz", "")])));
        assert_eq!(bare.crumbs(), ["zzzz (no description)"]);
    }
}
