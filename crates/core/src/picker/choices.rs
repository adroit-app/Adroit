//! The picker that goes to a session or starts one: one list of the sessions,
//! then of the directories to start one in, narrowed by a fuzzy query.

use std::cmp::Reverse;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use adroit_model::{CloneTarget, EnvironmentId, GitHubTyped, Session, Tree};
use adroit_protocol::{Offers, PickWork};

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
    /// A repository to clone.
    Clone,
    /// An owner on GitHub whose repositories to look through.
    Owner,
    /// How to look through GitHub.
    Help,
}

/// What the scratch row says.
pub const SCRATCH: &str = "New scratch session";

/// What the row telling how to clone from GitHub says.
pub const GITHUB_HELP: &str = "Type github:owner/repo to clone from GitHub";

/// What a row of the list offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// A session to start in the scratch folder.
    Scratch,
    /// A session to open.
    Session(SessionOffer),
    /// A directory to start a session in.
    Directory(Candidate),
    /// A repository to clone, then take as a directory chosen.
    Clone(CloneOffer),
    /// An owner on GitHub, typed to look through its repositories.
    Owner(String),
    /// [`GITHUB_HELP`], which types the prefix that looks through GitHub.
    GitHubHelp,
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
            (Choice::Clone(a), Choice::Clone(b)) => a == b,
            (Choice::Owner(a), Choice::Owner(b)) => a == b,
            (Choice::GitHubHelp, Choice::GitHubHelp) => true,
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

/// A repository typed that the host of `environment` can clone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneOffer {
    pub environment: EnvironmentId,
    pub target: CloneTarget,
}

/// The repository `query` names, offered to clone in `environment` while its
/// host clones with ghq and ghq does not keep it already.
pub fn clone_offer(
    environment: &EnvironmentId,
    offers: &Offers,
    query: &str,
) -> Option<CloneOffer> {
    let target = CloneTarget::parse(query)?;
    (offers.clones && target.kept_in(&offers.ghq).is_none()).then(|| CloneOffer {
        environment: environment.clone(),
        target,
    })
}

/// What one environment's host offers of GitHub: whether it clones, what
/// ghq keeps there, and what `gh` listed there.
pub struct GitHubOffers<'a> {
    pub environment: &'a EnvironmentId,
    pub offers: &'a Offers,
    pub listed: &'a PickWork,
}

/// The best `cap` rows for what `typed` asks of GitHub, from the hosts that
/// clone: the owners `gh` is logged in as for the start of an owner, or the
/// repositories of one ghq does not keep for an owner and a slash, the one
/// typed in full among them while the owner's are not listed. [`None`] when
/// `typed` asks nothing of GitHub.
pub fn github_choices<'a>(
    typed: &str,
    offered: impl IntoIterator<Item = GitHubOffers<'a>>,
    cap: usize,
) -> Option<Vec<Choice>> {
    let asked = GitHubTyped::parse(typed)?;
    let cloning = offered.into_iter().filter(|o| o.offers.clones);
    Some(match asked {
        GitHubTyped::Owner(start) => {
            let mut owners: Vec<&String> = Vec::new();
            for o in cloning {
                for owner in o.listed.accounts.iter().flatten() {
                    if !owners.contains(&owner) {
                        owners.push(owner);
                    }
                }
            }
            let query = Query::new(&start);
            best(
                owners.into_iter().filter_map(|owner| {
                    query
                        .rank_named(owner, owner)
                        .map(|rank| (rank, Choice::Owner(owner.clone())))
                }),
                cap,
            )
        }
        GitHubTyped::Repository { owner, name } => {
            let query = Query::new(&name);
            cloning
                .flat_map(|o| {
                    let listed = o.listed.owned.get(&owner);
                    let names: Vec<&str> = match listed {
                        Some(names) => names.iter().map(String::as_str).collect(),
                        None if !name.is_empty() => vec![name.as_str()],
                        None => Vec::new(),
                    };
                    let offers = names.into_iter().filter_map(|n| {
                        let target = GitHubTyped::target(&owner, n);
                        let rank = query.rank_named(n, n)?;
                        target.kept_in(&o.offers.ghq).is_none().then(|| {
                            let offer = CloneOffer {
                                environment: o.environment.clone(),
                                target,
                            };
                            (rank, Choice::Clone(offer))
                        })
                    });
                    best(offers, cap)
                })
                .collect()
        }
    })
}

/// Whether the row telling how to clone from GitHub is offered: where a host
/// clones, while nothing is typed or the start of the prefix.
pub fn github_help<'a>(typed: &str, offered: impl IntoIterator<Item = &'a Offers>) -> bool {
    let typed = typed.trim();
    let prefix = GitHubTyped::PREFIX.trim_end_matches(':');
    prefix.starts_with(typed) && offered.into_iter().any(|o| o.clones)
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
                Choice::Clone(c) => (
                    Some("Clone with ghq"),
                    Line {
                        heading: None,
                        name: c.target.path.clone(),
                        place: label(&c.environment),
                        badge: "ghq".into(),
                        mark: Mark::Clone,
                    },
                ),
                Choice::Owner(owner) => (
                    Some("GitHub owners"),
                    Line {
                        heading: None,
                        name: owner.clone(),
                        place: None,
                        badge: "gh".into(),
                        mark: Mark::Owner,
                    },
                ),
                Choice::GitHubHelp => (
                    None,
                    Line {
                        heading: None,
                        name: GITHUB_HELP.into(),
                        place: None,
                        badge: String::new(),
                        mark: Mark::Help,
                    },
                ),
            };
            let heading = group.filter(|_| last != Some(group));
            last = Some(group);
            Line { heading, ..line }
        })
        .collect()
}

pub use adroit_model::paths::tilde as short;

#[cfg(test)]
mod tests;
