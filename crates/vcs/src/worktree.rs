//! New worktrees for started sessions: a jj workspace where the directory
//! belongs to jj, a Git worktree where it belongs to Git alone.
use std::{
    ffi::OsStr,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use crate::Backend;

type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    /// The full commit the worktree starts from.
    pub base: String,
}

/// Makes a worktree named `name` of the repository holding `source`, starting
/// from `base`, or from `source`'s current change or commit when none is given.
pub async fn create(
    source: &Path,
    name: &str,
    base: Option<&str>,
    description: &str,
) -> Result<Worktree> {
    let source = canonical(source)?;
    let (backend, root, repository) = identify(&source).await?;
    let slug = slug(name);
    let parent = destination(&base_directory(&root, &repository, backend).await?)?;
    let path = (1..)
        .map(|n| match n {
            1 => parent.join(&slug),
            n => parent.join(format!("{slug}-{n}")),
        })
        .map(|path| exists(&path).map(|taken| (!taken).then_some(path)))
        .find_map(Result::transpose)
        .expect("an unbounded search finds a free name")?;
    let native = path
        .file_name()
        .expect("a joined name")
        .to_string_lossy()
        .into_owned();
    let default = match backend {
        Backend::Jj => "@",
        Backend::Git => "HEAD",
    };
    let base = revision(&source, backend, base.unwrap_or(default), base.is_none()).await?;
    std::fs::create_dir_all(&parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    match backend {
        Backend::Jj => {
            command(
                &root,
                "jj",
                &[
                    "--no-pager".as_ref(),
                    "workspace".as_ref(),
                    "add".as_ref(),
                    "--name".as_ref(),
                    native.as_ref(),
                    "--revision".as_ref(),
                    base.as_ref(),
                    "--message".as_ref(),
                    description.as_ref(),
                    path.as_os_str(),
                ],
            )
            .await?;
        }
        Backend::Git => {
            command(
                &root,
                "git",
                &[
                    "worktree".as_ref(),
                    "add".as_ref(),
                    "-b".as_ref(),
                    native.as_ref(),
                    path.as_os_str(),
                    base.as_ref(),
                ],
            )
            .await?;
        }
    }
    Ok(Worktree { path, base })
}

/// Lower-case words joined by dashes, so a session's name can name a directory.
fn slug(name: &str) -> String {
    let words: Vec<String> = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    if words.is_empty() {
        "session".into()
    } else {
        words.join("-")
    }
}

async fn identify(source: &Path) -> Result<(Backend, PathBuf, PathBuf)> {
    match Backend::of(source) {
        Some(Backend::Jj) => {
            let reported = jj(source, &["root".as_ref()]).await?;
            let root = canonical(Path::new(reported.trim()))?;
            let metadata = root.join(".jj/repo");
            let repository = if metadata.is_dir() {
                metadata
            } else {
                let pointer = std::fs::read_to_string(&metadata)
                    .map_err(|error| format!("cannot read the jj repository identity: {error}"))?;
                root.join(".jj").join(pointer.trim())
            };
            Ok((Backend::Jj, root, canonical(&repository)?))
        }
        Some(Backend::Git) => {
            let reported = command(
                source,
                "git",
                &[
                    "rev-parse".as_ref(),
                    "--path-format=absolute".as_ref(),
                    "--show-toplevel".as_ref(),
                    "--git-common-dir".as_ref(),
                ],
            )
            .await?;
            let lines: Vec<_> = reported.lines().collect();
            let [top, common] = lines.as_slice() else {
                return Err("Git did not report its worktree and repository".into());
            };
            Ok((
                Backend::Git,
                canonical(Path::new(top))?,
                canonical(Path::new(common))?,
            ))
        }
        None => Err(format!(
            "{} is not under version control, so no worktree can be made of it",
            source.display()
        )),
    }
}

async fn output(dir: &Path, program: &str, args: &[&OsStr]) -> Result<std::process::Output> {
    let mut process = tokio::process::Command::new(program);
    process
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
    ] {
        process.env_remove(key);
    }
    tokio::time::timeout(Duration::from_secs(30), process.output())
        .await
        .map_err(|_| format!("{program} timed out"))?
        .map_err(|error| format!("cannot start {program}: {error}"))
}

async fn command(dir: &Path, program: &str, args: &[&OsStr]) -> Result<String> {
    let output = output(dir, program, args).await?;
    if !output.status.success() {
        return Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| format!("{program} returned invalid UTF-8"))
}

async fn jj(dir: &Path, args: &[&OsStr]) -> Result<String> {
    let mut full = vec![
        OsStr::new("--no-pager"),
        OsStr::new("--color=never"),
        OsStr::new("--at-operation=@"),
        OsStr::new("--ignore-working-copy"),
    ];
    full.extend_from_slice(args);
    command(dir, "jj", &full).await
}

/// Where new worktrees go: Git's `wt.basedir` when set, otherwise a sibling
/// of the repository named after it.
async fn base_directory(root: &Path, repository: &Path, backend: Backend) -> Result<PathBuf> {
    let anchor = match backend {
        Backend::Jj => repository
            .parent()
            .filter(|parent| parent.file_name().is_some_and(|name| name == ".jj"))
            .and_then(Path::parent),
        Backend::Git => (repository.file_name().is_some_and(|name| name == ".git"))
            .then(|| repository.parent())
            .flatten(),
    }
    .unwrap_or(root);
    if exists(&root.join(".git"))? {
        let configured = output(
            root,
            "git",
            &[
                "config".as_ref(),
                "--path".as_ref(),
                "--get".as_ref(),
                "wt.basedir".as_ref(),
            ],
        )
        .await?;
        if configured.status.success() {
            let configured = String::from_utf8(configured.stdout)
                .map_err(|_| "the configured worktree base is not UTF-8")?;
            let configured = configured.trim_end_matches('\n');
            if configured.is_empty() || configured.lines().count() != 1 {
                return Err("the configured worktree base must be one nonempty path".into());
            }
            let configured = Path::new(configured);
            return Ok(if configured.is_absolute() {
                configured.into()
            } else {
                anchor.join(configured)
            });
        }
        if configured.status.code() != Some(1) {
            return Err("cannot read the configured worktree base".into());
        }
    }
    Ok(anchor
        .parent()
        .ok_or("the repository has no parent directory")?
        .join(format!(
            "{}-wt",
            anchor.file_name().unwrap_or_default().to_string_lossy()
        )))
}

async fn revision(dir: &Path, backend: Backend, requested: &str, snapshot: bool) -> Result<String> {
    let output = match backend {
        Backend::Jj => {
            let args: &[&OsStr] = &[
                "--no-pager".as_ref(),
                "--color=never".as_ref(),
                "log".as_ref(),
                "-r".as_ref(),
                requested.as_ref(),
                "--no-graph".as_ref(),
                "-T".as_ref(),
                "(commit_id ++ \"\\n\")".as_ref(),
            ];
            if snapshot {
                command(dir, "jj", args).await?
            } else {
                jj(dir, args).await?
            }
        }
        Backend::Git => {
            command(
                dir,
                "git",
                &[
                    "rev-parse".as_ref(),
                    "--verify".as_ref(),
                    format!("{requested}^{{commit}}").as_ref(),
                ],
            )
            .await?
        }
    };
    let values: Vec<_> = output.lines().collect();
    match values.as_slice() {
        [value]
            if matches!(value.len(), 40 | 64)
                && value.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
        {
            Ok((*value).into())
        }
        _ => Err(format!("{requested} does not name exactly one revision")),
    }
}

fn exists(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("cannot inspect {}: {error}", path.display())),
    }
}

fn canonical(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .map_err(|error| format!("cannot resolve {}: {error}", path.display()))
}

/// `path` with its existing part resolved, refused inside VCS metadata.
fn destination(path: &Path) -> Result<PathBuf> {
    let inside_metadata = |path: &Path| {
        path.components()
            .any(|part| matches!(part, Component::Normal(name) if name == ".git" || name == ".jj"))
    };
    if !path.is_absolute() || inside_metadata(path) {
        return Err(format!("{} cannot hold worktrees", path.display()));
    }
    let mut parent = path;
    let mut suffix = Vec::new();
    while !exists(parent)? {
        suffix.push(
            parent
                .file_name()
                .ok_or("the destination has no name")?
                .to_owned(),
        );
        parent = parent.parent().ok_or("the destination has no parent")?;
    }
    let mut resolved = canonical(parent)?;
    for component in suffix.into_iter().rev() {
        resolved.push(component);
    }
    if inside_metadata(&resolved) {
        return Err(format!("{} cannot hold worktrees", resolved.display()));
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn run(dir: &Path, program: &str, args: &[&str]) -> String {
        let output = Command::new(program)
            .args(args)
            .current_dir(dir)
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

    fn git_repository(dir: &Path) -> PathBuf {
        let source = dir.join("api");
        std::fs::create_dir(&source).unwrap();
        run(&source, "git", &["init", "-q", "-b", "main"]);
        std::fs::write(source.join("a.txt"), "a").unwrap();
        run(&source, "git", &["add", "a.txt"]);
        run(&source, "git", &["commit", "-q", "-m", "first"]);
        source
    }

    #[tokio::test]
    async fn a_git_repository_gets_a_worktree_where_git_keeps_them_named_after_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        run(&source, "git", &["config", "wt.basedir", "../trees"]);
        let head = run(&source, "git", &["rev-parse", "HEAD"]);

        let made = create(&source, "Fix the flaky login test", None, "fix")
            .await
            .unwrap();

        let trees = dir.path().canonicalize().unwrap().join("trees");
        assert_eq!(made.path, trees.join("fix-the-flaky-login-test"));
        assert_eq!(made.base, head.trim());
        assert!(made.path.join("a.txt").is_file());
    }

    #[tokio::test]
    async fn a_second_worktree_of_the_same_name_gets_a_number() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());

        let first = create(&source, "review", None, "review").await.unwrap();
        let second = create(&source, "review", None, "review").await.unwrap();

        assert_ne!(first.path, second.path);
        assert!(second.path.ends_with("review-2"));
    }

    #[tokio::test]
    async fn a_worktree_starts_from_the_revision_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        let first = run(&source, "git", &["rev-parse", "HEAD"]);
        std::fs::write(source.join("b.txt"), "b").unwrap();
        run(&source, "git", &["add", "b.txt"]);
        run(&source, "git", &["commit", "-q", "-m", "second"]);

        let made = create(&source, "continue", Some(first.trim()), "continue")
            .await
            .unwrap();

        assert_eq!(made.base, first.trim());
        assert!(!made.path.join("b.txt").exists());
    }

    #[tokio::test]
    async fn a_jj_repository_gets_a_workspace_holding_the_current_change() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("api");
        std::fs::create_dir(&source).unwrap();
        run(&source, "jj", &["git", "init", "--colocate"]);
        std::fs::write(source.join("draft.txt"), "unfinished").unwrap();

        let made = create(&source, "take over", None, "take over")
            .await
            .unwrap();

        assert!(made.path.join("draft.txt").is_file());
        let workspaces = run(&source, "jj", &["workspace", "list"]);
        assert!(workspaces.contains("take-over"), "{workspaces}");
    }

    #[tokio::test]
    async fn a_directory_outside_version_control_gets_no_worktree() {
        let dir = tempfile::tempdir().unwrap();

        let refused = create(dir.path(), "anything", None, "anything")
            .await
            .unwrap_err();

        assert!(refused.contains("not under version control"), "{refused}");
    }

    #[test]
    fn a_name_in_words_becomes_a_directory_name() {
        assert_eq!(slug("Review the API!"), "review-the-api");
        assert_eq!(slug("ログイン修正"), "session");
    }
}
