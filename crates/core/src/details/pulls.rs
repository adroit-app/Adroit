//! The pull requests a session's conversation brought up, told of as GitHub
//! last listed those open in the repository shown.

use std::path::{Path, PathBuf};

use adroit_model::github::{Checks, Listing, Repository};
use adroit_model::links::Link;

/// How a pull request stands, as far as the repository shown tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullState {
    /// GitHub lists it among the open ones.
    Open,
    /// Of the repository shown, but not among those open: merged or closed.
    Closed,
    /// Of another repository, or of one not read yet.
    Unknown,
}

/// A pull request as a row lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRow {
    /// What tells it apart from every other row.
    pub key: String,
    /// Its page on GitHub.
    pub url: String,
    /// `owner/name`.
    pub repo: String,
    pub number: u64,
    pub state: PullState,
    /// What GitHub calls it, while it lists it.
    pub title: Option<String>,
    pub draft: bool,
    pub checks: Option<Checks>,
    /// The repository shown that lists it open, which shows it in Adroit.
    pub root: Option<PathBuf>,
}

impl PullRow {
    /// How it is named: `owner/name#number`.
    pub fn reference(&self) -> String {
        format!("{}#{}", self.repo, self.number)
    }

    /// What its row says beside its name: how it stands and its checks.
    pub fn tag(&self) -> String {
        let mut said: Vec<String> = Vec::new();
        match self.state {
            PullState::Open if self.draft => said.push("draft".into()),
            PullState::Open => said.push("open".into()),
            PullState::Closed => said.push("merged or closed".into()),
            PullState::Unknown => {}
        }
        if let Some(checks) = &self.checks {
            let all = checks.passed + checks.failed + checks.pending;
            if all > 0 {
                said.push(format!("{}/{all} checks", checks.passed));
            }
        }
        if self.title.is_some() {
            said.push(self.reference());
        }
        said.join(" · ")
    }
}

/// The repository and number of the pull request whose page `url` is, or a
/// page of it is, and the page itself.
fn pull_of(url: &str) -> Option<(String, u64, String)> {
    let rest = url.strip_prefix("https://github.com/")?;
    let rest = rest.split(['#', '?']).next()?;
    let mut parts = rest.split('/');
    let (owner, name, kind, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if owner.is_empty() || name.is_empty() || kind != "pull" {
        return None;
    }
    let number: u64 = number.parse().ok()?;
    let repo = format!("{owner}/{name}");
    let page = format!("https://github.com/{repo}/pull/{number}");
    Some((repo, number, page))
}

/// The pull requests among `urls`, once each in the order first named, told
/// of as `shown`, the repository shown at its root, lists them.
pub fn pull_rows(urls: &[Link], shown: Option<(&Path, &Repository)>) -> Vec<PullRow> {
    let open = shown.and_then(|(_, r)| r.read(Listing::Pulls)?.as_ref().ok());
    let mut out: Vec<PullRow> = Vec::new();
    for (repo, number, url) in urls.iter().filter_map(|l| pull_of(&l.target)) {
        if out.iter().any(|p| p.url == url) {
            continue;
        }
        let listed = open.and_then(|items| items.iter().find(|i| i.url == url));
        let ours = shown.is_some_and(|(_, r)| r.slug == repo) && open.is_some();
        let state = match (listed, ours) {
            (Some(_), _) => PullState::Open,
            (None, true) => PullState::Closed,
            (None, false) => PullState::Unknown,
        };
        out.push(PullRow {
            key: format!("\0pull {url}"),
            url,
            repo,
            number,
            state,
            title: listed.map(|i| i.title.clone()),
            draft: listed.is_some_and(|i| i.draft),
            checks: listed
                .and_then(|i| i.pull.as_ref())
                .map(|p| p.checks.clone()),
            root: listed.and(shown).map(|(root, _)| root.to_path_buf()),
        });
    }
    out
}
