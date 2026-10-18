//! The picker that asks which directory a new session works in: one list of
//! candidates narrowed by a fuzzy query, or a directory tree for one the list
//! does not offer.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::filer::Filer;
use crate::fuzzy::{Query, best};

/// Where a candidate came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A session runs there now.
    Running,
    /// Claude Code has been used there.
    Recent,
    /// `ghq list` names it.
    Ghq,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Running => "running",
            Source::Recent => "recent",
            Source::Ghq => "ghq",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub source: Source,
}

/// The sources as one list, in the order given, each path once: the first
/// source that offers a path is the one it is listed under.
pub fn candidates(running: &[PathBuf], recent: &[PathBuf], ghq: &[PathBuf]) -> Vec<Candidate> {
    let mut seen = HashSet::new();
    [
        (running, Source::Running),
        (recent, Source::Recent),
        (ghq, Source::Ghq),
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
        let cs = candidates(&[], &paths(&["/z/long/path", "/a", "/m"]), &[]);
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
    fn candidates_that_answer_equally_keep_the_order_they_were_listed_in() {
        let cs = candidates(&paths(&["/b/x"]), &paths(&["/a/x"]), &[]);
        assert_eq!(
            listed(&matching(&cs, "x", 10)),
            vec![("/b/x", Source::Running), ("/a/x", Source::Recent)]
        );
    }

    #[test]
    fn a_query_is_matched_against_the_path_as_drawn_with_home_as_a_tilde() {
        let cs = candidates(&[], &paths(&["/Users/me/src/adroit", "/opt/adroit"]), &[]);
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

    #[test]
    fn every_source_is_named_by_its_badge() {
        assert_eq!(Source::Running.label(), "running");
        assert_eq!(Source::Recent.label(), "recent");
        assert_eq!(Source::Ghq.label(), "ghq");
    }
}
