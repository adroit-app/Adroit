//! What jj says about a workspace: the changes of its stack, what each
//! changed, and a file's diff in one of them.

use std::path::Path;

use crate::git::{parse_diff, parse_revision};
use crate::{Change, Commit, DIFF_BYTES, DiffLine, Error, Revision, run, run_head};

const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';

/// A change of the stack as [`parse_log`] reads it: the separators are
/// control characters no description or bookmark name holds.
pub const LOG_TEMPLATE: &str = r#"change_id ++ "\x1f" ++ change_id.shortest(8) ++ "\x1f" ++ description.first_line() ++ "\x1f" ++ committer.timestamp().ago() ++ "\x1f" ++ if(current_working_copy, "@") ++ "\x1f" ++ if(empty, "empty") ++ "\x1f" ++ if(conflict, "conflict") ++ "\x1f" ++ local_bookmarks.map(|b| b.name()).join(" ") ++ "\x1f" ++ commit_id ++ "\x1f" ++ parents.map(|c| c.commit_id()).join(" ") ++ "\x1e""#;

/// A file a change touches as [`parse_files`] reads it.
pub const FILES_TEMPLATE: &str =
    r#"status ++ "\x1f" ++ source.path() ++ "\x1f" ++ target.path() ++ "\x1e""#;

/// The changes the log shows: those on top of trunk, and the working copy
/// even when it is in trunk.
const STACK: &str = "trunk()..@ | @";

fn records(out: &str) -> impl Iterator<Item = std::str::Split<'_, char>> {
    out.split(RECORD)
        .map(|r| r.trim_start_matches('\n'))
        .filter(|r| !r.is_empty())
        .map(|r| r.split(FIELD))
}

/// Read `jj log` printed with [`LOG_TEMPLATE`].
pub fn parse_log(out: &str) -> Vec<Commit> {
    records(out)
        .filter_map(|mut f| {
            Some(Commit {
                id: f.next()?.to_string(),
                short: f.next()?.to_string(),
                subject: f.next()?.to_string(),
                when: f.next()?.to_string(),
                current: f.next()? == "@",
                empty: f.next()? == "empty",
                conflict: f.next()? == "conflict",
                bookmarks: f.next()?.split_whitespace().map(str::to_string).collect(),
                node: f.next()?.to_string(),
                parents: f.next()?.split_whitespace().map(str::to_string).collect(),
            })
        })
        .collect()
}

/// Read `jj diff` printed with [`FILES_TEMPLATE`], its statuses as git's
/// letters.
pub fn parse_files(out: &str) -> Vec<Change> {
    records(out)
        .filter_map(|mut f| {
            let (status, source, target) = (f.next()?, f.next()?, f.next()?);
            let letter = match status {
                "modified" => "M",
                "added" => "A",
                "removed" => "D",
                "renamed" => "R",
                "copied" => "C",
                _ => "?",
            };
            Some(Change {
                status: letter.to_string(),
                path: target.to_string(),
                from: matches!(letter, "R" | "C").then(|| source.to_string()),
            })
        })
        .collect()
}

/// The fileset naming exactly `paths`, from the workspace's root.
pub fn fileset(paths: &[&str]) -> String {
    paths
        .iter()
        .map(|p| {
            let quoted = p.replace('\\', r"\\").replace('"', "\\\"");
            format!(r#"root-file:"{quoted}""#)
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Every jj command reads plainly: no colour and no pager, whatever the
/// user's configuration asks.
const PLAIN: [&str; 3] = ["--color", "never", "--no-pager"];

/// `args` after [`PLAIN`].
fn plain<'a>(args: &[&'a str]) -> Vec<&'a str> {
    PLAIN.iter().chain(args).copied().collect()
}

/// What jj, asked `args` plainly in the workspace at `path`, prints.
async fn jj(path: &Path, args: &[&str]) -> Result<String, Error> {
    run("jj", &plain(args), Some(path)).await
}

/// The changes on top of trunk, the working copy first and each before its
/// parents, at most `n`.
pub async fn history(path: &Path, n: usize) -> Result<Vec<Commit>, Error> {
    let count = n.to_string();
    let out = jj(
        path,
        &[
            "log",
            "--no-graph",
            "-r",
            STACK,
            "-n",
            &count,
            "-T",
            LOG_TEMPLATE,
        ],
    )
    .await?;
    Ok(parse_log(&out))
}

pub async fn revision(path: &Path, rev: &str) -> Result<Revision, Error> {
    let out = jj(
        path,
        &[
            "log",
            "--no-graph",
            "-r",
            rev,
            "-T",
            r#"change_id ++ "\0" ++ change_id.shortest(8) ++ "\0" ++ description"#,
        ],
    )
    .await?;
    parse_revision(&out).ok_or_else(|| Error::Failed {
        program: "jj".into(),
        stderr: "Cannot read the change description".into(),
    })
}

/// The files change `rev` touches.
pub async fn files(path: &Path, rev: &str) -> Result<Vec<Change>, Error> {
    let out = jj(path, &["diff", "-r", rev, "-T", FILES_TEMPLATE]).await?;
    Ok(parse_files(&out))
}

/// What `change` does to its file in change `rev`, up to `lines` lines.
pub async fn file_diff(
    path: &Path,
    rev: &str,
    change: &Change,
    lines: usize,
) -> Result<Vec<DiffLine>, Error> {
    let files = fileset(&change.paths());
    let args = plain(&["diff", "-r", rev, "--git", "--", &files]);
    let out = run_head("jj", &args, Some(path), lines, DIFF_BYTES).await?;
    Ok(parse_diff(&out, lines))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DIFF_LINES, DiffKind};

    fn change(status: &str, path: &str, from: Option<&str>) -> Change {
        Change {
            status: status.into(),
            path: path.into(),
            from: from.map(Into::into),
        }
    }

    #[tokio::test]
    async fn revision_details_read_full_descriptions_for_the_working_copy_and_an_older_change() {
        let dir = scratch("revision-description");
        if !jj(&dir, &["git", "init", "."]) {
            return;
        }
        assert!(jj(
            &dir,
            &[
                "describe",
                "-m",
                "first subject",
                "-m",
                "Why this matters.\n\n日本語 and \"quotes\"."
            ]
        ));
        let first = revision(&dir, "@").await.unwrap();
        assert_eq!(
            first.description,
            "first subject\n\nWhy this matters.\n\n日本語 and \"quotes\"."
        );
        assert!(jj(&dir, &["new", "-m", "second subject"]));
        assert_eq!(
            revision(&dir, "@").await.unwrap().description,
            "second subject"
        );
        assert_eq!(revision(&dir, &first.id).await.unwrap(), first);
        assert!(jj(&dir, &["describe", "-m", ""]));
        assert_eq!(revision(&dir, "@").await.unwrap().description, "");
    }

    #[test]
    fn each_change_of_the_stack_carries_its_ids_description_age_marks_and_parents() {
        // Recorded from `jj log --no-graph -T LOG_TEMPLATE`.
        let out = "splomqtnxsqppnkotsuyxozn\u{1f}splomqtn\u{1f}adroit: drill into changes\u{1f}2 minutes ago\u{1f}@\u{1f}empty\u{1f}\u{1f}\u{1f}c1\u{1f}c2\u{1e}\
                   vrssmupvuyssmvmxvsqmkqwn\u{1f}vrssmupv\u{1f}\u{1f}8 minutes ago\u{1f}\u{1f}empty\u{1f}\u{1f}\u{1f}c2\u{1f}c3\u{1e}\
                   wmmoymmlprsuyzmqzzywxvxq\u{1f}wmmoymml\u{1f}nvim: keep the reader\u{1f}11 minutes ago\u{1f}\u{1f}\u{1f}conflict\u{1f}main reader-fix\u{1f}c3\u{1f}c4 c5\u{1e}";
        assert_eq!(
            parse_log(out),
            vec![
                Commit {
                    id: "splomqtnxsqppnkotsuyxozn".into(),
                    short: "splomqtn".into(),
                    subject: "adroit: drill into changes".into(),
                    when: "2 minutes ago".into(),
                    node: "c1".into(),
                    parents: vec!["c2".into()],
                    current: true,
                    empty: true,
                    ..Default::default()
                },
                Commit {
                    id: "vrssmupvuyssmvmxvsqmkqwn".into(),
                    short: "vrssmupv".into(),
                    when: "8 minutes ago".into(),
                    node: "c2".into(),
                    parents: vec!["c3".into()],
                    empty: true,
                    ..Default::default()
                },
                Commit {
                    id: "wmmoymmlprsuyzmqzzywxvxq".into(),
                    short: "wmmoymml".into(),
                    subject: "nvim: keep the reader".into(),
                    when: "11 minutes ago".into(),
                    node: "c3".into(),
                    parents: vec!["c4".into(), "c5".into()],
                    conflict: true,
                    bookmarks: vec!["main".into(), "reader-fix".into()],
                    ..Default::default()
                },
            ]
        );
    }

    #[test]
    fn a_description_may_hold_any_character_but_the_separators() {
        let out = "a\u{1f}a\u{1f}fix: \"quotes\", tabs\tand 日本語\u{1f}now\u{1f}\u{1f}\u{1f}\u{1f}\u{1f}c\u{1f}\u{1e}\n";
        assert_eq!(
            parse_log(out)[0].subject,
            "fix: \"quotes\", tabs\tand 日本語"
        );
        assert!(
            parse_log(out)[0].parents.is_empty(),
            "a root has no parents"
        );
    }

    #[test]
    fn the_files_a_change_touches_read_as_git_s_letters() {
        // Recorded from `jj diff -T FILES_TEMPLATE` over a removal, an edit,
        // a rename and an addition.
        let out = "removed\u{1f}gone.txt\u{1f}gone.txt\u{1e}\
                   modified\u{1f}keep.txt\u{1f}keep.txt\u{1e}\
                   renamed\u{1f}old.txt\u{1f}new.txt\u{1e}\
                   added\u{1f}日本 語.md\u{1f}日本 語.md\u{1e}\
                   copied\u{1f}a.txt\u{1f}b.txt\u{1e}";
        assert_eq!(
            parse_files(out),
            vec![
                change("D", "gone.txt", None),
                change("M", "keep.txt", None),
                change("R", "new.txt", Some("old.txt")),
                change("A", "日本 語.md", None),
                change("C", "b.txt", Some("a.txt")),
            ]
        );
    }

    #[test]
    fn a_fileset_names_each_path_literally_from_the_root() {
        assert_eq!(fileset(&["src/a.rs"]), r#"root-file:"src/a.rs""#);
        assert_eq!(
            fileset(&["old *.rs", r#"a"b\c"#]),
            r#"root-file:"old *.rs" | root-file:"a\"b\\c""#
        );
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("adroit-jj-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn jj(dir: &Path, args: &[&str]) -> bool {
        std::process::Command::new("jj")
            .args(["--config", "user.name=a", "--config", "user.email=a@b"])
            .args(args)
            .current_dir(dir)
            .output()
            .is_ok_and(|o| o.status.success())
    }

    #[tokio::test]
    async fn a_jj_stack_drills_into_a_change_s_files_and_one_file_s_diff() {
        let dir = scratch("stack");
        if !jj(&dir, &["git", "init", "."]) {
            eprintln!("jj is not installed; skipped");
            return;
        }
        std::fs::write(dir.join("a.txt"), "old\n").unwrap();
        assert!(jj(&dir, &["describe", "-m", "first: add a"]));
        assert!(jj(&dir, &["new"]));
        std::fs::write(dir.join("a.txt"), "new\n").unwrap();

        let stack = history(&dir, 10).await.unwrap();
        assert_eq!(stack.len(), 2);
        assert!(stack[0].current);
        assert_eq!(stack[0].subject, "");
        assert_eq!(stack[1].subject, "first: add a");
        assert_eq!(stack[0].parents, [stack[1].node.clone()]);
        let changed = files(&dir, "@").await.unwrap();
        assert_eq!(changed, vec![change("M", "a.txt", None)]);
        let diff = file_diff(&dir, "@", &changed[0], DIFF_LINES).await.unwrap();
        let got: Vec<_> = diff.into_iter().map(|d| (d.kind, d.text)).collect();
        assert_eq!(
            got,
            vec![
                (DiffKind::File, "a.txt".into()),
                (DiffKind::Hunk, "@@ -1,1 +1,1 @@".into()),
                (DiffKind::Removed, "-old".into()),
                (DiffKind::Added, "+new".into()),
            ]
        );
        let first = files(&dir, &stack[1].id).await.unwrap();
        assert_eq!(first, vec![change("A", "a.txt", None)]);
    }

    #[tokio::test]
    async fn a_jj_stack_comes_children_first_with_a_merge_s_parents_in_order() {
        let dir = scratch("merge");
        if !jj(&dir, &["git", "init", "."]) {
            eprintln!("jj is not installed; skipped");
            return;
        }
        assert!(jj(&dir, &["describe", "-m", "base"]));
        assert!(jj(&dir, &["new", "-m", "left"]));
        assert!(jj(
            &dir,
            &["new", "-m", "right", "description(glob:\"base*\")"]
        ));
        assert!(jj(
            &dir,
            &[
                "new",
                "-m",
                "merge",
                "description(glob:\"left*\")",
                "description(glob:\"right*\")"
            ]
        ));

        let stack = history(&dir, 10).await.unwrap();
        let subjects: Vec<_> = stack.iter().map(|c| c.subject.as_str()).collect();
        let at = |s: &str| subjects.iter().position(|x| *x == s).unwrap();
        let node = |s: &str| stack[at(s)].node.clone();
        assert_eq!(stack[at("merge")].parents, [node("left"), node("right")]);
        assert!(at("merge") < at("left") && at("merge") < at("right"));
        assert!(at("left") < at("base") && at("right") < at("base"));
    }
}
