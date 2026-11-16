//! What GitHub holds open of a repository, asked of `gh`, which keeps the
//! login: Adroit holds no token of its own.

use std::path::{Path, PathBuf};

use adroit_model::github::{Checks, Counts, Item, Label, Listing, Pull, Read, Review};
use serde::Deserialize;

use crate::{Backend, Error, run};

mod thread;
pub use thread::*;

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
        Listing::Pulls => (
            "pr",
            "number,title,url,author,isDraft,createdAt,updatedAt,labels,assignees,milestone,headRefName,baseRefName,additions,deletions,changedFiles,reviewDecision,reviewRequests,statusCheckRollup",
        ),
        Listing::Issues => (
            "issue",
            "number,title,url,author,createdAt,updatedAt,labels,assignees,milestone",
        ),
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

/// How many pull requests and issues the repository `slug` has open, asked
/// in `root`: in one query, as GitHub counts them, past the most a listing
/// reads.
pub async fn count(root: &Path, slug: &str) -> Result<Counts, String> {
    let (owner, name) = slug
        .split_once('/')
        .ok_or_else(|| format!("{slug} names no repository on GitHub"))?;
    let query = "query=query($owner:String!,$name:String!){repository(owner:$owner,name:$name){pullRequests(states:OPEN){totalCount} issues(states:OPEN){totalCount}}}";
    let owner = format!("owner={owner}");
    let name = format!("name={name}");
    let args = ["api", "graphql", "-f", query, "-F", &owner, "-F", &name];
    match run("gh", &args, Some(root)).await {
        Ok(out) => counted(&out),
        Err(Error::Failed { stderr, .. }) => Err(reason(&stderr)),
        Err(e) => Err(e.to_string()),
    }
}

/// The most repositories of an owner read.
pub const OWNED_LIMIT: usize = 1000;

/// The names of `owner`'s repositories, as many as [`OWNED_LIMIT`], those
/// pushed to last first.
pub async fn repositories(owner: &str) -> Result<Vec<String>, String> {
    let limit = OWNED_LIMIT.to_string();
    let args = ["repo", "list", owner, "--limit", &limit, "--json", "name"];
    match run("gh", &args, None).await {
        Ok(out) => named(&out),
        Err(Error::Failed { stderr, .. }) => Err(reason(&stderr)),
        Err(e) => Err(e.to_string()),
    }
}

/// The names `gh repo list --json name` printed.
fn named(out: &str) -> Result<Vec<String>, String> {
    #[derive(Deserialize)]
    struct Named {
        name: String,
    }
    let printed: Vec<Named> =
        serde_json::from_str(out).map_err(|e| format!("gh printed what cannot be read: {e}"))?;
    Ok(printed.into_iter().map(|n| n.name).collect())
}

/// The owners `gh` is logged in as: the user, then the organizations they
/// belong to.
pub async fn accounts() -> Result<Vec<String>, String> {
    let ask = |args: &'static [&'static str]| async move {
        match run("gh", args, None).await {
            Ok(out) => Ok(out),
            Err(Error::Failed { stderr, .. }) => Err(reason(&stderr)),
            Err(e) => Err(e.to_string()),
        }
    };
    let (user, orgs) = tokio::join!(
        ask(&["api", "user", "--jq", ".login"]),
        ask(&["api", "--paginate", "user/orgs", "--jq", ".[].login"]),
    );
    Ok(logins(&user?, &orgs.unwrap_or_default()))
}

/// The logins `gh api --jq` printed, one a line, of the user and of the
/// organizations, each once.
fn logins(user: &str, orgs: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for login in user.lines().take(1).chain(orgs.lines()).map(str::trim) {
        if !login.is_empty() && !found.iter().any(|f| f == login) {
            found.push(login.to_string());
        }
    }
    found
}

/// The counts `gh api graphql` printed for [`count`]'s query.
fn counted(out: &str) -> Result<Counts, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Total {
        total_count: u64,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Open {
        pull_requests: Total,
        issues: Total,
    }
    #[derive(Deserialize)]
    struct Data {
        repository: Open,
    }
    #[derive(Deserialize)]
    struct Printed {
        data: Data,
    }
    let printed: Printed =
        serde_json::from_str(out).map_err(|e| format!("gh printed what cannot be read: {e}"))?;
    let open = printed.data.repository;
    Ok(Counts {
        pulls: open.pull_requests.total_count,
        issues: open.issues.total_count,
    })
}

/// A new worktree of the repository at `root` for its pull request `number`
/// on GitHub: on the pull request's own branch when it comes from the
/// repository itself, or else, as for a fork's, at its head, named after
/// it; `title` describes the change a jj workspace starts.
pub async fn pull_worktree(root: &Path, number: u64, title: &str) -> Result<PathBuf, String> {
    let remote = remote(root)
        .await
        .ok_or("the repository has no remote on GitHub")?;
    if let Some(branch) = own_branch(root, &remote.slug, number).await
        && fetch_branch(root, &remote.name, &branch).await.is_ok()
        && let Ok(made) =
            crate::worktree::create_on_branch(root, &branch, &remote.name, title).await
    {
        return Ok(made.path);
    }
    let head = fetch_pull(root, &remote.name, number).await?;
    crate::worktree::create(root, &format!("pr-{number}"), Some(&head), title)
        .await
        .map(|w| w.path)
}

/// The branch the pull request `number` of the repository `slug` comes
/// from, when it comes from that repository rather than a fork.
async fn own_branch(root: &Path, slug: &str, number: u64) -> Option<String> {
    let path = format!("repos/{slug}/pulls/{number}");
    let out = run("gh", &["api", &path], Some(root)).await.ok()?;
    branch_of(slug, &out)
}

/// The branch of the pull request GitHub's API answered, when its head is
/// in `slug` itself.
fn branch_of(slug: &str, out: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Repo {
        full_name: String,
    }
    #[derive(Deserialize)]
    struct Head {
        #[serde(rename = "ref")]
        name: String,
        repo: Option<Repo>,
    }
    #[derive(Deserialize)]
    struct Pull {
        head: Head,
    }
    let pull: Pull = serde_json::from_str(out).ok()?;
    pull.head
        .repo
        .is_some_and(|r| r.full_name.eq_ignore_ascii_case(slug))
        .then_some(pull.head.name)
}

/// The branch `branch` of `remote` fetched into the repository at `root`,
/// where jj sees it too once imported.
async fn fetch_branch(root: &Path, remote: &str, branch: &str) -> Result<(), String> {
    let spec = format!("+refs/heads/{branch}:refs/remotes/{remote}/{branch}");
    let failed = |e: Error| e.to_string();
    run("git", &["fetch", "--quiet", remote, &spec], Some(root))
        .await
        .map_err(failed)?;
    if Backend::of(root) == Some(Backend::Jj) {
        run("jj", &["git", "import"], Some(root))
            .await
            .map_err(failed)?;
    }
    Ok(())
}

/// The commit the pull request `number` ends at on `remote`, fetched into
/// the repository at `root`. GitHub keeps it as `refs/pull/<number>/head`,
/// which a fork's pull request has too; it is kept as the remote's
/// `pull/<number>`, where jj, which reads only branches and tags, sees it
/// once imported.
async fn fetch_pull(root: &Path, remote: &str, number: u64) -> Result<String, String> {
    let tracking = format!("refs/remotes/{remote}/pull/{number}");
    let spec = format!("+refs/pull/{number}/head:{tracking}");
    let failed = |e: Error| e.to_string();
    run("git", &["fetch", "--quiet", remote, &spec], Some(root))
        .await
        .map_err(failed)?;
    if Backend::of(root) == Some(Backend::Jj) {
        run("jj", &["git", "import"], Some(root))
            .await
            .map_err(failed)?;
    }
    let commit = run(
        "git",
        &["rev-parse", "--verify", &format!("{tracking}^{{commit}}")],
        Some(root),
    )
    .await
    .map_err(failed)?;
    Ok(commit.trim().into())
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
    struct Login {
        login: String,
    }
    #[derive(Deserialize)]
    struct Titled {
        title: String,
    }
    #[derive(Deserialize)]
    struct Requested {
        login: Option<String>,
        name: Option<String>,
    }
    #[derive(Deserialize)]
    struct Check {
        name: Option<String>,
        context: Option<String>,
        status: Option<String>,
        conclusion: Option<String>,
        state: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Listed {
        number: u64,
        title: String,
        url: String,
        author: Option<Login>,
        #[serde(default)]
        is_draft: bool,
        #[serde(default)]
        created_at: String,
        #[serde(default)]
        updated_at: String,
        #[serde(default)]
        labels: Vec<Label>,
        #[serde(default)]
        assignees: Vec<Login>,
        milestone: Option<Titled>,
        head_ref_name: Option<String>,
        #[serde(default)]
        base_ref_name: String,
        #[serde(default)]
        additions: u64,
        #[serde(default)]
        deletions: u64,
        #[serde(default)]
        changed_files: u64,
        review_decision: Option<String>,
        #[serde(default)]
        review_requests: Vec<Requested>,
        #[serde(default)]
        status_check_rollup: Vec<Check>,
    }
    let listed: Vec<Listed> =
        serde_json::from_str(out).map_err(|e| format!("gh printed what cannot be read: {e}"))?;
    let checks = |rollup: Vec<Check>| {
        let mut checks = Checks::default();
        for check in rollup {
            // A check run ends in a conclusion; a commit status has a state.
            let ended = match check.status.as_deref() {
                None | Some("") => check.state.unwrap_or_default(),
                Some("COMPLETED") => check.conclusion.unwrap_or_default(),
                Some(_) => "PENDING".into(),
            };
            match ended.as_str() {
                "SUCCESS" | "NEUTRAL" | "SKIPPED" => checks.passed += 1,
                "PENDING" | "EXPECTED" | "QUEUED" | "IN_PROGRESS" | "WAITING" | "" => {
                    checks.pending += 1
                }
                _ => {
                    checks.failed += 1;
                    let name = check
                        .name
                        .filter(|n| !n.is_empty())
                        .or(check.context)
                        .unwrap_or_default();
                    checks.failing.push(name);
                }
            }
        }
        checks
    };
    Ok(listed
        .into_iter()
        .map(|l| Item {
            number: l.number,
            title: l.title,
            url: l.url,
            author: l.author.map(|a| a.login).unwrap_or_default(),
            draft: l.is_draft,
            created_at: l.created_at,
            updated_at: l.updated_at,
            labels: l.labels,
            assignees: l.assignees.into_iter().map(|a| a.login).collect(),
            milestone: l.milestone.map(|m| m.title),
            pull: l.head_ref_name.map(|head| Pull {
                head,
                base: l.base_ref_name,
                additions: l.additions,
                deletions: l.deletions,
                changed_files: l.changed_files,
                review: match l.review_decision.as_deref() {
                    Some("REVIEW_REQUIRED") => Some(Review::Required),
                    Some("CHANGES_REQUESTED") => Some(Review::ChangesRequested),
                    Some("APPROVED") => Some(Review::Approved),
                    _ => None,
                },
                requested: l
                    .review_requests
                    .into_iter()
                    .filter_map(|r| r.login.or(r.name))
                    .collect(),
                checks: checks(l.status_check_rollup),
            }),
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

    fn sh(dir: &Path, program: &str, args: &[&str]) -> String {
        let output = std::process::Command::new(program)
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .env("JJ_USER", "Test")
            .env("JJ_EMAIL", "test@example.invalid")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    #[test]
    fn an_owners_repositories_are_read_by_name_in_the_order_gh_lists_them() {
        assert_eq!(
            named(r#"[{"name":"adroit"},{"name":"vim-x"}]"#),
            Ok(vec!["adroit".to_string(), "vim-x".to_string()])
        );
        assert!(named("not json").is_err());
    }

    #[test]
    fn the_owners_logged_in_as_are_the_user_then_each_organization_once() {
        assert_eq!(
            logins("me\n", "org\n\nother\norg\n"),
            ["me", "org", "other"]
        );
    }

    #[test]
    fn a_pull_request_has_a_branch_of_its_own_only_when_it_comes_from_the_repository() {
        let from = |repo: &str| {
            format!(
                r#"{{"head": {{"ref": "fix/parser", "repo": {repo}}}, "base": {{"ref": "main"}}}}"#
            )
        };
        assert_eq!(
            branch_of("o/r", &from(r#"{"full_name": "O/r"}"#)).as_deref(),
            Some("fix/parser")
        );
        assert_eq!(branch_of("o/r", &from(r#"{"full_name": "fork/r"}"#)), None);
        assert_eq!(branch_of("o/r", &from("null")), None);
    }

    #[tokio::test]
    async fn a_git_worktree_on_a_pull_request_s_branch_checks_it_out_tracking_the_remote() {
        let dir = tempfile::tempdir().unwrap();
        let (clone, head) = cloned_with_a_pull(dir.path());
        fetch_branch(&clone, "origin", "fix").await.unwrap();
        let made = crate::worktree::create_on_branch(&clone, "fix", "origin", "t")
            .await
            .unwrap();
        assert_eq!(made.base, head);
        let on = sh(&made.path, "git", &["rev-parse", "--abbrev-ref", "HEAD"]);
        assert_eq!(on.trim(), "fix");
        let upstream = sh(
            &made.path,
            "git",
            &["rev-parse", "--abbrev-ref", "@{upstream}"],
        );
        assert_eq!(upstream.trim(), "origin/fix");
    }

    #[tokio::test]
    #[cfg_attr(windows, ignore = "runs jj")]
    async fn a_jj_workspace_on_a_pull_request_s_branch_tracks_its_bookmark_and_starts_on_it() {
        let dir = tempfile::tempdir().unwrap();
        let (clone, head) = cloned_with_a_pull(dir.path());
        sh(&clone, "jj", &["git", "init", "--colocate"]);
        fetch_branch(&clone, "origin", "fix").await.unwrap();
        let made = crate::worktree::create_on_branch(&clone, "fix", "origin", "t")
            .await
            .unwrap();
        let parent = sh(
            &made.path,
            "jj",
            &["log", "--no-graph", "-r", "@-", "-T", "commit_id"],
        );
        assert_eq!(parent.trim(), head);
        let tracked = sh(
            &clone,
            "jj",
            &["bookmark", "list", "--tracked", "-T", "name ++ \"\\n\""],
        );
        assert!(tracked.lines().any(|l| l == "fix"), "{tracked}");
    }

    /// A repository standing for GitHub's, holding pull request 7 as GitHub
    /// keeps it, and a clone of it; the head of the pull request.
    fn cloned_with_a_pull(dir: &Path) -> (std::path::PathBuf, String) {
        let hub = dir.join("hub");
        std::fs::create_dir(&hub).unwrap();
        sh(&hub, "git", &["init", "-q", "-b", "main"]);
        std::fs::write(hub.join("a.txt"), "a").unwrap();
        sh(&hub, "git", &["add", "a.txt"]);
        sh(&hub, "git", &["commit", "-q", "-m", "first"]);
        let clone = dir.join("clone");
        sh(dir, "git", &["clone", "-q", "hub", "clone"]);
        sh(&hub, "git", &["switch", "-q", "-c", "fix"]);
        std::fs::write(hub.join("b.txt"), "b").unwrap();
        sh(&hub, "git", &["add", "b.txt"]);
        sh(&hub, "git", &["commit", "-q", "-m", "fix"]);
        let head = sh(&hub, "git", &["rev-parse", "HEAD"]).trim().to_string();
        sh(&hub, "git", &["update-ref", "refs/pull/7/head", &head]);
        (clone, head)
    }

    #[tokio::test]
    async fn a_pull_request_s_head_is_fetched_as_its_remote_s_pull_ref() {
        let dir = tempfile::tempdir().unwrap();
        let (clone, head) = cloned_with_a_pull(dir.path());
        assert_eq!(fetch_pull(&clone, "origin", 7).await.unwrap(), head);
        let kept = sh(&clone, "git", &["rev-parse", "refs/remotes/origin/pull/7"]);
        assert_eq!(kept.trim(), head);
    }

    #[tokio::test]
    #[cfg_attr(windows, ignore = "runs jj")]
    async fn a_pull_request_s_head_fetched_into_a_jj_repository_is_seen_by_jj() {
        let dir = tempfile::tempdir().unwrap();
        let (clone, head) = cloned_with_a_pull(dir.path());
        sh(&clone, "jj", &["git", "init", "--colocate"]);
        assert_eq!(fetch_pull(&clone, "origin", 7).await.unwrap(), head);
        let seen = sh(
            &clone,
            "jj",
            &["log", "--no-graph", "-r", &head, "-T", "commit_id"],
        );
        assert_eq!(seen.trim(), head);
    }

    #[tokio::test]
    async fn a_pull_request_that_does_not_exist_cannot_be_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let (clone, _) = cloned_with_a_pull(dir.path());
        assert!(fetch_pull(&clone, "origin", 8).await.is_err());
    }

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
    fn a_listed_pull_request_carries_its_labels_branches_review_and_checks() {
        let out = r#"[{
            "number": 7, "title": "Fix", "url": "u", "author": {"login": "me"}, "isDraft": false,
            "createdAt": "2026-11-01T00:00:00Z", "updatedAt": "2026-11-02T00:00:00Z",
            "labels": [{"name": "bug", "color": "d73a4a"}],
            "assignees": [{"login": "you"}], "milestone": {"title": "v1"},
            "headRefName": "fix", "baseRefName": "main",
            "additions": 10, "deletions": 2, "changedFiles": 3,
            "reviewDecision": "CHANGES_REQUESTED",
            "reviewRequests": [{"__typename": "User", "login": "rev"}, {"__typename": "Team", "name": "core"}],
            "statusCheckRollup": [
                {"__typename": "CheckRun", "name": "build", "status": "COMPLETED", "conclusion": "SUCCESS"},
                {"__typename": "CheckRun", "name": "skip", "status": "COMPLETED", "conclusion": "SKIPPED"},
                {"__typename": "CheckRun", "name": "lint", "status": "COMPLETED", "conclusion": "FAILURE"},
                {"__typename": "CheckRun", "name": "test", "status": "IN_PROGRESS", "conclusion": ""},
                {"__typename": "StatusContext", "context": "ci/old", "state": "ERROR"},
                {"__typename": "StatusContext", "context": "ci/wait", "state": "PENDING"}
            ]
        }]"#;
        let item = items(out).unwrap().remove(0);
        assert_eq!(item.created_at, "2026-11-01T00:00:00Z");
        assert_eq!(item.updated_at, "2026-11-02T00:00:00Z");
        assert_eq!(
            item.labels,
            [Label {
                name: "bug".into(),
                color: "d73a4a".into()
            }]
        );
        assert_eq!(item.assignees, ["you"]);
        assert_eq!(item.milestone.as_deref(), Some("v1"));
        assert_eq!(
            item.pull,
            Some(Pull {
                head: "fix".into(),
                base: "main".into(),
                additions: 10,
                deletions: 2,
                changed_files: 3,
                review: Some(Review::ChangesRequested),
                requested: vec!["rev".into(), "core".into()],
                checks: Checks {
                    passed: 2,
                    failed: 2,
                    pending: 2,
                    failing: vec!["lint".into(), "ci/old".into()],
                },
            })
        );
    }

    #[test]
    fn a_listed_issue_has_nothing_of_a_pull_request() {
        let out = r#"[{"number": 1, "title": "t", "url": "u", "author": null, "labels": []}]"#;
        assert_eq!(items(out).unwrap()[0].pull, None);
    }

    #[test]
    fn output_that_is_not_a_listing_is_a_failed_read() {
        assert!(items("not json").is_err());
    }

    #[test]
    fn open_counts_are_read_from_what_gh_api_graphql_printed() {
        let out = r#"{"data":{"repository":{"pullRequests":{"totalCount":1},"issues":{"totalCount":42}}}}"#;
        assert_eq!(
            counted(out),
            Ok(Counts {
                pulls: 1,
                issues: 42
            })
        );
        assert!(counted(r#"{"data":{"repository":null}}"#).is_err());
        assert!(counted("not json").is_err());
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
