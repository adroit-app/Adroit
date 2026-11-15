//! What GitHub holds open of a repository, as the tree lists it.

use serde::{Deserialize, Serialize};

/// What the tree lists of a repository on GitHub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Listing {
    Pulls,
    Issues,
}

impl Listing {
    pub const ALL: [Listing; 2] = [Listing::Pulls, Listing::Issues];

    pub fn name(self) -> &'static str {
        match self {
            Listing::Pulls => "Pull Requests",
            Listing::Issues => "Issues",
        }
    }
}

/// An open pull request or issue.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub number: u64,
    pub title: String,
    /// Its page on GitHub.
    pub url: String,
    /// The login of whoever opened it.
    pub author: String,
    /// Whether it is a pull request marked as a draft.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub draft: bool,
    /// When it was opened and last updated, as GitHub writes a time.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub created_at: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<Label>,
    /// The logins it is assigned to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignees: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub milestone: Option<String>,
    /// What only a pull request has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull: Option<Pull>,
}

/// A label, with its colour as GitHub gives it: six hex digits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub name: String,
    #[serde(default)]
    pub color: String,
}

/// What a pull request has beside what an issue has.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pull {
    /// The branch it merges, and the one it merges into.
    pub head: String,
    pub base: String,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
    /// How its review stands, when one is required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<Review>,
    /// Who it awaits a review from: logins, and teams by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested: Vec<String>,
    #[serde(default)]
    pub checks: Checks,
}

/// How a pull request's review stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Review {
    Required,
    ChangesRequested,
    Approved,
}

/// The checks run on a pull request's head, counted by how they ended.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checks {
    pub passed: u32,
    pub failed: u32,
    pub pending: u32,
    /// The names of those that failed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failing: Vec<String>,
}

/// How a pull request's checks stand as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    Passed,
    Failed,
    Pending,
}

impl Checks {
    /// Failed once any failed, pending while any runs, passed once all
    /// passed; none when there are none.
    pub fn standing(&self) -> Option<Standing> {
        if self.failed > 0 {
            Some(Standing::Failed)
        } else if self.pending > 0 {
            Some(Standing::Pending)
        } else if self.passed > 0 {
            Some(Standing::Passed)
        } else {
            None
        }
    }
}

/// A pull request or an issue read whole: what its row tells, how it
/// stands, its body and everything said on it, oldest first, and the images
/// they show.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub item: Item,
    /// `open`, `closed`, `merged` or `draft`.
    pub state: String,
    pub body: Post,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub said: Vec<Post>,
    /// The images the posts show that were fetched, each by the address a
    /// post writes it with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<Image>,
    /// Of a pull request, the files it changes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileChange>,
    /// Of a pull request, the checks on its head, those that failed first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<Check>,
}

/// Something written on a pull request or an issue: its body, a comment or
/// a review.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Post {
    pub author: String,
    pub did: Did,
    /// When, as GitHub writes a time.
    pub at: String,
    /// Its Markdown, as written.
    pub body: String,
}

/// What a post did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Did {
    #[default]
    Opened,
    Commented,
    Approved,
    ChangesRequested,
    Reviewed,
    Dismissed,
}

impl Did {
    pub fn words(self) -> &'static str {
        match self {
            Did::Opened => "opened",
            Did::Commented => "commented",
            Did::Approved => "approved",
            Did::ChangesRequested => "requested changes",
            Did::Reviewed => "reviewed",
            Did::Dismissed => "reviewed, since dismissed",
        }
    }
}

/// An image a post shows, fetched: its type and its bytes in base64.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    pub address: String,
    pub mime: String,
    pub data: String,
}

/// A file a pull request changes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    /// `added`, `removed`, `modified`, `renamed` or another word GitHub uses.
    pub status: String,
    /// The path it had, when it was renamed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    pub additions: u64,
    pub deletions: u64,
    /// Its unified diff's hunks, none when GitHub gives none: a binary file,
    /// or one too large.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
}

/// A check run on a pull request's head, or a status set on it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    /// What ran it: an app's name, or empty for a status.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub by: String,
    pub outcome: Outcome,
    /// Where it tells more, on GitHub or elsewhere.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link: String,
    /// What it said of itself, in a line.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub said: String,
}

/// How a check ended, or that it has not.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Failed,
    #[default]
    Pending,
    Passed,
    Skipped,
}

/// What came of reading one listing: what is open, or why `gh` could not
/// say.
pub type Read = Result<Vec<Item>, String>;

/// A repository on GitHub and what was last read of it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    /// `owner/name`.
    pub slug: String,
    /// None until it is first read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pulls: Option<Read>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issues: Option<Read>,
    /// How many are open, as GitHub counted them; read whether or not a
    /// listing is, and none until first read or while GitHub cannot say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<Counts>,
}

/// How many pull requests and issues a repository has open.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub pulls: u64,
    pub issues: u64,
}

impl Counts {
    pub fn of(self, listing: Listing) -> u64 {
        match listing {
            Listing::Pulls => self.pulls,
            Listing::Issues => self.issues,
        }
    }
}

impl Repository {
    /// How many of `listing` are open: as GitHub counted them, which goes
    /// past what a listing reads, or else as many as it read.
    pub fn count(&self, listing: Listing) -> Option<u64> {
        self.open.map(|open| open.of(listing)).or_else(|| {
            let read = self.read(listing)?.as_ref().ok()?;
            Some(read.len() as u64)
        })
    }

    pub fn new(slug: impl Into<String>) -> Self {
        Repository {
            slug: slug.into(),
            ..Default::default()
        }
    }

    /// What was last read of `listing`.
    pub fn read(&self, listing: Listing) -> Option<&Read> {
        match listing {
            Listing::Pulls => self.pulls.as_ref(),
            Listing::Issues => self.issues.as_ref(),
        }
    }

    pub fn read_mut(&mut self, listing: Listing) -> &mut Option<Read> {
        match listing {
            Listing::Pulls => &mut self.pulls,
            Listing::Issues => &mut self.issues,
        }
    }

    /// The item numbered `number` in `listing`, as last read.
    pub fn item(&self, listing: Listing, number: u64) -> Option<&Item> {
        self.read(listing)?
            .as_ref()
            .ok()?
            .iter()
            .find(|i| i.number == number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(number: u64) -> Item {
        Item {
            number,
            title: format!("#{number}"),
            url: format!("https://github.com/o/r/issues/{number}"),
            author: "someone".into(),
            draft: false,
            ..Default::default()
        }
    }

    #[test]
    fn an_item_is_found_by_its_number_in_its_own_listing() {
        let mut repository = Repository::new("o/r");
        repository.issues = Some(Ok(vec![item(1), item(2)]));
        assert_eq!(repository.item(Listing::Issues, 2), Some(&item(2)));
        assert_eq!(repository.item(Listing::Pulls, 2), None);
        assert_eq!(repository.item(Listing::Issues, 3), None);
    }

    #[test]
    fn checks_stand_failed_once_any_failed_pending_while_any_runs_else_passed() {
        let checks = |passed, failed, pending| Checks {
            passed,
            failed,
            pending,
            failing: Vec::new(),
        };
        assert_eq!(checks(3, 1, 1).standing(), Some(Standing::Failed));
        assert_eq!(checks(3, 0, 1).standing(), Some(Standing::Pending));
        assert_eq!(checks(3, 0, 0).standing(), Some(Standing::Passed));
        assert_eq!(checks(0, 0, 0).standing(), None);
    }

    #[test]
    fn a_listing_counts_as_github_counted_it_else_as_many_as_were_read() {
        let mut repository = Repository::new("o/r");
        assert_eq!(repository.count(Listing::Pulls), None, "nothing read");
        repository.issues = Some(Ok(vec![item(1), item(2)]));
        assert_eq!(repository.count(Listing::Issues), Some(2));
        repository.pulls = Some(Err("gh is not logged in".into()));
        assert_eq!(repository.count(Listing::Pulls), None, "failed");
        repository.open = Some(Counts {
            pulls: 0,
            issues: 41,
        });
        assert_eq!(repository.count(Listing::Issues), Some(41));
        assert_eq!(repository.count(Listing::Pulls), Some(0));
    }

    #[test]
    fn a_listing_that_failed_holds_no_item() {
        let mut repository = Repository::new("o/r");
        repository.pulls = Some(Err("gh is not logged in".into()));
        assert_eq!(repository.item(Listing::Pulls, 1), None);
    }
}
