//! The picker that goes to a session or starts one: one list of the sessions,
//! then of the directories to start one in, narrowed by a fuzzy query, or, for
//! a directory the list does not offer, a directory tree.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use adroit_model::{Session, Tree};

use crate::filer::Filer;
use crate::fuzzy::{Query, best};

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
    /// A session to start in a new directory of its own.
    Scratch,
    /// A session to open.
    Session(SessionOffer),
    /// A directory to start a session in.
    Directory(Candidate),
}

/// Where a candidate came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A session runs there now.
    Running,
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
            Source::Running => "running",
            Source::Recent => "recent",
            Source::Ghq => "ghq",
            Source::Listed => "listed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub source: Source,
}

/// A session the picker offers, with where it runs and how it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOffer {
    pub id: String,
    pub name: String,
    /// The workspace, and the worktree when the sidebar shows one.
    pub place: String,
    pub state: &'static str,
}

/// Every session in the sidebar's order.
pub fn sessions(tree: &Tree) -> Vec<SessionOffer> {
    let offer = |s: &Session, place: String| SessionOffer {
        id: s.id.clone(),
        name: s.name.clone(),
        place,
        state: s.state.word(),
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
/// and where it runs, its name counting for more; with nothing typed, the
/// first `cap` as given.
pub fn matching_sessions(offers: &[SessionOffer], query: &str, cap: usize) -> Vec<SessionOffer> {
    let query = Query::new(query);
    best(
        offers.iter().filter_map(|s| {
            let text = format!("{} {}", s.name, s.place);
            query
                .rank_named(&text, &s.name)
                .map(|rank| (rank, s.clone()))
        }),
        cap,
    )
}

/// The sources as one list, in the order given, each path once: the first
/// source that offers a path is the one it is listed under.
pub fn candidates(
    running: &[PathBuf],
    recent: &[PathBuf],
    ghq: &[PathBuf],
    listed: &[PathBuf],
) -> Vec<Candidate> {
    let mut seen = HashSet::new();
    [
        (running, Source::Running),
        (recent, Source::Recent),
        (ghq, Source::Ghq),
        (listed, Source::Listed),
    ]
    .into_iter()
    .flat_map(|(paths, source)| paths.iter().map(move |path| (path, source)))
    .filter(|(path, _)| seen.insert(*path))
    .map(|(path, source)| Candidate {
        path: path.clone(),
        source,
    })
    .collect()
}

/// The folders in `roots`, each root's in name order, leaving out hidden ones
/// and a root that cannot be read.
pub fn folders_in(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .flat_map(|root| {
            let mut found: Vec<PathBuf> = std::fs::read_dir(root)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .filter(|e| e.path().is_dir())
                .map(|e| e.path())
                .collect();
            found.sort();
            found
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
/// directory never comes between sessions.
pub fn choices(
    sessions: &[SessionOffer],
    directories: &[Candidate],
    query: &str,
    home: &Path,
    cap: usize,
) -> Vec<Choice> {
    let scratch = Query::new(query)
        .rank_named(SCRATCH, SCRATCH)
        .map(|_| Choice::Scratch);
    let sessions = matching_sessions(sessions, query, cap).into_iter();
    let directories = matching(directories, query, home, cap).into_iter();
    scratch
        .into_iter()
        .chain(sessions.map(Choice::Session))
        .chain(directories.map(Choice::Directory))
        .collect()
}

/// The rows `choices` are drawn as, the first of each group under its heading.
pub fn lines(choices: &[Choice], home: &Path) -> Vec<Line> {
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
                        place: Some(s.place.clone()),
                        badge: s.state.into(),
                        mark: Mark::Session,
                    },
                ),
                Choice::Directory(d) => (
                    Some("Start in"),
                    Line {
                        heading: None,
                        name: short(&d.path, home),
                        place: None,
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
    crate::nav::wrap(cursor, delta, len)
}

/// `path` with the home directory written as `~`.
pub fn short(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    List,
    Tree,
}

/// What the picker holds while it is open.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Picker {
    pub query: String,
    pub cursor: usize,
    pub mode: Mode,
    pub filer: Filer,
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
    use std::collections::HashMap;

    use adroit_model::{State, Wait, Worktree, build};

    use super::*;

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
            &paths(&["/r2", "/r1"]),
            &paths(&["/c"]),
            &paths(&["/g2", "/g1"]),
            &[],
        );
        assert_eq!(
            listed(&cs),
            vec![
                ("/r2", Source::Running),
                ("/r1", Source::Running),
                ("/c", Source::Recent),
                ("/g2", Source::Ghq),
                ("/g1", Source::Ghq),
            ]
        );
    }

    #[test]
    fn a_path_offered_twice_is_listed_once_under_the_first_source_that_offers_it() {
        let cs = candidates(
            &paths(&["/a"]),
            &paths(&["/b", "/a", "/b"]),
            &paths(&["/a", "/b", "/g"]),
            &[],
        );
        assert_eq!(
            listed(&cs),
            vec![
                ("/a", Source::Running),
                ("/b", Source::Recent),
                ("/g", Source::Ghq),
            ]
        );
    }

    #[test]
    fn nothing_typed_keeps_the_list_in_order_up_to_the_cap() {
        let cs = candidates(&[], &paths(&["/z/long/path", "/a", "/m"]), &[], &[]);
        assert_eq!(
            listed(&matching(&cs, "", 2)),
            vec![("/z/long/path", Source::Recent), ("/a", Source::Recent)]
        );
    }

    #[test]
    fn a_query_keeps_what_matches_best_first() {
        let cs = candidates(
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
        let cs = candidates(&paths(&["/a"]), &[], &paths(&["/g"]), &paths(&["/g", "/l"]));
        assert_eq!(
            cs.iter()
                .map(|c| (c.path.to_str().unwrap(), c.source))
                .collect::<Vec<_>>(),
            vec![
                ("/a", Source::Running),
                ("/g", Source::Ghq),
                ("/l", Source::Listed)
            ]
        );
    }

    #[test]
    fn a_root_lists_the_folders_in_it_but_hidden_ones_and_files() {
        let root = tempfile::tempdir().unwrap();
        for d in ["b", "a", ".git"] {
            std::fs::create_dir(root.path().join(d)).unwrap();
        }
        std::fs::write(root.path().join("file"), "").unwrap();
        let got = folders_in(&[root.path().to_path_buf(), PathBuf::from("/no/such")]);
        assert_eq!(got, [root.path().join("a"), root.path().join("b")]);
    }

    #[test]
    fn candidates_that_answer_equally_keep_the_order_they_were_listed_in() {
        let cs = candidates(&paths(&["/b/x"]), &paths(&["/a/x"]), &[], &[]);
        assert_eq!(
            listed(&matching(&cs, "x", 10)),
            vec![("/b/x", Source::Running), ("/a/x", Source::Recent)]
        );
    }

    #[test]
    fn a_query_is_matched_against_the_path_as_drawn_with_home_as_a_tilde() {
        let cs = candidates(
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
    fn the_home_directory_is_written_as_a_tilde() {
        let home = Path::new("/Users/me");
        assert_eq!(short(Path::new("/Users/me/src/x"), home), "~/src/x");
        assert_eq!(short(Path::new("/Users/me"), home), "~");
        assert_eq!(short(Path::new("/Users/meow"), home), "/Users/meow");
        assert_eq!(short(Path::new("/opt/x"), home), "/opt/x");
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

    fn session(id: &str, name: &str, cwd: &str, state: State) -> Session {
        Session {
            id: id.into(),
            name: name.into(),
            cwd: cwd.into(),
            state,
            ..Default::default()
        }
    }

    const OAUTH: &str = "/w/api/.claude/worktrees/oauth";

    /// `api` runs a session in its main checkout and one in a worktree; `tmp`
    /// is outside git.
    fn tree() -> Tree {
        let wt = |path: &str, branch: &str| Worktree {
            path: path.into(),
            repo: "/w/api".into(),
            branch: Some(branch.into()),
        };
        let found: HashMap<PathBuf, Option<Worktree>> = [
            ("/w/api", Some(wt("/w/api", "main"))),
            (OAUTH, Some(wt(OAUTH, "oauth"))),
            ("/tmp", None),
        ]
        .into_iter()
        .map(|(p, w)| (PathBuf::from(p), w))
        .collect();
        build(
            &[
                session("a", "fix login", "/w/api", State::Working),
                session("o", "add tokens", OAUTH, State::Waiting(Wait::Input)),
                session("z", "scratch", "/tmp", State::Idle),
            ],
            &[],
            &found,
            &[],
            None,
        )
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
            offered(&sessions(&tree())),
            vec![
                ("a", "api › main", "working"),
                ("o", "api › oauth", "waiting"),
                ("z", "tmp", "idle"),
            ]
        );
    }

    #[test]
    fn a_session_is_found_by_its_name_or_by_where_it_runs() {
        let all = sessions(&tree());
        let ids = |q: &str| -> Vec<String> {
            matching_sessions(&all, q, 10)
                .into_iter()
                .map(|s| s.id)
                .collect()
        };
        assert_eq!(ids("login"), ["a"]);
        assert_eq!(ids("oauth"), ["o"]);
        assert_eq!(ids("api"), ["a", "o"]);
        assert_eq!(ids(""), ["a", "o", "z"]);
        assert!(ids("nothing").is_empty());
    }

    #[test]
    fn a_session_named_by_the_query_comes_before_one_that_only_runs_where_it_says() {
        let found: HashMap<PathBuf, Option<Worktree>> = HashMap::new();
        let t = build(
            &[
                session("x", "docs", "/w/scratch", State::Idle),
                session("s", "scratch", "/w/docs", State::Idle),
            ],
            &[],
            &found,
            &[],
            None,
        );
        let ids: Vec<String> = matching_sessions(&sessions(&t), "scratch", 10)
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids, ["s", "x"]);
    }

    fn dirs(ps: &[&str]) -> Vec<Candidate> {
        candidates(&[], &paths(ps), &[], &[])
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
        let all = sessions(&tree());
        let ds = dirs(&["/w/x/api", "/w/api"]);
        let pick = |q: &str| chosen(&choices(&all, &ds, q, Path::new(HOME), 10));
        assert_eq!(pick(""), ["scratch", "a", "o", "z", "/w/x/api", "/w/api"]);
        // A directory answering better than a session still comes after it.
        assert_eq!(pick("api"), ["a", "o", "/w/api", "/w/x/api"]);
        assert_eq!(pick("login"), ["a"]);
        assert_eq!(pick("x/api"), ["/w/x/api"]);
    }

    #[test]
    fn each_group_keeps_no_more_than_the_cap() {
        let all = sessions(&tree());
        let ds = dirs(&["/a", "/b", "/c"]);
        let found = choices(&all, &ds, "", Path::new(HOME), 2);
        assert_eq!(chosen(&found), ["scratch", "a", "o", "/a", "/b"]);
    }

    #[test]
    fn a_scratch_session_is_offered_above_everything_while_the_query_answers_to_it() {
        let all = sessions(&tree());
        let pick = |q: &str| chosen(&choices(&all, &[], q, Path::new(HOME), 10));
        assert_eq!(pick("")[0], "scratch");
        // The session named scratch answers too, after it.
        assert_eq!(pick("scratch"), ["scratch", "z"]);
        assert_eq!(pick("new ses"), ["scratch"]);
        assert!(!pick("login").contains(&"scratch".to_string()));
    }

    #[test]
    fn each_group_is_headed_once_above_its_first_row() {
        let all = sessions(&tree());
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
        let all = sessions(&tree());
        let found = [
            Choice::Scratch,
            Choice::Session(all[1].clone()),
            Choice::Directory(Candidate {
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

    #[test]
    fn every_source_is_named_by_its_badge() {
        assert_eq!(Source::Running.label(), "running");
        assert_eq!(Source::Recent.label(), "recent");
        assert_eq!(Source::Ghq.label(), "ghq");
    }
}
