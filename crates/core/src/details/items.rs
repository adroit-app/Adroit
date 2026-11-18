//! The pull requests and issues a session's conversation brought up, told of
//! as GitHub last listed those open in the repository shown.

use std::path::{Path, PathBuf};

use adroit_model::github::{Checks, Listing, Repository};
use adroit_model::links::Link;

/// How a pull request or issue stands, as far as the repository shown tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemState {
    /// GitHub lists it among the open ones.
    Open,
    /// Of the repository shown, but not among those open: merged or closed.
    Closed,
    /// Of another repository, or of one not read yet.
    Unknown,
}

/// A pull request or issue as a row lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRow {
    /// What tells it apart from every other row.
    pub key: String,
    pub listing: Listing,
    /// Its page on GitHub.
    pub url: String,
    /// `owner/name`.
    pub repo: String,
    pub number: u64,
    pub state: ItemState,
    /// What GitHub calls it, while it lists it.
    pub title: Option<String>,
    pub draft: bool,
    /// A pull request's checks, while GitHub lists it.
    pub checks: Option<Checks>,
    /// The repository shown that lists it open, which shows it in Adroit.
    pub root: Option<PathBuf>,
}

impl ItemRow {
    /// How it is named: `owner/name#number`.
    pub fn reference(&self) -> String {
        format!("{}#{}", self.repo, self.number)
    }

    /// What its row says beside its name: how it stands and its checks.
    pub fn tag(&self) -> String {
        let mut said: Vec<String> = Vec::new();
        match (self.state, self.listing) {
            (ItemState::Open, _) if self.draft => said.push("draft".into()),
            (ItemState::Open, _) => said.push("open".into()),
            (ItemState::Closed, Listing::Pulls) => said.push("merged or closed".into()),
            (ItemState::Closed, Listing::Issues) => said.push("closed".into()),
            (ItemState::Unknown, _) => {}
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

/// The repository and number of the item of `listing` whose page `url` is,
/// or a page of it is, and the page itself.
fn item_of(url: &str, listing: Listing) -> Option<(String, u64, String)> {
    let kind = match listing {
        Listing::Pulls => "pull",
        Listing::Issues => "issues",
    };
    let rest = url.strip_prefix("https://github.com/")?;
    let rest = rest.split(['#', '?']).next()?;
    let mut parts = rest.split('/');
    let (owner, name, of, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if owner.is_empty() || name.is_empty() || of != kind {
        return None;
    }
    let number: u64 = number.parse().ok()?;
    let repo = format!("{owner}/{name}");
    let page = format!("https://github.com/{repo}/{kind}/{number}");
    Some((repo, number, page))
}

/// The items of `listing` among `urls`, once each in the order first named,
/// told of as `shown`, the repository shown at its root, lists them.
pub fn item_rows(
    urls: &[Link],
    listing: Listing,
    shown: Option<(&Path, &Repository)>,
) -> Vec<ItemRow> {
    let open = shown.and_then(|(_, r)| r.read(listing)?.as_ref().ok());
    let mut out: Vec<ItemRow> = Vec::new();
    for (repo, number, url) in urls.iter().filter_map(|l| item_of(&l.target, listing)) {
        if out.iter().any(|p| p.url == url) {
            continue;
        }
        let listed = open.and_then(|items| items.iter().find(|i| i.url == url));
        let ours = shown.is_some_and(|(_, r)| r.slug == repo) && open.is_some();
        let state = match (listed, ours) {
            (Some(_), _) => ItemState::Open,
            (None, true) => ItemState::Closed,
            (None, false) => ItemState::Unknown,
        };
        out.push(ItemRow {
            // No address starts with a NUL.
            key: format!("\0item {url}"),
            listing,
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
