//! A repository named to be cloned with ghq: what was typed, handed to
//! `ghq get` as it is, and where ghq keeps it once cloned.

use std::path::Path;

/// A repository typed as `host/owner/repo` or a URL git clones from, or
/// chosen from an owner's on GitHub.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CloneTarget {
    /// What was typed, which `ghq get` is given unchanged so that the URL's
    /// scheme picks how it is cloned.
    pub typed: String,
    /// The repository's place under a ghq root: its host, then its path.
    pub path: String,
}

impl CloneTarget {
    /// The repository `typed` names, if it names one.
    pub fn parse(typed: &str) -> Option<Self> {
        let typed = typed.trim();
        if typed.is_empty() || typed.chars().any(char::is_whitespace) {
            return None;
        }
        let (rest, addressed) = match typed.split_once("://") {
            Some((scheme, rest)) => {
                if !matches!(scheme, "https" | "http" | "ssh" | "git") {
                    return None;
                }
                (strip_user(rest).to_string(), true)
            }
            None => match scp(typed) {
                Some(rest) => (rest, true),
                None => (typed.to_string(), false),
            },
        };
        let rest = rest.trim_end_matches('/');
        let rest = rest.strip_suffix(".git").unwrap_or(rest);
        let mut parts: Vec<&str> = rest.split('/').collect();
        if addressed {
            let host = parts.first_mut()?;
            *host = host.split(':').next().unwrap_or_default();
        }
        let path = match parts.as_slice() {
            [host, _, _, ..] if host.contains('.') || addressed => parts.join("/"),
            _ => return None,
        };
        let segments: Vec<&str> = path.split('/').collect();
        if !segments.iter().all(|s| segment(s)) {
            return None;
        }
        Some(Self {
            typed: typed.to_string(),
            path,
        })
    }

    /// The directory of `kept` that is this repository, matched by its last
    /// components without regard to case, as the hosts ghq clones from name
    /// a repository.
    pub fn kept_in<'a, P: AsRef<Path>>(&self, kept: &'a [P]) -> Option<&'a P> {
        let wanted: Vec<String> = self.path.split('/').map(str::to_lowercase).collect();
        kept.iter().find(|dir| {
            let tail: Vec<String> = dir
                .as_ref()
                .components()
                .rev()
                .take(wanted.len())
                .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
                .collect();
            tail.iter().rev().eq(wanted.iter())
        })
    }
}

/// What is typed after `github:` to look through GitHub: the start of an
/// owner's name, or an owner, a slash and the start of one of its
/// repositories' names. Either start may be nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHubTyped {
    Owner(String),
    Repository { owner: String, name: String },
}

impl GitHubTyped {
    pub const PREFIX: &str = "github:";

    /// What `typed` asks of GitHub, if it starts with [`Self::PREFIX`].
    pub fn parse(typed: &str) -> Option<Self> {
        let rest = typed.trim().strip_prefix(Self::PREFIX)?;
        if rest.chars().any(char::is_whitespace) {
            return None;
        }
        let started = |s: &str| s.is_empty() || segment(s);
        match rest.split_once('/') {
            None => login(rest).then(|| Self::Owner(rest.to_string())),
            Some((owner, name)) => {
                (!owner.is_empty() && login(owner) && !name.contains('/') && started(name)).then(
                    || Self::Repository {
                        owner: owner.to_string(),
                        name: name.to_string(),
                    },
                )
            }
        }
    }

    /// What is typed to look through `owner`'s repositories.
    pub fn owner_query(owner: &str) -> String {
        format!("{}{owner}/", Self::PREFIX)
    }

    /// `owner`'s repository `name`, to clone.
    pub fn target(owner: &str, name: &str) -> CloneTarget {
        CloneTarget {
            typed: format!("github.com/{owner}/{name}"),
            path: format!("github.com/{owner}/{name}"),
        }
    }
}

/// Whether `s` is, or starts, a name GitHub gives an owner: letters, digits
/// and dashes, not starting with a dash.
fn login(s: &str) -> bool {
    !s.starts_with('-') && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// The address of an scp-like `user@host:path`, as `host/path`.
fn scp(typed: &str) -> Option<String> {
    let (user_host, path) = typed.split_once(':')?;
    let (_, host) = user_host.split_once('@')?;
    (!host.is_empty() && !host.contains('/') && !path.starts_with('/'))
        .then(|| format!("{host}/{path}"))
}

fn strip_user(rest: &str) -> &str {
    let host_end = rest.find('/').unwrap_or(rest.len());
    match rest[..host_end].rfind('@') {
        Some(at) => &rest[at + 1..],
        None => rest,
    }
}

fn segment(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with(['.', '-'])
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn path(typed: &str) -> Option<String> {
        CloneTarget::parse(typed).map(|t| t.path)
    }

    #[test]
    fn a_host_before_them_names_where_they_are() {
        assert_eq!(
            path("gitlab.com/group/sub/repo").as_deref(),
            Some("gitlab.com/group/sub/repo")
        );
        assert_eq!(
            path(" github.com/a-b/c.d_e ").as_deref(),
            Some("github.com/a-b/c.d_e")
        );
    }

    #[test]
    fn a_url_is_read_without_its_scheme_user_port_and_git_suffix() {
        for typed in [
            "https://github.com/o/r",
            "https://github.com/o/r.git",
            "https://github.com/o/r/",
            "ssh://git@github.com:22/o/r.git",
            "git@github.com:o/r.git",
            "git://github.com/o/r",
        ] {
            assert_eq!(path(typed).as_deref(), Some("github.com/o/r"), "{typed}");
        }
    }

    #[test]
    fn what_is_typed_is_kept_as_typed_for_ghq() {
        let target = CloneTarget::parse(" git@github.com:o/r.git ").unwrap();
        assert_eq!(target.typed, "git@github.com:o/r.git");
    }

    #[test]
    fn words_paths_and_other_schemes_name_no_repository() {
        for typed in [
            "",
            "repo",
            "owner/repo",
            "github.com/owner/repo extra",
            "a/b/c",
            "~/src/repo",
            "/abs/path",
            "./o/r",
            "x.y/o/.r",
            "x.y/-o/r",
            "https://-x/o/r",
            "x.y/o/r?x",
            "file:///o/r",
            "https://github.com",
            "@w",
        ] {
            assert_eq!(path(typed), None, "{typed}");
        }
    }

    #[test]
    fn github_and_a_colon_start_an_owner_and_a_slash_after_it_its_repositories() {
        let owner = |o: &str| Some(GitHubTyped::Owner(o.into()));
        let repository = |o: &str, n: &str| {
            Some(GitHubTyped::Repository {
                owner: o.into(),
                name: n.into(),
            })
        };
        assert_eq!(GitHubTyped::parse("github:"), owner(""));
        assert_eq!(GitHubTyped::parse(" github:adr "), owner("adr"));
        assert_eq!(
            GitHubTyped::parse("github:adroit-app/"),
            repository("adroit-app", "")
        );
        assert_eq!(
            GitHubTyped::parse("github:adroit-app/ad"),
            repository("adroit-app", "ad")
        );
    }

    #[test]
    fn without_github_and_a_colon_or_past_a_name_github_gives_nothing_is_asked_of_it() {
        for typed in [
            "",
            "adroit-app/",
            "github.com/o/",
            "github",
            "github:/",
            "github:-o",
            "github:o.x",
            "github:o/r/x",
            "github:o/.r",
            "github:o/r x",
        ] {
            assert_eq!(GitHubTyped::parse(typed), None, "{typed}");
        }
    }

    #[test]
    fn an_owners_repository_is_what_ghq_clones_and_an_owner_what_looks_through_them() {
        let target = GitHubTyped::target("o", "r");
        assert_eq!(target.path, "github.com/o/r");
        assert_eq!(
            CloneTarget::parse(&target.typed),
            Some(target),
            "the host parses what it is asked to clone again"
        );
        assert_eq!(GitHubTyped::owner_query("o"), "github:o/");
    }

    #[test]
    fn a_kept_repository_is_found_by_its_last_components_whatever_their_case() {
        let target = GitHubTyped::target("Adroit-App", "adroit");
        let kept = [
            PathBuf::from("/g/github.com/other/adroit"),
            PathBuf::from("/g/github.com/adroit-app/Adroit"),
        ];
        assert_eq!(target.kept_in(&kept), Some(&kept[1]));
        assert_eq!(
            GitHubTyped::target("o", "new").kept_in(&kept),
            None::<&PathBuf>
        );
    }
}
