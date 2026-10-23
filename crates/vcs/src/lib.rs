//! What adroit reads of and makes in repositories: through jj in a jj
//! repository, through git otherwise, and the repositories ghq keeps.

use std::path::{Path, PathBuf};

use tokio::process::Command;

mod discover;
pub mod ghq;
pub mod git;
pub mod jj;
mod view;
pub mod worktree;

pub use adroit_model::repository::*;
pub use discover::*;
pub use view::*;

/// The most diff lines read; the rest is cut.
pub const DIFF_LINES: usize = 2000;

/// The most diff bytes read, for a diff of few but very long lines.
pub const DIFF_BYTES: usize = 1 << 20;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot run {program}: {source}")]
    Spawn {
        program: String,
        source: std::io::Error,
    },
    #[error("{program} failed: {stderr}")]
    Failed { program: String, stderr: String },
}

/// The version control a directory belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Git,
    Jj,
}

impl Backend {
    /// The version control of the nearest directory above `dir` holding
    /// `.jj` or `.git`: jj when it holds `.jj`, as a colocated one also
    /// does; `None` outside both.
    pub fn of(dir: impl AsRef<Path>) -> Option<Backend> {
        let found = dir
            .as_ref()
            .ancestors()
            .find(|d| d.join(".jj").is_dir() || d.join(".git").exists())?;
        Some(if found.join(".jj").is_dir() {
            Backend::Jj
        } else {
            Backend::Git
        })
    }
}

/// A worktree, read through the version control it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repo {
    Git(PathBuf),
    Jj(PathBuf),
}

impl Repo {
    /// The worktree at `path`: a jj workspace when it belongs to jj, read
    /// through git otherwise, where reading fails outside git.
    pub fn open(path: impl Into<PathBuf>) -> Repo {
        let path = path.into();
        match Backend::of(&path) {
            Some(Backend::Jj) => Repo::Jj(path),
            Some(Backend::Git) | None => Repo::Git(path),
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Repo::Git(path) | Repo::Jj(path) => path,
        }
    }

    pub fn is_jj(&self) -> bool {
        matches!(self, Repo::Jj(_))
    }

    /// What `spot` shows, read through jj in a jj workspace, through git
    /// otherwise or when jj cannot be run.
    pub async fn load(&self, spot: &Spot, limits: Limits) -> View {
        view::load(self, spot, limits).await
    }

    /// What the version control says of each file of the worktree. git
    /// reads it in a jj workspace too, which jj colocates with git.
    pub async fn marks(&self) -> Marks {
        let root = self.path();
        let changes = git::changes(root).await.unwrap_or_default();
        let ignored = git::ignored(root).await.unwrap_or_default();
        Marks::new(root, &changes, &ignored, self.is_jj())
    }

    /// The files git tracks or would track, at most [`git::FILES`].
    pub async fn files(&self) -> Result<Vec<String>, Error> {
        git::files(self.path()).await
    }
}

/// What `program` prints, run with `args` in `cwd`, or why it failed.
async fn run(program: &str, args: &[&str], cwd: Option<&Path>) -> Result<String, Error> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let out = cmd.output().await.map_err(|source| Error::Spawn {
        program: program.into(),
        source,
    })?;
    if !out.status.success() {
        return Err(Error::Failed {
            program: program.into(),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What [`run`] prints, up to `max_lines` lines and `max_bytes` bytes: the
/// program is killed once either is read, so output past them is never read
/// nor waited for. Whether it would have failed is then unknown, and not told.
async fn run_head(
    program: &str,
    args: &[&str],
    cwd: Option<&Path>,
    max_lines: usize,
    max_bytes: usize,
) -> Result<String, Error> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
    let spawn_error = |source| Error::Spawn {
        program: program.into(),
        source,
    };
    let mut cmd = Command::new(program);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let mut child = cmd
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(spawn_error)?;
    // Read beside the output: a child blocked writing errors no one reads
    // would never finish the output either.
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let errors = tokio::spawn(async move {
        let mut kept = Vec::new();
        let _ = (&mut stderr).take(ERRORS_KEPT).read_to_end(&mut kept).await;
        let _ = tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await;
        kept
    });
    let stdout = child.stdout.take().expect("stdout is piped");
    let limit = u64::try_from(max_bytes).unwrap_or(u64::MAX);
    let mut reader = BufReader::new(stdout.take(limit));
    let mut read = Vec::new();
    let mut lines = 0;
    while lines < max_lines {
        if reader
            .read_until(b'\n', &mut read)
            .await
            .map_err(spawn_error)?
            == 0
        {
            break;
        }
        lines += 1;
    }
    let text = String::from_utf8_lossy(&read).into_owned();
    if lines == max_lines || reader.get_ref().limit() == 0 {
        let _ = child.start_kill();
        let _ = child.wait().await;
        return Ok(text);
    }
    let status = child.wait().await.map_err(spawn_error)?;
    if !status.success() {
        let errors = errors.await.unwrap_or_default();
        return Err(Error::Failed {
            program: program.into(),
            stderr: String::from_utf8_lossy(&errors).trim().to_string(),
        });
    }
    Ok(text)
}

/// The most of a capped command's errors kept to report.
const ERRORS_KEPT: u64 = 64 * 1024;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("adroit-vcs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_directory_is_jj_s_when_the_nearest_repository_above_it_is() {
        let dir = scratch("detect");
        for d in [
            "jj/.jj",
            "jj/src",
            "colo/.jj",
            "colo/.git",
            "jj/wt/.git",
            "plain/.git",
        ] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        assert_eq!(Backend::of(dir.join("jj/src")), Some(Backend::Jj));
        assert_eq!(Backend::of(dir.join("colo")), Some(Backend::Jj));
        // A git worktree inside a jj repository is git's.
        assert_eq!(Backend::of(dir.join("jj/wt")), Some(Backend::Git));
        assert_eq!(Backend::of(dir.join("plain")), Some(Backend::Git));
        assert!(Repo::open(dir.join("jj/src")).is_jj());
        assert!(!Repo::open(dir.join("plain")).is_jj());
    }

    #[tokio::test]
    async fn output_past_its_line_cap_is_not_waited_for() {
        let got = tokio::time::timeout(
            Duration::from_secs(5),
            run_head("yes", &["+a"], None, 3, usize::MAX),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(got, "+a\n+a\n+a\n");
    }

    #[tokio::test]
    async fn more_errors_than_a_pipe_holds_do_not_stall_the_output() {
        let got = tokio::time::timeout(
            Duration::from_secs(5),
            run_head(
                "sh",
                &["-c", "head -c 200000 /dev/zero >&2; echo done"],
                None,
                10,
                usize::MAX,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(got, "done\n");
    }

    #[tokio::test]
    async fn output_past_its_byte_cap_is_not_waited_for() {
        let got = tokio::time::timeout(
            Duration::from_secs(5),
            run_head("yes", &["a long line"], None, usize::MAX, 8),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(got, "a long l");
    }

    #[tokio::test]
    async fn output_within_its_caps_comes_whole() {
        assert_eq!(
            run_head("printf", &["a\\nb"], None, 3, 100).await.unwrap(),
            "a\nb"
        );
    }

    #[tokio::test]
    async fn a_failure_within_the_caps_is_reported() {
        assert!(matches!(
            run_head("false", &[], None, 3, 100).await,
            Err(Error::Failed { .. })
        ));
    }
}
