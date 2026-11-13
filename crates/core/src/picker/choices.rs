//! The picker that goes to a session or starts one: one list of the sessions,
//! then of the directories to start one in, narrowed by a fuzzy query, or, for
//! a directory the list does not offer, a directory tree.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use adroit_model::{EnvironmentId, Session, Tree};
use adroit_protocol::Offers;

use super::filer::{Filer, Row};
use super::fuzzy::{Query, best};
use crate::attention::{self, Changes};
use crate::completion::Completions;

/// One row of the list as drawn: the heading of the group it starts, what
/// the query marks, where it is, and a badge beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub heading: Option<&'static str>,
    pub name: String,
    pub place: Option<String>,
    pub badge: String,
    pub mark: Mark,
}

/// What a row is marked as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Scratch,
    Session,
    /// A path, drawn in a fixed-width face.
    Directory,
}

/// What the scratch row says.
pub const SCRATCH: &str = "New scratch session";

/// What a row of the list offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// A session to start in the scratch folder.
    Scratch,
    /// A session to open.
    Session(SessionOffer),
    /// A directory to start a session in.
    Directory(Candidate),
}

impl Choice {
    /// Whether `other` is this row, however it now stands.
    pub fn same_row(&self, other: &Choice) -> bool {
        match (self, other) {
            (Choice::Scratch, Choice::Scratch) => true,
            (Choice::Session(a), Choice::Session(b)) => {
                a.environment == b.environment && a.id == b.id
            }
            (Choice::Directory(a), Choice::Directory(b)) => {
                a.environment == b.environment && a.path == b.path
            }
            _ => false,
        }
    }
}

/// Where a candidate came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The tree lists it.
    Tree,
    /// Claude Code has been used there.
    Recent,
    /// `ghq list` names it.
    Ghq,
    /// A folder in one of the roots the settings list.
    Listed,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Tree => "tree",
            Source::Recent => "recent",
            Source::Ghq => "ghq",
            Source::Listed => "listed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The environment the directory is in.
    pub environment: EnvironmentId,
    pub path: PathBuf,
    pub source: Source,
}

/// A session the picker offers, with where it runs and how it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOffer {
    /// The environment the session runs in.
    pub environment: EnvironmentId,
    pub id: String,
    pub name: String,
    /// The workspace, and the worktree when the sidebar shows one.
    pub place: String,
    /// The state as the sidebar shows it: a finished session the user has
    /// seen stands idle.
    pub state: &'static str,
    /// When the window saw its state last change, as [`Changes`] counts.
    pub changed: u64,
}

/// What was typed, cut into the states it names and the rest: a term `@`
/// followed by a state's word, or the start of one, keeps the sessions in
/// that state, and the rest is matched as fzf would.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Narrowing {
    /// What is matched against the rows, the state terms taken out.
    pub text: String,
    /// The states the sessions kept stand in; [`None`] when no term names
    /// one, and every row is kept.
    pub states: Option<Vec<&'static str>>,
}

impl Narrowing {
    pub fn new(query: &str) -> Self {
        let (named, rest): (Vec<&str>, Vec<&str>) =
            query.split_whitespace().partition(|t| t.starts_with('@'));
        let states = (!named.is_empty()).then(|| {
            let mut states: Vec<&'static str> = named
                .iter()
                .flat_map(|t| attention::named(&t[1..]))
                .collect();
            states.sort_by_key(|w| attention::rank(w));
            states.dedup();
            states
        });
        Self {
            text: rest.join(" "),
            states,
        }
    }

    fn keeps(&self, state: &str) -> bool {
        self.states.as_ref().is_none_or(|s| s.contains(&state))
    }
}

/// Every session of `environment`, listed as `tree`, in the sidebar's
/// order, each standing as `completions` and `changes` have it.
pub fn sessions(
    environment: &EnvironmentId,
    tree: &Tree,
    completions: &Completions,
    changes: &Changes,
) -> Vec<SessionOffer> {
    let offer = |s: &Session, place: String| SessionOffer {
        environment: environment.clone(),
        id: s.id.clone(),
        name: s.name.clone(),
        place,
        state: completions.state(s).word(),
        changed: changes.last(environment, &s.id),
    };
    tree.workspaces
        .iter()
        .flat_map(|ws| {
            ws.worktrees.iter().flat_map(move |w| {
                let place = if ws.flat() {
                    ws.name.clone()
                } else {
                    let label = w.worktree.branch.clone().unwrap_or_else(|| {
                        w.worktree
                            .path
                            .file_name()
                            .map(|f| f.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    });
                    format!("{} › {label}", ws.name)
                };
                w.sessions.iter().map(move |s| offer(s, place.clone()))
            })
        })
        .collect()
}

/// The best `cap` sessions for `query`, best first, each matched by its name
/// and where it runs, its name counting for more. With nothing typed but
/// states, the first `cap` by how much they need the user: by
/// [`attention::ORDER`], then the latest to change first, then as given.
pub fn matching_sessions(offers: &[SessionOffer], query: &str, cap: usize) -> Vec<SessionOffer> {
    let narrowing = Narrowing::new(query);
    let query = Query::new(&narrowing.text);
    let kept = offers.iter().filter(|s| narrowing.keeps(s.state));
    if query.is_empty() {
        let mut kept: Vec<SessionOffer> = kept.cloned().collect();
        kept.sort_by_key(|s| (attention::rank(s.state), Reverse(s.changed)));
        kept.truncate(cap);
        return kept;
    }
    best(
        kept.filter_map(|s| {
            let text = format!("{} {}", s.name, s.place);
            query
                .rank_named(&text, &s.name)
                .map(|rank| (rank, s.clone()))
        }),
        cap,
    )
}

/// The sources of `environment` as one list, in the order given, each path
/// once: the first source that offers a path is the one it is listed under.
pub fn candidates(
    environment: &EnvironmentId,
    running: &[PathBuf],
    recent: &[PathBuf],
    ghq: &[PathBuf],
    listed: &[PathBuf],
) -> Vec<Candidate> {
    let mut seen = HashSet::new();
    [
        (running, Source::Tree),
        (recent, Source::Recent),
        (ghq, Source::Ghq),
        (listed, Source::Listed),
    ]
    .into_iter()
    .flat_map(|(paths, source)| paths.iter().map(move |path| (path, source)))
    .filter(|(path, _)| seen.insert(*path))
    .map(|(path, source)| Candidate {
        environment: environment.clone(),
        path: path.clone(),
        source,
    })
    .collect()
}

/// The best `cap` candidates for `query`, best first; with nothing typed, the
/// first `cap` as given.
///
/// Each is matched as it is drawn, `home` written as `~`: the absolute path
/// would let the home directory's own name answer for every row, and a `~`
/// typed answer for none.
pub fn matching(candidates: &[Candidate], query: &str, home: &Path, cap: usize) -> Vec<Candidate> {
    let query = Query::new(query);
    best(
        candidates.iter().filter_map(|c| {
            let shown = PathBuf::from(short(&c.path, home));
            query.rank_path(&shown).map(|rank| (rank, c.clone()))
        }),
        cap,
    )
}

/// The scratch row while `query` answers to it, the sessions for `query`,
/// then the directories, each group ranked among its own and cut at `cap`: a
/// directory never comes between sessions. A query that names a state offers
/// only sessions, since nothing else stands in one.
pub fn choices(
    sessions: &[SessionOffer],
    directories: &[Candidate],
    query: &str,
    home: &Path,
    cap: usize,
) -> Vec<Choice> {
    let sessions = matching_sessions(sessions, query, cap).into_iter();
    let narrowing = Narrowing::new(query);
    if narrowing.states.is_some() {
        return sessions.map(Choice::Session).collect();
    }
    let scratch = Query::new(query)
        .rank_named(SCRATCH, SCRATCH)
        .map(|_| Choice::Scratch);
    let directories = matching(directories, query, home, cap).into_iter();
    scratch
        .into_iter()
        .chain(sessions.map(Choice::Session))
        .chain(directories.map(Choice::Directory))
        .collect()
}

/// The rows `choices` are drawn as, the first of each group under its heading.
pub fn lines(choices: &[Choice], home: &Path) -> Vec<Line> {
    labelled_lines(choices, home, |_| None)
}

/// The rows `choices` are drawn as, each session and directory beside the
/// name `label` gives its environment, if it gives one.
pub fn labelled_lines(
    choices: &[Choice],
    home: &Path,
    label: impl Fn(&EnvironmentId) -> Option<String>,
) -> Vec<Line> {
    let mut last = None;
    choices
        .iter()
        .map(|c| {
            let (group, line) = match c {
                Choice::Scratch => (
                    None,
                    Line {
                        heading: None,
                        name: SCRATCH.into(),
                        place: None,
                        badge: String::new(),
                        mark: Mark::Scratch,
                    },
                ),
                Choice::Session(s) => (
                    Some("Sessions"),
                    Line {
                        heading: None,
                        name: s.name.clone(),
                        place: Some(match label(&s.environment) {
                            Some(environment) => format!("{environment} · {}", s.place),
                            None => s.place.clone(),
                        }),
                        badge: s.state.into(),
                        mark: Mark::Session,
                    },
                ),
                Choice::Directory(d) => (
                    Some("Start in"),
                    Line {
                        heading: None,
                        name: short(&d.path, home),
                        place: label(&d.environment),
                        badge: d.source.label().into(),
                        mark: Mark::Directory,
                    },
                ),
            };
            let heading = group.filter(|_| last != Some(group));
            last = Some(group);
            Line { heading, ..line }
        })
        .collect()
}

/// The cursor moved by `delta` over `len` rows, wrapping at either end.
pub fn step(cursor: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    crate::tree::nav::wrap(cursor, delta, len)
}

pub use adroit_model::paths::tilde as short;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    List,
    Tree,
}

/// What choosing a row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Purpose {
    /// Go to the session, or start one in the directory.
    #[default]
    Go,
    /// Add the directory to the tree; only directories are offered.
    Add,
}

/// What the picker holds while it is open.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Picker {
    pub query: String,
    pub cursor: usize,
    pub mode: Mode,
    pub purpose: Purpose,
    pub filer: Filer,
    /// What was typed, where the cursor was and the row it was on when the
    /// rows were last drawn.
    pub(crate) held: Option<(String, usize, Choice)>,
    /// The directories the tree listed when the picker opened, by the
    /// environment they are in.
    pub running: BTreeMap<EnvironmentId, Vec<PathBuf>>,
    /// What each environment's host offered, as the list ranks it; none
    /// from a host until it is listed.
    pub offered: BTreeMap<EnvironmentId, Offers>,
    /// The directory opened before it was listed, which the cursor moves
    /// into once it is.
    pub(crate) entering: Option<Row>,
    /// The environment the directory tree browses, and the scratch row
    /// starts in.
    pub environment: EnvironmentId,
    /// The home directory of that environment's host, where the tree starts.
    pub home: PathBuf,
}

impl Picker {
    pub fn new(filer: Filer) -> Self {
        Self {
            filer,
            ..Self::default()
        }
    }

    /// Typing narrows the list anew, so the cursor goes back to the best row.
    /// Narrow by `query` from now on, from the first row when it changed.
    pub fn set_query(&mut self, query: &str) {
        if self.query != query {
            self.query = query.to_string();
            self.cursor = 0;
        }
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            Mode::List => Mode::Tree,
            Mode::Tree => Mode::List,
        };
    }
}

#[cfg(test)]
mod tests {
    use adroit_model::testing::{self, session};
    use adroit_model::{State, Wait};

    use super::*;
    use crate::completion::Viewed;

    fn paths(ps: &[&str]) -> Vec<PathBuf> {
        ps.iter().map(PathBuf::from).collect()
    }

    const HOME: &str = "/Users/me";

    fn matching(cs: &[Candidate], query: &str, cap: usize) -> Vec<Candidate> {
        super::matching(cs, query, Path::new(HOME), cap)
    }

    fn listed(cs: &[Candidate]) -> Vec<(&str, Source)> {
        cs.iter()
            .map(|c| (c.path.to_str().unwrap(), c.source))
            .collect()
    }

    #[test]
    fn the_sources_are_listed_running_then_recent_then_ghq_each_in_its_own_order() {
        let cs = candidates(
            &EnvironmentId::default(),
            &paths(&["/r2", "/r1"]),
            &paths(&["/c"]),
            &paths(&["/g2", "/g1"]),
            &[],
        );
        assert_eq!(
            listed(&cs),
            vec![
                ("/r2", Source::Tree),
                ("/r1", Source::Tree),
                ("/c", Source::Recent),
                ("/g2", Source::Ghq),
                ("/g1", Source::Ghq),
            ]
        );
    }

    #[test]
    fn a_path_offered_twice_is_listed_once_under_the_first_source_that_offers_it() {
        let cs = candidates(
            &EnvironmentId::default(),
            &paths(&["/a"]),
            &paths(&["/b", "/a", "/b"]),
            &paths(&["/a", "/b", "/g"]),
            &[],
        );
        assert_eq!(
            listed(&cs),
            vec![
                ("/a", Source::Tree),
                ("/b", Source::Recent),
                ("/g", Source::Ghq),
            ]
        );
    }

    #[test]
    fn nothing_typed_keeps_the_list_in_order_up_to_the_cap() {
        let cs = candidates(
            &EnvironmentId::default(),
            &[],
            &paths(&["/z/long/path", "/a", "/m"]),
            &[],
            &[],
        );
        assert_eq!(
            listed(&matching(&cs, "", 2)),
            vec![("/z/long/path", Source::Recent), ("/a", Source::Recent)]
        );
    }

    #[test]
    fn a_query_keeps_what_matches_best_first() {
        let cs = candidates(
            &EnvironmentId::default(),
            &[],
            &paths(&["/w/adroit/docs", "/w/arto", "/w/src/adroit"]),
            &[],
            &[],
        );
        let found = matching(&cs, "adroit", 10);
        assert_eq!(
            listed(&found),
            vec![
                ("/w/src/adroit", Source::Recent),
                ("/w/adroit/docs", Source::Recent),
            ]
        );
    }

    #[test]
    fn folders_under_the_listed_roots_come_last_and_once() {
        let cs = candidates(
            &EnvironmentId::default(),
            &paths(&["/a"]),
            &[],
            &paths(&["/g"]),
            &paths(&["/g", "/l"]),
        );
        assert_eq!(
            cs.iter()
                .map(|c| (c.path.to_str().unwrap(), c.source))
                .collect::<Vec<_>>(),
            vec![
                ("/a", Source::Tree),
                ("/g", Source::Ghq),
                ("/l", Source::Listed)
            ]
        );
    }

    #[test]
    fn candidates_that_answer_equally_keep_the_order_they_were_listed_in() {
        let cs = candidates(
            &EnvironmentId::default(),
            &paths(&["/b/x"]),
            &paths(&["/a/x"]),
            &[],
            &[],
        );
        assert_eq!(
            listed(&matching(&cs, "x", 10)),
            vec![("/b/x", Source::Tree), ("/a/x", Source::Recent)]
        );
    }

    #[test]
    fn a_query_is_matched_against_the_path_as_drawn_with_home_as_a_tilde() {
        let cs = candidates(
            &EnvironmentId::default(),
            &[],
            &paths(&["/Users/me/src/adroit", "/opt/adroit"]),
            &[],
            &[],
        );
        assert_eq!(
            listed(&matching(&cs, "^~/src", 10)),
            vec![("/Users/me/src/adroit", Source::Recent)]
        );
        // The home directory's own name is not in what is drawn.
        assert!(matching(&cs, "me/", 10).is_empty());
    }

    #[test]
    fn the_cursor_wraps_at_either_end() {
        assert_eq!(step(0, 3, 1), 1);
        assert_eq!(step(2, 3, 1), 0);
        assert_eq!(step(0, 3, -1), 2);
        assert_eq!(step(1, 3, -1), 0);
        assert_eq!(step(0, 0, 1), 0);
        assert_eq!(step(0, 0, -1), 0);
    }

    #[test]
    fn typing_narrows_anew_and_puts_the_cursor_back_on_the_first_row() {
        let mut p = Picker {
            cursor: 3,
            ..Picker::default()
        };
        p.set_query("adr");
        assert_eq!(p.query, "adr");
        assert_eq!(p.cursor, 0);
    }

    #[test]
    fn the_same_query_again_keeps_the_cursor_where_it_is() {
        let mut p = Picker::default();
        p.set_query("aあ");
        p.cursor = 2;
        p.set_query("aあ");
        assert_eq!(p.cursor, 2);
    }

    #[test]
    fn the_picker_opens_on_the_list_and_toggles_to_the_tree_and_back() {
        let mut p = Picker::new(Filer::new(paths(&["/w"])));
        assert_eq!(p.mode, Mode::List);
        p.toggle_mode();
        assert_eq!(p.mode, Mode::Tree);
        p.toggle_mode();
        assert_eq!(p.mode, Mode::List);
    }

    const OAUTH: &str = "/w/api/.claude/worktrees/oauth";

    /// `api` runs a session in its main checkout and one in a worktree; `tmp`
    /// is outside git.
    fn tree() -> Tree {
        testing::tree()
            .session(
                session("a")
                    .name("fix login")
                    .cwd("/w/api")
                    .state(State::Working),
            )
            .session(
                session("o")
                    .name("add tokens")
                    .cwd(OAUTH)
                    .state(State::Waiting(Wait::Input)),
            )
            .session(session("z").name("scratch").cwd("/tmp"))
            .checkout("/w/api", "/w/api", "main")
            .checkout(OAUTH, "/w/api", "oauth")
            .found("/tmp", None)
            .build()
    }

    fn offered(found: &[SessionOffer]) -> Vec<(&str, &str, &str)> {
        found
            .iter()
            .map(|s| (s.id.as_str(), s.place.as_str(), s.state))
            .collect()
    }

    #[test]
    fn every_session_is_offered_in_the_sidebar_s_order_with_where_it_runs_and_how_it_stands() {
        assert_eq!(
            offered(&sessions(
                &EnvironmentId::default(),
                &tree(),
                &Completions::default(),
                &Changes::default()
            )),
            vec![
                ("a", "api › main", "working"),
                ("o", "api › oauth", "waiting"),
                ("z", "tmp", "idle"),
            ]
        );
    }

    #[test]
    fn a_session_is_found_by_its_name_or_by_where_it_runs() {
        let all = sessions(
            &EnvironmentId::default(),
            &tree(),
            &Completions::default(),
            &Changes::default(),
        );
        let ids = |q: &str| -> Vec<String> {
            matching_sessions(&all, q, 10)
                .into_iter()
                .map(|s| s.id)
                .collect()
        };
        assert_eq!(ids("login"), ["a"]);
        assert_eq!(ids("oauth"), ["o"]);
        assert_eq!(ids("api"), ["a", "o"]);
        assert_eq!(ids(""), ["o", "a", "z"], "the one waiting first");
        assert!(ids("nothing").is_empty());
    }

    #[test]
    fn a_session_named_by_the_query_comes_before_one_that_only_runs_where_it_says() {
        let t = testing::tree()
            .session(session("x").name("docs").cwd("/w/scratch"))
            .session(session("s").name("scratch").cwd("/w/docs"))
            .build();
        let ids: Vec<String> = matching_sessions(
            &sessions(
                &EnvironmentId::default(),
                &t,
                &Completions::default(),
                &Changes::default(),
            ),
            "scratch",
            10,
        )
        .into_iter()
        .map(|s| s.id)
        .collect();
        assert_eq!(ids, ["s", "x"]);
    }

    fn dirs(ps: &[&str]) -> Vec<Candidate> {
        candidates(&EnvironmentId::default(), &[], &paths(ps), &[], &[])
    }

    fn chosen(found: &[Choice]) -> Vec<String> {
        found
            .iter()
            .map(|c| match c {
                Choice::Scratch => "scratch".into(),
                Choice::Session(s) => s.id.clone(),
                Choice::Directory(d) => d.path.display().to_string(),
            })
            .collect()
    }

    #[test]
    fn the_sessions_come_before_the_directories_each_ranked_among_its_own() {
        let all = sessions(
            &EnvironmentId::default(),
            &tree(),
            &Completions::default(),
            &Changes::default(),
        );
        let ds = dirs(&["/w/x/api", "/w/api"]);
        let pick = |q: &str| chosen(&choices(&all, &ds, q, Path::new(HOME), 10));
        assert_eq!(pick(""), ["scratch", "o", "a", "z", "/w/x/api", "/w/api"]);
        // A directory answering better than a session still comes after it.
        assert_eq!(pick("api"), ["a", "o", "/w/api", "/w/x/api"]);
        assert_eq!(pick("login"), ["a"]);
        assert_eq!(pick("x/api"), ["/w/x/api"]);
    }

    #[test]
    fn each_group_keeps_no_more_than_the_cap() {
        let all = sessions(
            &EnvironmentId::default(),
            &tree(),
            &Completions::default(),
            &Changes::default(),
        );
        let ds = dirs(&["/a", "/b", "/c"]);
        let found = choices(&all, &ds, "", Path::new(HOME), 2);
        assert_eq!(chosen(&found), ["scratch", "o", "a", "/a", "/b"]);
    }

    #[test]
    fn a_scratch_session_is_offered_above_everything_while_the_query_answers_to_it() {
        let all = sessions(
            &EnvironmentId::default(),
            &tree(),
            &Completions::default(),
            &Changes::default(),
        );
        let pick = |q: &str| chosen(&choices(&all, &[], q, Path::new(HOME), 10));
        assert_eq!(pick("")[0], "scratch");
        // The session named scratch answers too, after it.
        assert_eq!(pick("scratch"), ["scratch", "z"]);
        assert_eq!(pick("new ses"), ["scratch"]);
        assert!(!pick("login").contains(&"scratch".to_string()));
    }

    #[test]
    fn each_group_is_headed_once_above_its_first_row() {
        let all = sessions(
            &EnvironmentId::default(),
            &tree(),
            &Completions::default(),
            &Changes::default(),
        );
        let ds = dirs(&["/a", "/b"]);
        let headings = |found: &[Choice]| -> Vec<Option<&str>> {
            lines(found, Path::new(HOME))
                .into_iter()
                .map(|l| l.heading)
                .collect()
        };
        let both = choices(&all, &ds, "", Path::new(HOME), 10);
        // The scratch row stands above the groups, under no heading.
        assert_eq!(
            headings(&both),
            [None, Some("Sessions"), None, None, Some("Start in"), None]
        );
        // A group nothing in answers has no heading.
        let only_dirs = choices(&all, &ds, "/a", Path::new(HOME), 10);
        assert_eq!(headings(&only_dirs), [Some("Start in")]);
    }

    #[test]
    fn a_row_draws_a_directory_by_its_short_path_a_session_by_its_name_and_place_and_the_scratch_row_by_what_it_starts()
     {
        let all = sessions(
            &EnvironmentId::default(),
            &tree(),
            &Completions::default(),
            &Changes::default(),
        );
        let found = [
            Choice::Scratch,
            Choice::Session(all[1].clone()),
            Choice::Directory(Candidate {
                environment: EnvironmentId::default(),
                path: "/Users/me/src/x".into(),
                source: Source::Ghq,
            }),
        ];
        assert_eq!(
            lines(&found, Path::new(HOME)),
            [
                Line {
                    heading: None,
                    name: SCRATCH.into(),
                    place: None,
                    badge: String::new(),
                    mark: Mark::Scratch,
                },
                Line {
                    heading: Some("Sessions"),
                    name: "add tokens".into(),
                    place: Some("api › oauth".into()),
                    badge: "waiting".into(),
                    mark: Mark::Session,
                },
                Line {
                    heading: Some("Start in"),
                    name: "~/src/x".into(),
                    place: None,
                    badge: "ghq".into(),
                    mark: Mark::Directory,
                },
            ]
        );
    }

    fn offer(id: &str, state: &'static str, changed: u64) -> SessionOffer {
        SessionOffer {
            environment: EnvironmentId::default(),
            id: id.into(),
            name: format!("{id} name"),
            place: "api".into(),
            state,
            changed,
        }
    }

    fn ids_for(offers: &[SessionOffer], query: &str) -> Vec<String> {
        matching_sessions(offers, query, 10)
            .into_iter()
            .map(|s| s.id)
            .collect()
    }

    #[test]
    fn with_nothing_typed_the_sessions_stand_by_what_needs_the_user_the_latest_change_first() {
        let offers = [
            offer("idle", "idle", 9),
            offer("working-old", "working", 1),
            offer("done", "done", 0),
            offer("working-new", "working", 5),
            offer("starting", "starting", 8),
            offer("waiting-unseen", "waiting", 0),
            offer("failed", "failed", 2),
            offer("waiting-new", "waiting", 7),
            offer("waiting-unseen-too", "waiting", 0),
        ];
        assert_eq!(
            ids_for(&offers, ""),
            [
                "waiting-new",
                "waiting-unseen",
                "waiting-unseen-too",
                "done",
                "failed",
                "working-new",
                "working-old",
                "starting",
                "idle",
            ],
            "sessions never seen to change keep the sidebar's order among their own"
        );
    }

    #[test]
    fn a_query_ranks_by_how_well_each_answers_however_it_stands() {
        let mut offers = [offer("a", "idle", 0), offer("b", "waiting", 3)];
        offers[0].name = "login".into();
        offers[1].name = "the slow logging in".into();
        assert_eq!(ids_for(&offers, "login"), ["a", "b"]);
    }

    #[test]
    fn a_term_at_a_state_keeps_the_sessions_in_that_state() {
        let offers = [
            offer("i", "idle", 0),
            offer("w", "waiting", 0),
            offer("r", "working", 0),
            offer("d", "done", 0),
        ];
        for (query, kept) in [
            ("@w", &["w"][..]),
            ("@waiting", &["w"]),
            ("@wo", &["r"]),
            ("@d", &["d"]),
            ("@i", &["i"]),
            ("@W", &["w"]),
            // Two state terms keep either.
            ("@i @w", &["w", "i"]),
            // A term no state's word starts with keeps none.
            ("@x", &[]),
            // `@` alone names no state in particular.
            ("@", &["w", "d", "r", "i"]),
            // The rest is matched as before.
            ("@w w", &["w"]),
            ("@w nothing", &[]),
        ] {
            assert_eq!(ids_for(&offers, query), kept, "{query:?}");
        }
    }

    #[test]
    fn a_query_naming_a_state_offers_only_sessions() {
        let all = sessions(
            &EnvironmentId::default(),
            &tree(),
            &Completions::default(),
            &Changes::default(),
        );
        let ds = dirs(&["/w/api"]);
        let pick = |q: &str| chosen(&choices(&all, &ds, q, Path::new(HOME), 10));
        assert_eq!(pick("@"), ["o", "a", "z"]);
        assert_eq!(pick("@wo api"), ["a"]);
        assert_eq!(pick("api"), ["a", "o", "/w/api"]);
    }

    #[test]
    fn the_terms_naming_states_are_taken_out_of_what_is_matched() {
        for (query, text, states) in [
            ("api", "api", None),
            ("@w api", "api", Some(vec!["waiting"])),
            ("fix  @d  login", "fix login", Some(vec!["done"])),
            ("@w @w", "", Some(vec!["waiting"])),
            ("@i @w", "", Some(vec!["waiting", "idle"])),
            // `@` inside a term is the term's own.
            ("me@host", "me@host", None),
        ] {
            assert_eq!(
                Narrowing::new(query),
                Narrowing {
                    text: text.into(),
                    states
                },
                "{query:?}"
            );
        }
    }

    #[test]
    fn a_finished_session_the_user_has_seen_is_offered_as_idle() {
        let t = testing::tree()
            .session(session("d").cwd("/w/api").state(State::Done))
            .session(session("u").cwd("/w/api").state(State::Done))
            .build();
        let seen = Completions::default().updated(&t, Some(Viewed::Session("d")));
        let found = sessions(&EnvironmentId::default(), &t, &seen, &Changes::default());
        assert_eq!(
            found
                .iter()
                .map(|s| (s.id.as_str(), s.state))
                .collect::<Vec<_>>(),
            [("d", "idle"), ("u", "done")]
        );
    }

    #[test]
    fn a_row_is_the_same_row_however_it_now_stands() {
        let was = Choice::Session(offer("a", "working", 1));
        assert!(was.same_row(&Choice::Session(offer("a", "waiting", 4))));
        assert!(!was.same_row(&Choice::Session(offer("b", "working", 1))));
        assert!(Choice::Scratch.same_row(&Choice::Scratch));
        assert!(!was.same_row(&Choice::Scratch));
    }

    #[test]
    fn every_source_is_named_by_its_badge() {
        assert_eq!(Source::Tree.label(), "tree");
        assert_eq!(Source::Recent.label(), "recent");
        assert_eq!(Source::Ghq.label(), "ghq");
    }
}
