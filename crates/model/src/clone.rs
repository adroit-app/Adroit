//! A repository named to be cloned with ghq: what was typed, handed to
//! `ghq get` as it is, and where ghq keeps it once cloned.

use std::path::Path;

/// A repository typed as `owner/repo`, `host/owner/repo`, or a URL git
/// clones from.
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
            [owner, repo] if !addressed => format!("github.com/{owner}/{repo}"),
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
    fn an_owner_and_a_repository_are_on_github() {
        assert_eq!(path("owner/repo").as_deref(), Some("github.com/owner/repo"));
        assert_eq!(path(" a-b/c.d_e ").as_deref(), Some("github.com/a-b/c.d_e"));
    }

    #[test]
    fn a_host_before_them_names_where_they_are() {
        assert_eq!(
            path("gitlab.com/group/sub/repo").as_deref(),
            Some("gitlab.com/group/sub/repo")
        );
        assert_eq!(path("github.com/o/r").as_deref(), Some("github.com/o/r"));
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
            "owner/repo extra",
            "a/b/c",
            "~/src/repo",
            "/abs/path",
            "./o/r",
            "o/.r",
            "-o/r",
            "https://-x/o/r",
            "o/r?x",
            "file:///o/r",
            "https://github.com",
            "@w",
        ] {
            assert_eq!(path(typed), None, "{typed}");
        }
    }

    #[test]
    fn a_kept_repository_is_found_by_its_last_components_whatever_their_case() {
        let target = CloneTarget::parse("Adroit-App/adroit").unwrap();
        let kept = [
            PathBuf::from("/g/github.com/other/adroit"),
            PathBuf::from("/g/github.com/adroit-app/Adroit"),
        ];
        assert_eq!(target.kept_in(&kept), Some(&kept[1]));
        assert_eq!(
            CloneTarget::parse("o/new").unwrap().kept_in(&kept),
            None::<&PathBuf>
        );
    }
}
