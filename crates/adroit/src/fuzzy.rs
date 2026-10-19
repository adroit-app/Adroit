//! Narrowing a list of paths by what was typed, with fzf's syntax: the
//! characters of each term appear in order, `'exact`, `^start`, `end$` and
//! `!not` are understood, and a lowercase term ignores case.
//!
//! The engine is [`nucleo_matcher`], the matcher behind Helix's pickers.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::path::Path;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

thread_local! {
    // A matcher owns the allocations it works in and is meant to be reused;
    // building one per candidate would cost more than the match.
    static SCRATCH: RefCell<(Matcher, Vec<char>)> =
        RefCell::new((Matcher::new(Config::DEFAULT.match_paths()), Vec::new()));
}

/// One run of a candidate's text, and whether the query marked it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub matched: bool,
}

/// How well a candidate answered: the better score first, then the shorter
/// candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rank {
    score: u32,
    length: usize,
}

impl Rank {
    // Nothing asked tells no two candidates apart, not even by length, so an
    // unnarrowed list keeps the order it was given.
    const UNASKED: Self = Self {
        score: 0,
        length: 0,
    };
}

/// The best `cap` of what was ranked, best first; a tie keeps the order the
/// candidates arrived in.
pub fn best<T>(ranked: impl IntoIterator<Item = (Rank, T)>, cap: usize) -> Vec<T> {
    let mut all: Vec<(Rank, T)> = ranked.into_iter().collect();
    all.sort_by_key(|(r, _)| (Reverse(r.score), r.length));
    all.into_iter().take(cap).map(|(_, item)| item).collect()
}

/// What was typed, parsed once and asked of many candidates.
#[derive(Debug, Clone, Default)]
pub struct Query {
    pattern: Pattern,
}

impl Query {
    pub fn new(query: &str) -> Self {
        Self {
            pattern: Pattern::parse(query, CaseMatching::Smart, Normalization::Smart),
        }
    }

    /// Whether nothing was asked; whitespace alone asks nothing.
    pub fn is_empty(&self) -> bool {
        self.pattern.atoms.is_empty()
    }

    /// How well `path` answers, or [`None`] if it does not.
    ///
    /// The path is matched whole, and its last component is scored again on
    /// its own and added, so `adroit` finds `~/src/adroit` above
    /// `~/src/adroit/docs`.
    pub fn rank_path(&self, path: &Path) -> Option<Rank> {
        let name = path.file_name().map(|n| n.to_string_lossy());
        self.rank(&path.to_string_lossy(), name.as_deref())
    }

    /// How well `text` answers, or [`None`] if it does not, with `name`, a
    /// part of it, scored again on its own and added.
    pub fn rank_named(&self, text: &str, name: &str) -> Option<Rank> {
        self.rank(text, Some(name))
    }

    fn rank(&self, whole: &str, name: Option<&str>) -> Option<Rank> {
        if self.is_empty() {
            return Some(Rank::UNASKED);
        }
        let score = self.score(whole)?;
        let named = name.and_then(|n| self.score(n)).unwrap_or(0);
        Some(Rank {
            score: score + named,
            length: whole.chars().count(),
        })
    }

    /// `text` cut into the runs the query marked and the runs it did not.
    ///
    /// Each term marks what of it is in `text` on its own, so a row drawn
    /// shorter than the path it was ranked on (`~` for the home directory)
    /// still shows why it is there.
    pub fn highlight_path(&self, text: &str) -> Vec<Span> {
        if self.is_empty() || text.is_empty() {
            return vec![Span {
                text: text.to_string(),
                matched: false,
            }];
        }
        let mut indices = Vec::new();
        SCRATCH.with_borrow_mut(|(matcher, buf)| {
            // One char a position, as `spans` cuts: `Utf32Str::new` keeps one
            // a grapheme, which puts every mark after a decomposed character
            // (`か` and a combining sound mark, as macOS file names can be)
            // one place early.
            buf.clear();
            buf.extend(text.chars());
            let haystack = Utf32Str::Unicode(&buf[..]);
            for atom in &self.pattern.atoms {
                if atom.negative {
                    continue;
                }
                let marked = indices.len();
                if atom.indices(haystack, matcher, &mut indices).is_none() {
                    indices.truncate(marked);
                }
            }
        });
        indices.sort_unstable();
        indices.dedup();
        spans(text, &indices)
    }

    fn score(&self, text: &str) -> Option<u32> {
        SCRATCH
            .with_borrow_mut(|(matcher, buf)| self.pattern.score(Utf32Str::new(text, buf), matcher))
    }
}

/// Cut `text` at the sorted character positions in `matched`.
fn spans(text: &str, matched: &[u32]) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut next = matched.iter().copied().peekable();
    for (at, ch) in text.chars().enumerate() {
        let at = at as u32;
        while next.peek().is_some_and(|i| *i < at) {
            next.next();
        }
        let hit = next.peek() == Some(&at);
        match spans.last_mut() {
            Some(last) if last.matched == hit => last.text.push(ch),
            _ => spans.push(Span {
                text: ch.to_string(),
                matched: hit,
            }),
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finds(query: &str, path: &str) -> bool {
        Query::new(query).rank_path(Path::new(path)).is_some()
    }

    fn score(query: &str, path: &str) -> u32 {
        Query::new(query).rank_path(Path::new(path)).unwrap().score
    }

    fn text_of(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    fn matched_of(spans: &[Span]) -> String {
        spans
            .iter()
            .filter(|s| s.matched)
            .map(|s| s.text.as_str())
            .collect()
    }

    #[test]
    fn the_characters_of_a_term_appear_in_order_anywhere_in_the_path() {
        assert!(finds("adr", "/home/me/src/adroit"));
        assert!(finds("srcad", "/home/me/src/adroit"));
        assert!(!finds("rda", "/home/me/src/adroit"));
    }

    #[test]
    fn every_term_has_to_appear_in_any_order() {
        assert!(finds("adroit src", "/home/me/src/adroit"));
        assert!(finds("src adroit", "/home/me/src/adroit"));
        assert!(!finds("src arto", "/home/me/src/adroit"));
    }

    #[test]
    fn the_fzf_operators_are_understood() {
        assert!(finds("'adroit", "/w/adroit"));
        assert!(!finds("'adroit", "/w/a-d-r-o-i-t"));
        assert!(finds("^/w", "/w/adroit"));
        assert!(!finds("^w", "/w/adroit"));
        assert!(finds("roit$", "/w/adroit"));
        assert!(!finds("roit$", "/w/adroit-old"));
        assert!(finds("adroit !old", "/w/adroit"));
        assert!(!finds("adroit !old", "/w/adroit-old"));
    }

    #[test]
    fn a_lowercase_term_ignores_case_and_a_capital_means_it() {
        assert!(finds("adroit", "/w/Adroit"));
        assert!(finds("Adroit", "/w/Adroit"));
        assert!(!finds("Adroit", "/w/adroit"));
    }

    #[test]
    fn whitespace_alone_asks_nothing_and_every_path_answers() {
        let query = Query::new("   ");
        assert!(query.is_empty());
        assert!(query.rank_path(Path::new("/anything")).is_some());
    }

    #[test]
    fn the_last_component_scores_above_the_same_letters_further_up() {
        let named = score("adroit", "/w/src/adroit");
        let buried = score("adroit", "/w/adroit/src");
        assert!(named > buried, "{named} should beat {buried}");
    }

    #[test]
    fn a_name_scores_above_the_same_letters_elsewhere_in_the_text() {
        let query = Query::new("docs");
        let named = query.rank_named("docs scratch", "docs").unwrap();
        let elsewhere = query.rank_named("scratch docs", "scratch").unwrap();
        assert!(named.score > elsewhere.score);
        assert!(query.rank_named("scratch", "scratch").is_none());
        assert!(Query::new("").rank_named("any", "any").is_some());
    }

    #[test]
    fn the_shorter_of_two_equal_answers_comes_first() {
        let query = Query::new("adroit");
        let ranked = best(
            [
                (query.rank_path(Path::new("/w/x/adroit")).unwrap(), "long"),
                (query.rank_path(Path::new("/w/adroit")).unwrap(), "short"),
            ],
            usize::MAX,
        );
        assert_eq!(ranked, vec!["short", "long"]);
    }

    #[test]
    fn nothing_asked_keeps_the_order_given_whatever_the_lengths() {
        let query = Query::new("");
        let ranked = best(
            [
                (query.rank_path(Path::new("/a/long/path")).unwrap(), "first"),
                (query.rank_path(Path::new("/b")).unwrap(), "second"),
            ],
            usize::MAX,
        );
        assert_eq!(ranked, vec!["first", "second"]);
    }

    #[test]
    fn ranking_keeps_no_more_than_it_was_asked_for() {
        let query = Query::new("");
        let ranked = best(
            (0..10).map(|i| (query.rank_path(Path::new("/w")).unwrap(), i)),
            3,
        );
        assert_eq!(ranked, vec![0, 1, 2]);
        assert!(best([(Rank::UNASKED, 0)], 0).is_empty());
    }

    #[test]
    fn highlighting_marks_the_characters_that_answered() {
        let spans = Query::new("adr").highlight_path("~/src/adroit");
        assert_eq!(text_of(&spans), "~/src/adroit");
        assert_eq!(matched_of(&spans), "adr");
    }

    #[test]
    fn a_term_not_in_sight_marks_nothing_while_the_others_still_do() {
        let spans = Query::new("home adroit").highlight_path("~/src/adroit");
        assert_eq!(matched_of(&spans), "adroit");
        let spans = Query::new("adroit !old").highlight_path("~/src/adroit");
        assert_eq!(matched_of(&spans), "adroit");
    }

    #[test]
    fn nothing_asked_or_nothing_found_is_one_unmarked_span() {
        for q in ["", "zzz"] {
            let spans = Query::new(q).highlight_path("~/src/adroit");
            assert_eq!(
                spans,
                vec![Span {
                    text: "~/src/adroit".into(),
                    matched: false
                }]
            );
        }
    }

    #[test]
    fn highlighting_counts_characters_rather_than_bytes() {
        let spans = Query::new("あd").highlight_path("~/あいう/docs");
        assert_eq!(text_of(&spans), "~/あいう/docs");
        assert_eq!(matched_of(&spans), "あd");
    }

    #[test]
    fn highlighting_after_a_decomposed_character_marks_what_answered() {
        let spans = Query::new("docs").highlight_path("~/\u{304b}\u{3099}/docs");
        assert_eq!(matched_of(&spans), "docs");
    }
}
