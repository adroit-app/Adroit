//! What GitHub holds open of a repository, asked of `gh`, which keeps the
//! login: Adroit holds no token of its own.

use std::path::Path;

use adroit_model::github::{Item, Listing, Read};
use serde::Deserialize;

use crate::{Error, run};

/// The most items of a listing read.
pub const LIMIT: usize = 30;

/// A remote of a repository that is on GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    /// The remote's name, as git calls it.
    pub name: String,
    /// `owner/name` on GitHub.
    pub slug: String,
}

/// Whether `gh` can be run.
pub async fn installed() -> bool {
    run("gh", &["--version"], None).await.is_ok()
}

/// The remote of the repository at `root` that is on GitHub, if one is.
pub async fn remote(root: &Path) -> Option<Remote> {
    let listed = match run("git", &["remote", "-v"], Some(root)).await {
        Ok(listed) => listed,
        Err(_) => run("jj", &["git", "remote", "list"], Some(root))
            .await
            .ok()?,
    };
    choose(&remotes(&listed))
}

/// What is open of `listing` in the repository `slug`, asked in `root`.
pub async fn list(root: &Path, slug: &str, listing: Listing) -> Read {
    let limit = LIMIT.to_string();
    let (command, fields) = match listing {
        Listing::Pulls => ("pr", "number,title,url,author,isDraft"),
        Listing::Issues => ("issue", "number,title,url,author"),
    };
    let args = [
        command, "list", "--repo", slug, "--state", "open", "--limit", &limit, "--json", fields,
    ];
    match run("gh", &args, Some(root)).await {
        Ok(out) => items(&out),
        Err(Error::Failed { stderr, .. }) => Err(reason(&stderr)),
        Err(e) => Err(e.to_string()),
    }
}

/// Each remote `git remote -v` or `jj git remote list` names, by its name
/// and its URL, once.
fn remotes(listed: &str) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for line in listed.lines() {
        let mut words = line.split_whitespace();
        let (Some(name), Some(url)) = (words.next(), words.next()) else {
            continue;
        };
        if !found.iter().any(|(n, _)| n == name) {
            found.push((name.into(), url.into()));
        }
    }
    found
}

/// The remote on GitHub that pull requests and issues are read of: as `gh`
/// picks it, `upstream` before `github` before `origin` before any other.
fn choose(remotes: &[(String, String)]) -> Option<Remote> {
    let rank = |name: &str| match name {
        "upstream" => 0,
        "github" => 1,
        "origin" => 2,
        _ => 3,
    };
    remotes
        .iter()
        .filter_map(|(name, url)| {
            Some(Remote {
                name: name.clone(),
                slug: slug_of(url)?,
            })
        })
        .min_by_key(|r| rank(&r.name))
}

/// `owner/name` of a URL of a repository on github.com, in any form git
/// takes it.
fn slug_of(url: &str) -> Option<String> {
    let rest = ["https://", "http://", "ssh://", "git://", "git+ssh://"]
        .iter()
        .find_map(|scheme| url.strip_prefix(scheme))
        .map(|rest| rest.split_once('@').map_or(rest, |(_, host)| host))
        .and_then(|rest| {
            rest.strip_prefix("github.com")
                .map(|p| p.trim_start_matches(':'))
        })
        .and_then(|rest| {
            // A port, as in `ssh://git@github.com:22/owner/name`.
            let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
            rest.strip_prefix('/')
        })
        .or_else(|| url.strip_prefix("git@github.com:"))?;
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let mut parts = rest.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(name), None) if !owner.is_empty() && !name.is_empty() => {
            Some(format!("{owner}/{name}"))
        }
        _ => None,
    }
}

/// The items `gh … list --json` printed.
fn items(out: &str) -> Read {
    #[derive(Deserialize)]
    struct Author {
        login: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Listed {
        number: u64,
        title: String,
        url: String,
        author: Option<Author>,
        #[serde(default)]
        is_draft: bool,
    }
    let listed: Vec<Listed> =
        serde_json::from_str(out).map_err(|e| format!("gh printed what cannot be read: {e}"))?;
    Ok(listed
        .into_iter()
        .map(|l| Item {
            number: l.number,
            title: l.title,
            url: l.url,
            author: l.author.map(|a| a.login).unwrap_or_default(),
            draft: l.is_draft,
        })
        .collect())
}

/// Why `gh` could not answer, from what it wrote: in a word where it is
/// known, else its first line.
fn reason(stderr: &str) -> String {
    if stderr.contains("gh auth login") {
        return "gh is not logged in".into();
    }
    stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("gh failed")
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_github_url_in_any_form_git_takes_names_its_repository() {
        let cases = [
            ("git@github.com:o/r.git", Some("o/r")),
            ("git@github.com:o/r", Some("o/r")),
            ("https://github.com/o/r.git", Some("o/r")),
            ("https://github.com/o/r", Some("o/r")),
            ("https://github.com/o/r/", Some("o/r")),
            ("https://user@github.com/o/r.git", Some("o/r")),
            ("ssh://git@github.com/o/r.git", Some("o/r")),
            ("ssh://git@github.com:22/o/r.git", Some("o/r")),
            ("git://github.com/o/r.git", Some("o/r")),
            ("https://gitlab.com/o/r.git", None),
            ("git@gitlab.com:o/r.git", None),
            ("https://github.com.evil.example/o/r", None),
            ("https://github.com/o", None),
            ("https://github.com/o/r/extra", None),
            ("/local/path/r.git", None),
        ];
        for (url, want) in cases {
            assert_eq!(slug_of(url).as_deref(), want, "{url}");
        }
    }

    #[test]
    fn remotes_are_read_once_each_from_git_s_and_jj_s_listings() {
        let git = "origin\tgit@github.com:o/r.git (fetch)\norigin\tgit@github.com:o/r.git (push)\n";
        assert_eq!(
            remotes(git),
            [("origin".into(), "git@github.com:o/r.git".into())]
        );
        let jj = "origin https://github.com/o/r.git\nupstream https://github.com/u/r.git\n";
        assert_eq!(remotes(jj).len(), 2);
    }

    #[test]
    fn the_remote_read_is_chosen_as_gh_chooses_it() {
        let named = |names: &[(&str, &str)]| {
            names
                .iter()
                .map(|(n, u)| (n.to_string(), u.to_string()))
                .collect::<Vec<_>>()
        };
        let fork = named(&[
            ("origin", "git@github.com:me/r.git"),
            ("upstream", "git@github.com:them/r.git"),
        ]);
        assert_eq!(choose(&fork).map(|r| r.slug).as_deref(), Some("them/r"));
        let elsewhere = named(&[
            ("origin", "git@gitlab.com:me/r.git"),
            ("mirror", "https://github.com/me/r"),
        ]);
        assert_eq!(
            choose(&elsewhere),
            Some(Remote {
                name: "mirror".into(),
                slug: "me/r".into()
            })
        );
        assert_eq!(choose(&named(&[("origin", "/srv/r.git")])), None);
    }

    #[test]
    fn listed_items_carry_their_number_title_page_author_and_draft() {
        let out = r#"[
            {"number": 2, "title": "Draft", "url": "https://github.com/o/r/pull/2",
             "author": {"login": "me", "is_bot": false}, "isDraft": true},
            {"number": 1, "title": "Ghost", "url": "https://github.com/o/r/issues/1",
             "author": null}
        ]"#;
        let read = items(out).unwrap();
        assert_eq!(read[0].number, 2);
        assert_eq!(read[0].author, "me");
        assert!(read[0].draft);
        assert_eq!(read[1].author, "");
        assert!(!read[1].draft);
    }

    #[test]
    fn output_that_is_not_a_listing_is_a_failed_read() {
        assert!(items("not json").is_err());
    }

    #[test]
    fn a_login_gh_lacks_is_told_in_a_word_and_any_other_failure_by_its_first_line() {
        assert_eq!(
            reason("To get started with GitHub CLI, please run:  gh auth login\n"),
            "gh is not logged in"
        );
        assert_eq!(
            reason("\nerror connecting to api.github.com\ncheck your internet connection\n"),
            "error connecting to api.github.com"
        );
        assert_eq!(reason(""), "gh failed");
    }
}
