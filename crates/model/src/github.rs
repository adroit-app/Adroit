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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}

impl Repository {
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
    fn a_listing_that_failed_holds_no_item() {
        let mut repository = Repository::new("o/r");
        repository.pulls = Some(Err("gh is not logged in".into()));
        assert_eq!(repository.item(Listing::Pulls, 1), None);
    }
}
