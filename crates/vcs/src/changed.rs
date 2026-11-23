//! What a worktree changed over the branch it goes back to: each file and how
//! many lines, without the lines.

use std::collections::HashMap;
use std::path::Path;

use crate::{Backend, Change, Commit, DIFF_BYTES, Error, git, jj, run, run_head};

/// The most of a diff read to count its lines.
const COUNTED_LINES: usize = 500_000;
const COUNTED_BYTES: usize = 32 << 20;

/// How many lines of a file were added and removed; none where they are not
/// counted, as those of a binary file.
pub type Counts = Option<(u64, u64)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    pub backend: Backend,
    /// The revision everything is counted from.
    pub base: String,
    /// The commits or changes since, the newest first.
    pub commits: Vec<Commit>,
    pub files: Vec<(Change, Counts)>,
    /// Whether the diff is too long for every file's lines to be counted.
    pub cut: bool,
}

/// The lines each file of the diff `text`, in git's form, adds and removes,
/// by the path it has after.
pub fn parse_counts(text: &str) -> HashMap<String, Counts> {
    counted(text, false)
}

/// Whether `text`, read up to so many lines and bytes, may have been cut there.
fn reached(text: &str, lines: usize, bytes: usize) -> bool {
    text.len() >= bytes || text.lines().count() >= lines
}

/// [`parse_counts`] of a diff that may be `cut`, whose last file is then not
/// counted.
fn counted(text: &str, cut: bool) -> HashMap<String, Counts> {
    let mut out: HashMap<String, Counts> = HashMap::new();
    let mut path: Option<String> = None;
    // How many lines the hunk read still holds, before and after: within it, a
    // line that looks like a header is none.
    let mut left = (0u64, 0u64);
    for line in text.lines() {
        if left != (0, 0) {
            let counts = path.as_ref().and_then(|p| out.get_mut(p)?.as_mut());
            match line.as_bytes().first() {
                Some(b'+') => {
                    left.1 = left.1.saturating_sub(1);
                    if let Some((added, _)) = counts {
                        *added += 1;
                    }
                }
                Some(b'-') => {
                    left.0 = left.0.saturating_sub(1);
                    if let Some((_, removed)) = counts {
                        *removed += 1;
                    }
                }
                Some(b'\\') => {}
                _ => left = (left.0.saturating_sub(1), left.1.saturating_sub(1)),
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("diff --git a/") {
            // Both sides name one file but for a rename, which says so below.
            path = rest.split_once(" b/").map(|(_, after)| after.to_string());
            if let Some(path) = &path {
                out.insert(path.clone(), Some((0, 0)));
            }
        } else if let Some(to) = line.strip_prefix("rename to ") {
            if let Some(was) = path.replace(to.to_string()) {
                out.remove(&was);
            }
            out.insert(to.to_string(), Some((0, 0)));
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            if let Some(path) = &path {
                out.insert(path.clone(), None);
            }
        } else if let Some(hunk) = line.strip_prefix("@@ -") {
            left = hunk_lines(hunk).unwrap_or_default();
        }
    }
    if cut && let Some(last) = path {
        out.insert(last, None);
    }
    out
}

/// How many lines before and after a hunk headed `@@ -<hunk>` holds.
fn hunk_lines(hunk: &str) -> Option<(u64, u64)> {
    let mut sides = hunk.split(' ');
    let count = |side: &str| match side.split_once(',') {
        Some((_, n)) => n.parse().ok(),
        None => Some(1),
    };
    let before = count(sides.next()?)?;
    let after = count(sides.next()?.strip_prefix('+')?)?;
    Some((before, after))
}

/// Read `git diff --numstat -z`, by the path each file has after.
pub fn parse_numstat(out: &str) -> HashMap<String, Counts> {
    let mut fields = out.split('\0');
    let mut found = HashMap::new();
    while let Some(entry) = fields.next() {
        let mut parts = entry.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let path = match path {
            "" => {
                fields.next();
                fields.next().unwrap_or_default()
            }
            path => path,
        };
        let counts = added.parse().ok().zip(removed.parse().ok());
        found.insert(path.to_string(), counts);
    }
    found
}

/// What the worktree holding `dir` changed, its newest `commits` listed; none
/// outside version control.
pub async fn changed(dir: &Path, commits: usize) -> Result<Option<Changed>, Error> {
    match Backend::of(dir) {
        Some(Backend::Jj) => in_jj(dir, commits).await.map(Some),
        Some(Backend::Git) => in_git(dir, commits).await.map(Some),
        None => Ok(None),
    }
}

async fn in_jj(dir: &Path, commits: usize) -> Result<Changed, Error> {
    let root = run("jj", &["--no-pager", "root"], Some(dir)).await?;
    let root = Path::new(root.trim_end_matches('\n'));
    let from = "fork_point(trunk() | @)";
    let plain = ["--color", "never", "--no-pager"];
    let ask = |args: &[&'static str]| [&plain[..], args].concat();
    let base = run(
        "jj",
        &ask(&["log", "--no-graph", "-r", from, "-T", "commit_id"]),
        Some(root),
    )
    .await?;
    let files = run(
        "jj",
        &ask(&[
            "diff",
            "--from",
            from,
            "--to",
            "@",
            "-T",
            jj::FILES_TEMPLATE,
        ]),
        Some(root),
    )
    .await?;
    let diff = run_head(
        "jj",
        &ask(&["diff", "--from", from, "--to", "@", "--git"]),
        Some(root),
        COUNTED_LINES,
        COUNTED_BYTES,
    )
    .await?;
    let cut = reached(&diff, COUNTED_LINES, COUNTED_BYTES);
    let counts = counted(&diff, cut);
    Ok(Changed {
        backend: Backend::Jj,
        cut,
        base,
        commits: jj::history(root, commits).await?,
        files: jj::parse_files(&files)
            .into_iter()
            .map(|change| {
                let counted = counts.get(&change.path).copied().flatten();
                (change, counted)
            })
            .collect(),
    })
}

/// The commit the branch of the worktree at `root` left the branch it goes
/// back to at: the remote's default, else `main` or `master`, else its own.
async fn git_base(root: &Path) -> Result<String, Error> {
    for branch in ["origin/HEAD", "main", "master"] {
        let asked = ["merge-base", "--end-of-options", branch, "HEAD"];
        if let Ok(base) = run("git", &asked, Some(root)).await {
            return Ok(base.trim().to_string());
        }
    }
    let head = run("git", &["rev-parse", "HEAD"], Some(root)).await?;
    Ok(head.trim().to_string())
}

async fn in_git(dir: &Path, commits: usize) -> Result<Changed, Error> {
    let root = run("git", &["rev-parse", "--show-toplevel"], Some(dir)).await?;
    let root = Path::new(root.trim_end_matches('\n'));
    let base = git_base(root).await?;
    let diff = |how: &'static str| {
        let base = base.clone();
        async move {
            let args = ["diff", how, "-z", "--no-ext-diff", &base, "--"];
            run("git", &args, Some(root)).await
        }
    };
    let counts = parse_numstat(&diff("--numstat").await?);
    let mut files: Vec<(Change, Counts)> = git::parse_name_status(&diff("--name-status").await?)
        .into_iter()
        .map(|change| {
            let counted = counts.get(&change.path).copied().flatten();
            (change, counted)
        })
        .collect();
    for change in git::changes(root).await? {
        if change.status == "??" {
            let counted = added_lines(&root.join(&change.path));
            files.push((change, counted));
        }
    }
    Ok(Changed {
        backend: Backend::Git,
        cut: false,
        commits: git::history_since(root, &base, commits).await?,
        base,
        files,
    })
}

/// How many lines an untracked file adds; none for one that is no text or too
/// large to count.
fn added_lines(path: &Path) -> Counts {
    let meta = path.symlink_metadata().ok()?;
    if !meta.is_file() || meta.len() > DIFF_BYTES as u64 {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    (!text.contains('\0')).then(|| (text.lines().count() as u64, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_of_each_file_are_counted_and_none_that_look_like_headers() {
        let diff = "diff --git a/a.rs b/a.rs\nindex 1..2 100644\n--- a/a.rs\n+++ b/a.rs\n\
            @@ -1,2 +1,3 @@ fn x\n keep\n--- old\n+diff --git a/z b/z\n+more\n\
            diff --git a/new b/new\nnew file mode 100644\n--- /dev/null\n+++ b/new\n@@ -0,0 +1 @@\n+one\n\\ No newline at end of file\n\
            diff --git a/was b/now\nrename from was\nrename to now\n\
            diff --git a/i.png b/i.png\nBinary files a/i.png and b/i.png differ\n";
        let counts = parse_counts(diff);
        let mut got: Vec<_> = counts.iter().map(|(p, c)| (p.as_str(), *c)).collect();
        got.sort();
        assert_eq!(
            got,
            [
                ("a.rs", Some((2, 1))),
                ("i.png", None),
                ("new", Some((1, 0))),
                ("now", Some((0, 0))),
            ]
        );
    }

    #[test]
    fn a_file_whose_diff_was_cut_is_not_counted_and_those_before_it_are() {
        let diff = "diff --git a/a b/a\n@@ -0,0 +1 @@\n+x\n\
            diff --git a/b b/b\n@@ -0,0 +1,9 @@\n+x\n";
        let cut = counted(diff, true);
        assert_eq!((cut["a"], cut["b"]), (Some((1, 0)), None));
        let whole = counted(diff, false);
        assert_eq!(whole["b"], Some((1, 0)));
    }

    #[test]
    fn a_diff_read_up_to_a_limit_is_taken_as_cut() {
        assert!(reached("a\nb\n", 2, 100));
        assert!(reached("abcd", 9, 4));
        assert!(!reached("a\nb\n", 3, 100));
    }

    #[tokio::test]
    async fn the_commits_of_a_branch_merged_in_from_before_the_base_are_told() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let commit = |name: &str| git(root, &["commit", "-q", "--allow-empty", "-m", name]);
        git(root, &["init", "-q", "-b", "main"]);
        commit("root");
        git(root, &["checkout", "-q", "-b", "side"]);
        commit("side");
        git(root, &["checkout", "-q", "main"]);
        commit("base");
        git(root, &["checkout", "-q", "-b", "work"]);
        commit("work");
        git(root, &["merge", "-q", "--no-ff", "-m", "merge", "side"]);

        let got = changed(root, 10).await.unwrap().unwrap();

        let mut subjects: Vec<_> = got.commits.iter().map(|c| c.subject.as_str()).collect();
        subjects.sort();
        assert_eq!(subjects, ["merge", "side", "work"]);
    }

    #[test]
    fn numstat_is_read_with_renames_and_binary_files() {
        let counts = parse_numstat("3\t1\ta b.rs\0-\t-\ti.png\x000\t2\t\0was\0now\0");
        assert_eq!(counts["a b.rs"], Some((3, 1)));
        assert_eq!(counts["i.png"], None);
        assert_eq!(counts["now"], Some((0, 2)));
        assert_eq!(counts.len(), 3);
    }

    fn ran(program: &str, dir: &Path, args: &[&str]) -> bool {
        std::process::Command::new(program)
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    fn git(dir: &Path, args: &[&str]) {
        let all = [&["-c", "commit.gpgsign=false"], args].concat();
        assert!(ran("git", dir, &all), "git {args:?}");
    }

    fn told(changed: &Changed) -> Vec<(&str, &str, Counts)> {
        let mut files: Vec<_> = changed
            .files
            .iter()
            .map(|(c, n)| (c.status.trim(), c.path.as_str(), *n))
            .collect();
        files.sort();
        files
    }

    #[tokio::test]
    async fn a_git_worktree_tells_what_its_branch_and_its_files_changed_since_main() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-q", "-b", "main"]);
        std::fs::write(root.join("a"), "one\n").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "first"]);
        git(root, &["checkout", "-q", "-b", "work"]);
        std::fs::write(root.join("b"), "one\ntwo\n").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "second"]);
        std::fs::write(root.join("a"), "two\n").unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/new"), "x\n").unwrap();

        let got = changed(&root.join("sub"), 10).await.unwrap().unwrap();

        assert_eq!(got.backend, Backend::Git);
        assert_eq!(
            told(&got),
            [
                ("??", "sub/new", Some((1, 0))),
                ("A", "b", Some((2, 0))),
                ("M", "a", Some((1, 1))),
            ]
        );
        let subjects: Vec<_> = got.commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["second"]);
    }

    #[tokio::test]
    async fn a_jj_workspace_tells_what_its_changes_hold_over_trunk() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let jj = |args: &[&str]| {
            let who = ["--config", "user.name=a", "--config", "user.email=a@b"];
            ran("jj", root, &[&who, args].concat())
        };
        if !jj(&["git", "init", "."]) {
            eprintln!("jj is not installed; skipped");
            return;
        }
        std::fs::write(root.join("a"), "one\n").unwrap();
        assert!(jj(&["describe", "-m", "first"]));
        assert!(jj(&["new", "-m", "second"]));
        std::fs::write(root.join("a"), "one\ntwo\n").unwrap();

        let got = changed(root, 10).await.unwrap().unwrap();

        assert_eq!(got.backend, Backend::Jj);
        assert_eq!(told(&got), [("A", "a", Some((2, 0)))]);
        let subjects: Vec<_> = got.commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["second", "first"]);
    }

    #[tokio::test]
    async fn a_directory_outside_version_control_tells_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(changed(dir.path(), 10).await.unwrap(), None);
    }
}
