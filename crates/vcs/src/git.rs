//! What git says about a worktree: the files changed in it, its recent
//! commits, what each changed, and a file's diff.

use std::path::Path;

use adroit_model::Worktree;

use crate::{Change, Commit, DIFF_BYTES, DiffKind, DiffLine, Error, Revision, run, run_head};

pub fn parse_revision(out: &str) -> Option<Revision> {
    let mut fields = out.splitn(3, '\0');
    Some(Revision {
        id: fields.next()?.to_string(),
        short: fields.next()?.to_string(),
        description: fields.next()?.trim_end_matches('\n').to_string(),
    })
}

pub async fn revision(path: &Path, rev: &str) -> Result<Revision, Error> {
    let out = run(
        "git",
        &[
            "show",
            "--no-patch",
            "--no-show-signature",
            "--format=%H%x00%h%x00%B",
            "--end-of-options",
            rev,
            "--",
        ],
        Some(path),
    )
    .await?;
    parse_revision(&out).ok_or_else(|| Error::Failed {
        program: "git".into(),
        stderr: "Cannot read the commit description".into(),
    })
}

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
        let from = if status.contains(['R', 'C']) {
            fields.next().map(str::to_string)
        } else {
            None
        };
        found.push(Change {
            status: status.to_string(),
            path: path.to_string(),
            from,
        });
    }
    found
}

/// Read `git show --name-status -z`: a status, then the path, or for a
/// rename or copy, scored as `R100`, the path it came from and the path.
pub fn parse_name_status(out: &str) -> Vec<Change> {
    let mut fields = out.split('\0').filter(|f| !f.is_empty());
    let mut found = Vec::new();
    while let Some(code) = fields.next() {
        let Some(letter) = code.get(..1) else {
            continue;
        };
        let Some(first) = fields.next() else { break };
        let (path, from) = if matches!(letter, "R" | "C") {
            match fields.next() {
                Some(to) => (to, Some(first.to_string())),
                None => (first, None),
            }
        } else {
            (first, None)
        };
        found.push(Change {
            status: letter.to_string(),
            path: path.to_string(),
            from,
        });
    }
    found
}

/// Read `git log` printed with [`LOG_FORMAT`].
pub fn parse_log(out: &str) -> Vec<Commit> {
    out.lines()
        .filter_map(|l| {
            let mut f = l.split(FIELD);
            let id = f.next()?.to_string();
            let parents = f.next()?.split_whitespace().map(str::to_string).collect();
            Some(Commit {
                node: id.clone(),
                id,
                parents,
                short: f.next()?.to_string(),
                subject: f.next()?.to_string(),
                when: f.next()?.to_string(),
                ..Default::default()
            })
        })
        .collect()
}

const LOG_FORMAT: &str = "--format=%H%x1f%P%x1f%h%x1f%s%x1f%cr";

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

/// A file git does not track yet, as the diff that would add it, at most
/// `max` lines.
pub fn added(path: &str, content: &[u8], max: usize) -> Vec<DiffLine> {
    let line = |kind, text: String| DiffLine { kind, text };
    let mut lines = vec![line(DiffKind::File, path.to_string())];
    if content.contains(&0) {
        lines.push(line(DiffKind::Context, "Binary file".into()));
        return lines;
    }
    let text = String::from_utf8_lossy(content);
    let count = text.lines().count();
    let hunk = match count {
        1 => "@@ -0,0 +1 @@".to_string(),
        n => format!("@@ -0,0 +1,{n} @@"),
    };
    lines.push(line(DiffKind::Hunk, hunk));
    lines.extend(text.lines().map(|l| line(DiffKind::Added, format!("+{l}"))));
    lines.truncate(max);
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

/// What git, asked `args` about the worktree at `path`, prints.
async fn git(path: &Path, args: &[&str]) -> Result<String, Error> {
    let mut all = vec!["-C", git_dir(path)?];
    all.extend(args);
    run("git", &all, None).await
}

/// The paths of a `-z` listing, at most [`FILES`].
fn nul_paths(out: &str) -> Vec<String> {
    out.split('\0')
        .filter(|f| !f.is_empty())
        .take(FILES)
        .map(str::to_string)
        .collect()
}

/// The files changed in a worktree, staged or not, and every untracked one,
/// each file of an untracked directory too.
pub async fn changes(path: &Path) -> Result<Vec<Change>, Error> {
    let out = git(
        path,
        &["status", "--porcelain", "-z", "--untracked-files=all"],
    )
    .await?;
    Ok(parse_status(&out))
}

/// The last `n` commits of a worktree's branch, each before its parents.
pub async fn history(path: &Path, n: usize) -> Result<Vec<Commit>, Error> {
    let count = format!("-n{n}");
    let out = git(path, &["log", "--topo-order", &count, LOG_FORMAT]).await?;
    Ok(parse_log(&out))
}

/// The files commit `rev` changed.
pub async fn commit_files(path: &Path, rev: &str) -> Result<Vec<Change>, Error> {
    let out = git(
        path,
        &[
            "-c",
            UNQUOTED,
            "show",
            "--name-status",
            "-z",
            "--format=",
            rev,
            "--",
        ],
    )
    .await?;
    Ok(parse_name_status(&out))
}

/// What `change` does to its file: in commit `rev`, or in the worktree
/// against its last commit when `rev` is `None`; up to `lines` lines.
pub async fn file_diff(
    path: &Path,
    rev: Option<&str>,
    change: &Change,
    lines: usize,
) -> Result<Vec<DiffLine>, Error> {
    let dir = git_dir(path)?;
    if rev.is_none() && change.status == "??" {
        let content =
            read_head(&path.join(&change.path), DIFF_BYTES).map_err(|e| Error::Failed {
                program: "git".into(),
                stderr: format!("cannot read {}: {e}", change.path),
            })?;
        return Ok(added(&change.path, &content, lines));
    }
    let mut args = vec!["-C", dir, "-c", UNQUOTED];
    match rev {
        Some(rev) => args.extend(["show", "--format=", rev]),
        None => args.extend(["diff", "HEAD"]),
    }
    args.extend(["--no-color", "--no-ext-diff", "--"]);
    args.extend(change.paths());
    let out = run_head("git", &args, None, lines, DIFF_BYTES).await?;
    Ok(parse_diff(&out, lines))
}

/// At most `max` bytes from the start of a file.
fn read_head(file: &Path, max: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut kept = Vec::new();
    std::fs::File::open(file)?
        .take(u64::try_from(max).unwrap_or(u64::MAX))
        .read_to_end(&mut kept)?;
    Ok(kept)
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
    let out = git(repo, &["worktree", "list", "--porcelain"]).await?;
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
    let out = git(
        path,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )
    .await?;
    Ok(nul_paths(&out))
}

/// The paths in a worktree git ignores, relative to it; a directory ignored
/// whole is named once, with a trailing `/`.
pub async fn ignored(path: &Path) -> Result<Vec<String>, Error> {
    let out = git(
        path,
        &[
            "-c",
            UNQUOTED,
            "ls-files",
            "-z",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
        ],
    )
    .await?;
    Ok(nul_paths(&out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DIFF_LINES;

    #[tokio::test]
    async fn ignored_paths_name_a_directory_ignored_whole_once() {
        let dir = std::env::temp_dir().join(format!("adroit-git-ignored-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("target/debug")).unwrap();
        git(&dir, &["init", "-q"]);
        std::fs::write(dir.join(".gitignore"), "target/\n*.log\n").unwrap();
        std::fs::write(dir.join("target/debug/a"), "x").unwrap();
        std::fs::write(dir.join("run.log"), "x").unwrap();
        std::fs::write(dir.join("kept.rs"), "x").unwrap();
        let mut got = ignored(&dir).await.unwrap();
        got.sort();
        assert_eq!(got, ["run.log", "target/"]);
    }

    #[tokio::test]
    async fn revision_details_keep_the_whole_message_and_read_the_selected_commit() {
        let dir = repo("revision-description");
        git(
            &dir,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "first subject",
                "-m",
                "Why this matters.\n\n日本語 and \"quotes\".",
            ],
        );
        let first = revision(&dir, "HEAD").await.unwrap();
        assert_eq!(
            first.description,
            "first subject\n\nWhy this matters.\n\n日本語 and \"quotes\"."
        );
        assert!(first.id.starts_with(&first.short));
        git(
            &dir,
            &["commit", "-q", "--allow-empty", "-m", "second subject"],
        );
        assert_eq!(
            revision(&dir, "HEAD").await.unwrap().description,
            "second subject"
        );
        assert_eq!(revision(&dir, &first.id).await.unwrap(), first);
    }

    #[test]
    fn revision_descriptions_keep_blank_lines_and_separator_characters() {
        let detail = parse_revision("full\0short\0subject\n\nbody\t日本語\u{1f}\u{1e}\n").unwrap();
        assert_eq!(detail.description, "subject\n\nbody\t日本語\u{1f}\u{1e}");
        assert_eq!(parse_revision("id\0id\0").unwrap().description, "");
        assert!(parse_revision("invalid").is_none());
    }

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

    fn change(status: &str, path: &str, from: Option<&str>) -> Change {
        Change {
            status: status.into(),
            path: path.into(),
            from: from.map(Into::into),
        }
    }

    #[test]
    fn a_nul_separated_listing_names_each_path_once_up_to_files() {
        assert_eq!(nul_paths("a\0b c\0\0dir/\0"), ["a", "b c", "dir/"]);
        let many = "f\0".repeat(FILES + 3);
        assert_eq!(nul_paths(&many).len(), FILES);
    }

    #[test]
    fn status_entries_split_into_code_and_path() {
        let got = parse_status(" M src/lib.rs\0?? notes.md\0R  new.rs\0old.rs\0");
        assert_eq!(
            got,
            vec![
                change(" M", "src/lib.rs", None),
                change("??", "notes.md", None),
                change("R ", "new.rs", Some("old.rs")),
            ]
        );
    }

    #[test]
    fn log_lines_carry_hash_parents_subject_and_age() {
        let got = parse_log(
            "abc1234def\u{1f}1111111aaa 2222222bbb\u{1f}abc1234\u{1f}merge: the callback\u{1f}2 hours ago\n\
             0000000aaa\u{1f}\u{1f}0000000\u{1f}\u{1f}3 days ago\n",
        );
        assert_eq!(
            got,
            vec![
                Commit {
                    id: "abc1234def".into(),
                    short: "abc1234".into(),
                    subject: "merge: the callback".into(),
                    when: "2 hours ago".into(),
                    node: "abc1234def".into(),
                    parents: vec!["1111111aaa".into(), "2222222bbb".into()],
                    ..Default::default()
                },
                Commit {
                    id: "0000000aaa".into(),
                    short: "0000000".into(),
                    subject: "".into(),
                    when: "3 days ago".into(),
                    node: "0000000aaa".into(),
                    ..Default::default()
                },
            ]
        );
    }

    #[test]
    fn what_a_commit_changed_is_read_with_a_rename_s_two_paths() {
        let got =
            parse_name_status("M\0src/a.rs\0R100\0old.rs\0new.rs\0A\0日本語.md\0D\0gone.txt\0");
        assert_eq!(
            got,
            vec![
                change("M", "src/a.rs", None),
                change("R", "new.rs", Some("old.rs")),
                change("A", "日本語.md", None),
                change("D", "gone.txt", None),
            ]
        );
    }

    #[test]
    fn a_new_file_reads_as_every_line_added() {
        let got: Vec<_> = added("notes.md", b"one\ntwo\n", 100)
            .into_iter()
            .map(|d| (d.kind, d.text))
            .collect();
        assert_eq!(
            got,
            vec![
                (DiffKind::File, "notes.md".into()),
                (DiffKind::Hunk, "@@ -0,0 +1,2 @@".into()),
                (DiffKind::Added, "+one".into()),
                (DiffKind::Added, "+two".into()),
            ]
        );
    }

    #[test]
    fn a_new_binary_file_is_not_spelled_out_and_a_long_one_is_cut() {
        let got = added("logo.png", b"\x89PNG\0\0", 100);
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].kind, DiffKind::Context);
        assert_eq!(added("long.txt", "x\n".repeat(50).as_bytes(), 10).len(), 10);
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(["-c", "user.name=a", "-c", "user.email=a@b"])
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    fn repo(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("adroit-git-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        dir
    }

    #[tokio::test]
    async fn a_file_s_diff_in_the_worktree_is_against_the_last_commit_or_whole_when_untracked() {
        let dir = repo("file-diff");
        std::fs::write(dir.join("a.txt"), "old\n").unwrap();
        git(&dir, &["add", "a.txt"]);
        git(&dir, &["commit", "-q", "-m", "first"]);
        std::fs::write(dir.join("a.txt"), "new\n").unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/b.txt"), "fresh\n").unwrap();

        let listed = changes(&dir).await.unwrap();
        assert_eq!(
            listed,
            vec![change(" M", "a.txt", None), change("??", "sub/b.txt", None)]
        );
        let texts = |d: Vec<DiffLine>| d.into_iter().map(|d| d.text).collect::<Vec<_>>();
        let a = texts(file_diff(&dir, None, &listed[0], DIFF_LINES).await.unwrap());
        assert_eq!(a, ["a.txt", "@@ -1 +1 @@", "-old", "+new"]);
        let b = texts(file_diff(&dir, None, &listed[1], DIFF_LINES).await.unwrap());
        assert_eq!(b, ["sub/b.txt", "@@ -0,0 +1 @@", "+fresh"]);
    }

    #[tokio::test]
    async fn a_commit_lists_what_it_changed_and_diffs_one_file_of_it() {
        let dir = repo("commit-files");
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        std::fs::write(dir.join("b.txt"), "two\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "first"]);

        let log = history(&dir, 10).await.unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].subject, "first");
        let files = commit_files(&dir, &log[0].id).await.unwrap();
        assert_eq!(
            files,
            vec![change("A", "a.txt", None), change("A", "b.txt", None)]
        );
        let diff = file_diff(&dir, Some(&log[0].id), &files[1], DIFF_LINES)
            .await
            .unwrap();
        let texts: Vec<_> = diff.into_iter().map(|d| d.text).collect();
        assert_eq!(texts, ["b.txt", "@@ -0,0 +1 @@", "+two"]);
    }

    #[tokio::test]
    async fn a_branch_s_log_comes_children_first_with_a_merge_s_parents_in_order() {
        let dir = repo("merge-log");
        git(&dir, &["commit", "-q", "--allow-empty", "-m", "base"]);
        git(&dir, &["branch", "side"]);
        git(&dir, &["commit", "-q", "--allow-empty", "-m", "main"]);
        git(&dir, &["switch", "-q", "side"]);
        git(&dir, &["commit", "-q", "--allow-empty", "-m", "side"]);
        git(&dir, &["switch", "-q", "-"]);
        git(&dir, &["merge", "-q", "--no-ff", "-m", "merge", "side"]);

        let log = history(&dir, 10).await.unwrap();
        let subjects: Vec<_> = log.iter().map(|c| c.subject.as_str()).collect();
        let at = |s: &str| subjects.iter().position(|x| *x == s).unwrap();
        let node = |s: &str| log[at(s)].node.clone();
        assert_eq!(log[at("merge")].parents, [node("main"), node("side")]);
        assert_eq!(at("merge"), 0);
        assert!(at("main") < at("base") && at("side") < at("base"));
        assert!(log[at("base")].parents.is_empty());
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
