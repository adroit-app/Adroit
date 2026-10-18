//! What git says about a worktree: the files changed in it, its recent
//! commits and its diff.

use std::path::Path;

use adroit_model::Worktree;

use crate::{Error, run, run_head};

/// A file that differs from the last commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// git's two-letter status: index, then worktree (`M `, ` M`, `??`, …).
    pub status: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
    /// How long ago, as git words it.
    pub when: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    File,
    Hunk,
    Added,
    Removed,
    Context,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub text: String,
}

/// The most diff lines read; the rest is cut.
pub const DIFF_LINES: usize = 2000;

/// The most diff bytes read, for a diff of few but very long lines.
pub const DIFF_BYTES: usize = 1 << 20;

const FIELD: char = '\u{1f}';

/// Read `git status --porcelain -z`, where a rename or copy is followed by
/// the path it came from.
pub fn parse_status(out: &str) -> Vec<Change> {
    let mut fields = out.split('\0');
    let mut found = Vec::new();
    while let Some(entry) = fields.next() {
        let (Some(status), Some(path)) = (entry.get(..2), entry.get(3..)) else {
            continue;
        };
        let path = if status.contains(['R', 'C']) {
            match fields.next() {
                Some(from) => format!("{from} -> {path}"),
                None => path.to_string(),
            }
        } else {
            path.to_string()
        };
        found.push(Change {
            status: status.to_string(),
            path,
        });
    }
    found
}

/// Read `git log` printed with [`LOG_FORMAT`].
pub fn parse_log(out: &str) -> Vec<Commit> {
    out.lines()
        .filter_map(|l| {
            let mut f = l.split(FIELD);
            Some(Commit {
                hash: f.next()?.to_string(),
                subject: f.next()?.to_string(),
                when: f.next()?.to_string(),
            })
        })
        .collect()
}

const LOG_FORMAT: &str = "--format=%h%x1f%s%x1f%cr";

/// Read a unified diff, keeping at most `max` lines.
pub fn parse_diff(out: &str, max: usize) -> Vec<DiffLine> {
    // Between a file's `diff --git` line and its first hunk come header lines
    // (`index`, `---`, `+++`, modes, renames) that repeat what the file line
    // says; inside a hunk a line starting with `---` is a removed line.
    let mut in_header = false;
    let mut lines = Vec::new();
    for l in out.lines().take(max) {
        let kind = if l.starts_with("diff --git") {
            in_header = true;
            DiffKind::File
        } else if l.starts_with("@@") {
            in_header = false;
            DiffKind::Hunk
        } else if in_header {
            continue;
        } else if l.starts_with('+') {
            DiffKind::Added
        } else if l.starts_with('-') {
            DiffKind::Removed
        } else {
            DiffKind::Context
        };
        let text = match kind {
            DiffKind::File => l.rsplit_once(" b/").map_or(l, |(_, path)| path),
            _ => l,
        };
        lines.push(DiffLine {
            kind,
            text: text.to_string(),
        });
    }
    lines
}

/// Has git write a path as it is rather than octal escapes for anything
/// outside ASCII, so a Japanese name reads as itself.
const UNQUOTED: &str = "core.quotePath=false";

fn git_dir(path: &Path) -> Result<&str, Error> {
    path.to_str().ok_or_else(|| Error::Failed {
        program: "git".into(),
        stderr: format!("{} is not valid UTF-8", path.display()),
    })
}

/// The files changed in a worktree, staged or not, and the untracked ones.
pub async fn changes(path: &Path) -> Result<Vec<Change>, Error> {
    let dir = git_dir(path)?;
    let out = run("git", &["-C", dir, "status", "--porcelain", "-z"], None).await?;
    Ok(parse_status(&out))
}

/// The last `n` commits of a worktree's branch.
pub async fn history(path: &Path, n: usize) -> Result<Vec<Commit>, Error> {
    let dir = git_dir(path)?;
    let count = format!("-n{n}");
    let out = run("git", &["-C", dir, "log", &count, LOG_FORMAT], None).await?;
    Ok(parse_log(&out))
}

/// What the worktree changes against its last commit.
pub async fn diff(path: &Path) -> Result<Vec<DiffLine>, Error> {
    let dir = git_dir(path)?;
    let out = run_head(
        "git",
        &[
            "-C",
            dir,
            "-c",
            UNQUOTED,
            "diff",
            "HEAD",
            "--no-color",
            "--no-ext-diff",
        ],
        DIFF_LINES,
        DIFF_BYTES,
    )
    .await?;
    Ok(parse_diff(&out, DIFF_LINES))
}

/// Read `git worktree list --porcelain`: the first entry is the main checkout,
/// which every worktree belongs to. A bare repository and a worktree whose
/// directory is gone are left out.
pub fn parse_worktrees(out: &str) -> Vec<Worktree> {
    let mut repo = None::<std::path::PathBuf>;
    let mut found = Vec::new();
    for entry in out.split("\n\n") {
        let mut path = None;
        let mut branch = None;
        let mut skip = false;
        for line in entry.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                path = Some(std::path::PathBuf::from(p));
            } else if let Some(b) = line.strip_prefix("branch ") {
                branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
            } else if line == "bare" || line.starts_with("prunable") {
                skip = true;
            }
        }
        let Some(path) = path else { continue };
        let repo = repo.get_or_insert_with(|| path.clone()).clone();
        if !skip {
            found.push(Worktree { path, repo, branch });
        }
    }
    found
}

/// Every worktree of the repository whose main checkout is `repo`, each
/// belonging to `repo`. git names the main checkout by the repository's git
/// directory when that is not its `.git`, as for a submodule, so `repo` is
/// taken for it instead.
pub async fn worktrees(repo: &Path) -> Result<Vec<Worktree>, Error> {
    let out = run(
        "git",
        &["-C", git_dir(repo)?, "worktree", "list", "--porcelain"],
        None,
    )
    .await?;
    Ok(parse_worktrees(&out)
        .into_iter()
        .map(|w| Worktree {
            path: if w.is_main() {
                repo.to_path_buf()
            } else {
                w.path
            },
            repo: repo.to_path_buf(),
            branch: w.branch,
        })
        .collect())
}

/// The most files `files` lists.
pub const FILES: usize = 5000;

/// The files of a worktree git tracks or would track, at most [`FILES`].
pub async fn files(path: &Path) -> Result<Vec<String>, Error> {
    let dir = git_dir(path)?;
    let out = run(
        "git",
        &[
            "-C",
            dir,
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
        None,
    )
    .await?;
    Ok(out
        .split('\0')
        .filter(|f| !f.is_empty())
        .take(FILES)
        .map(str::to_string)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_worktree_belongs_to_the_main_checkout_listed_first() {
        let out = "worktree /src/api\nHEAD 1\nbranch refs/heads/main\n\n\
                   worktree /src/api/.claude/worktrees/x\nHEAD 2\nbranch refs/heads/feat/x\n\n\
                   worktree /src/api/.claude/worktrees/d\nHEAD 3\ndetached\n";
        let got = parse_worktrees(out);
        assert_eq!(got.len(), 3);
        assert!(got.iter().all(|w| w.repo == Path::new("/src/api")));
        assert_eq!(got[0].branch.as_deref(), Some("main"));
        assert_eq!(got[1].branch.as_deref(), Some("feat/x"));
        assert_eq!(got[2].branch, None);
    }

    #[test]
    fn a_bare_repository_and_a_vanished_worktree_are_left_out() {
        let out = "worktree /src/api.git\nbare\n\n\
                   worktree /src/w1\nHEAD 1\nbranch refs/heads/a\nprunable gitdir file points to non-existent location\n\n\
                   worktree /src/w2\nHEAD 2\nbranch refs/heads/b\n";
        let got = parse_worktrees(out);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, Path::new("/src/w2"));
        assert_eq!(got[0].repo, Path::new("/src/api.git"));
    }

    #[tokio::test]
    async fn a_file_named_in_japanese_is_listed_by_its_name() {
        let dir = std::env::temp_dir().join(format!("adroit-git-ja-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ok = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        // Quoted, as git does unless told otherwise.
        let ok = std::process::Command::new("git")
            .args(["config", "core.quotePath", "true"])
            .current_dir(&dir)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        std::fs::write(dir.join("日本語.md"), "x").unwrap();
        assert_eq!(files(&dir).await.unwrap(), ["日本語.md"]);
        assert_eq!(changes(&dir).await.unwrap()[0].path, "日本語.md");
    }

    #[tokio::test]
    async fn a_file_name_git_would_quote_is_listed_as_it_is() {
        let dir = std::env::temp_dir().join(format!("adroit-git-quote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ok = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        std::fs::write(dir.join("a\"b\\c.md"), "x").unwrap();
        assert_eq!(files(&dir).await.unwrap(), ["a\"b\\c.md"]);
        assert_eq!(changes(&dir).await.unwrap()[0].path, "a\"b\\c.md");
    }

    #[test]
    fn status_entries_split_into_code_and_path() {
        let got = parse_status(" M src/lib.rs\0?? notes.md\0R  new.rs\0old.rs\0");
        assert_eq!(
            got,
            vec![
                Change {
                    status: " M".into(),
                    path: "src/lib.rs".into()
                },
                Change {
                    status: "??".into(),
                    path: "notes.md".into()
                },
                Change {
                    status: "R ".into(),
                    path: "old.rs -> new.rs".into()
                },
            ]
        );
    }

    #[test]
    fn log_lines_carry_hash_subject_and_age() {
        let got = parse_log("abc1234\u{1f}fix: the callback\u{1f}2 hours ago\n");
        assert_eq!(
            got,
            vec![Commit {
                hash: "abc1234".into(),
                subject: "fix: the callback".into(),
                when: "2 hours ago".into(),
            }]
        );
    }

    #[test]
    fn a_diff_names_each_file_once_and_marks_what_changed() {
        let out = "diff --git a/src/a.rs b/src/a.rs\nindex 1..2 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,2 +1,2 @@\n keep\n-old\n+new\n";
        let kinds: Vec<_> = parse_diff(out, 100)
            .into_iter()
            .map(|d| (d.kind, d.text))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (DiffKind::File, "src/a.rs".into()),
                (DiffKind::Hunk, "@@ -1,2 +1,2 @@".into()),
                (DiffKind::Context, " keep".into()),
                (DiffKind::Removed, "-old".into()),
                (DiffKind::Added, "+new".into()),
            ]
        );
    }

    #[test]
    fn a_long_diff_is_cut() {
        let out = "+a\n".repeat(10);
        assert_eq!(parse_diff(&out, 3).len(), 3);
    }
    #[test]
    fn a_removed_line_that_looks_like_a_header_is_kept() {
        let out = "diff --git a/x.lua b/x.lua\n--- a/x.lua\n+++ b/x.lua\n@@ -1 +1 @@\n--- old note\n+++i;\n";
        let kinds: Vec<_> = parse_diff(out, 100).into_iter().map(|d| d.kind).collect();
        assert_eq!(
            kinds,
            [
                DiffKind::File,
                DiffKind::Hunk,
                DiffKind::Removed,
                DiffKind::Added
            ]
        );
    }
}
