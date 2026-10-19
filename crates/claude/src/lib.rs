//! The calls adroit makes to the outside: Claude Code for sessions, git for
//! worktrees, jj for what a jj repository holds, and ghq for the directories
//! sessions may start in.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use adroit_model::{ParseError, Session, Worktree};
use tokio::process::Command;

pub mod git;
pub mod history;
pub mod hook;
pub mod jj;
pub mod places;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot run {program}: {source}")]
    Spawn {
        program: String,
        source: std::io::Error,
    },
    #[error("{program} failed: {stderr}")]
    Failed { program: String, stderr: String },
    #[error(transparent)]
    Sessions(#[from] ParseError),
    #[error("process {pid} is still running")]
    StillRunning { pid: u32 },
    #[error("no process {pid} to stop")]
    NoProcess { pid: u32 },
    #[error("not permitted to stop process {pid}")]
    NotPermitted { pid: u32 },
}

/// The program used to reach Claude Code: `$ADROIT_CLAUDE`, or `claude`.
pub fn claude_program() -> String {
    std::env::var("ADROIT_CLAUDE").unwrap_or_else(|_| "claude".into())
}

/// The environment Claude Code runs in when adroit starts it: told the
/// console draws Kitty graphics, which it would not believe of a terminal
/// whose XTVERSION reply names felis rather than kitty or Ghostty; claiming
/// another name would mislead every other program asking.
pub const ENV: &[(&str, &str)] = &[("CLAUDE_CODE_FORCE_TERMINAL_IMAGES", "1")];

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

/// The sessions running in other terminals, which Claude Code lists.
pub async fn list_sessions() -> Result<Vec<Session>, Error> {
    let out = run(&claude_program(), &["agents", "--json"], None).await?;
    Ok(adroit_model::parse_agents(&out)?)
}

/// How a session is let loose on its worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Claude Code's own permission settings apply.
    Edit,
    /// Plan mode, with the tools that edit files denied.
    Read,
}

/// The arguments that start Claude Code named `name`, running `settings`,
/// with a first prompt.
pub fn start_args(name: &str, settings: &str, access: Access, prompt: &str) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--name".into(),
        name.into(),
        "--settings".into(),
        settings.into(),
    ];
    if access == Access::Read {
        args.extend(
            [
                "--permission-mode",
                "plan",
                "--disallowedTools",
                "Edit,Write,NotebookEdit",
            ]
            .map(String::from),
        );
    }
    // The prompt follows `--`, so one starting with `-` is not read as a flag.
    args.extend(["--".into(), prompt.into()]);
    args
}

/// The arguments that continue the conversation `session_id`, running
/// `settings`.
pub fn resume_args(session_id: &str, settings: &str) -> Vec<String> {
    ["--resume", session_id, "--settings", settings]
        .map(String::from)
        .to_vec()
}

/// How long a session's process is given to exit after each signal.
pub const STOP_GRACE: Duration = Duration::from_secs(3);

/// Ask a process to exit, with an interrupt first, as Ctrl-C would, and a
/// termination once `grace` passes. A process still running after both is
/// left alone and reported.
pub async fn stop(pid: u32, grace: Duration) -> Result<(), Error> {
    // kill(2) reads 0 and -1 as process groups, not one process.
    let target = i32::try_from(pid)
        .ok()
        .filter(|&p| p > 1)
        .ok_or(Error::NoProcess { pid })?;
    for signal in [libc::SIGINT, libc::SIGTERM] {
        if !alive(pid) {
            return Ok(());
        }
        // SAFETY: kill(2) takes plain integers and touches no memory.
        if unsafe { libc::kill(target, signal) } != 0 {
            return match std::io::Error::last_os_error().raw_os_error() {
                Some(libc::ESRCH) => Ok(()),
                _ => Err(Error::NotPermitted { pid }),
            };
        }
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            if !alive(pid) {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    if alive(pid) {
        Err(Error::StillRunning { pid })
    } else {
        Ok(())
    }
}

/// Whether a process exists, including one owned by someone else.
fn alive(pid: u32) -> bool {
    let Ok(target) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 only checks that the process can be signalled.
    if unsafe { libc::kill(target, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// The main checkout of the repository a worktree belongs to, from git's
/// common directory and the `core.worktree` it names: the directory holding
/// `.git` for an ordinary checkout, the work tree it names for a submodule,
/// the directory itself for a bare repository.
pub fn repo_root(common_dir: &Path, work_tree: Option<&Path>) -> PathBuf {
    if let Some(work_tree) = work_tree {
        work_tree.to_path_buf()
    } else if common_dir.file_name().is_some_and(|n| n == ".git") {
        common_dir.parent().unwrap_or(common_dir).to_path_buf()
    } else {
        common_dir.to_path_buf()
    }
}

/// Ask git what `path` is. `None` when it is not inside a git worktree.
pub async fn inspect(path: &Path) -> Option<Worktree> {
    let dir = path.to_str()?;
    let out = run(
        "git",
        &[
            "-C",
            dir,
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-common-dir",
        ],
        None,
    )
    .await
    .ok()?;
    let mut lines = out.lines();
    let top = PathBuf::from(lines.next()?);
    let common = PathBuf::from(lines.next()?);
    let work_tree = main_work_tree(&common).await;
    let branch = run("git", &["-C", dir, "branch", "--show-current"], None)
        .await
        .ok()
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty());
    Some(Worktree {
        path: top,
        repo: repo_root(&common, work_tree.as_deref()),
        branch,
    })
}

/// The work tree a repository's own config names, as a submodule's does; read
/// from the config file, as git leaves `core.worktree` unapplied in a linked
/// worktree.
async fn main_work_tree(common_dir: &Path) -> Option<PathBuf> {
    let config = common_dir.join("config");
    let out = run(
        "git",
        &[
            "config",
            "--file",
            config.to_str()?,
            "--get",
            "core.worktree",
        ],
        None,
    )
    .await
    .ok()?;
    let joined = common_dir.join(out.trim_end_matches('\n'));
    Some(joined.canonicalize().unwrap_or(joined))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_starts_named_with_its_settings_and_first_prompt() {
        assert_eq!(
            start_args("fix", "{}", Access::Edit, "-h is a flag"),
            ["--name", "fix", "--settings", "{}", "--", "-h is a flag"]
        );
    }

    #[test]
    fn a_reader_starts_in_plan_mode_without_the_editing_tools() {
        assert_eq!(
            start_args("look", "{}", Access::Read, "b"),
            [
                "--name",
                "look",
                "--settings",
                "{}",
                "--permission-mode",
                "plan",
                "--disallowedTools",
                "Edit,Write,NotebookEdit",
                "--",
                "b"
            ]
        );
    }

    #[test]
    fn a_taken_over_session_resumes_its_conversation_with_the_settings() {
        assert_eq!(
            resume_args("a08bbcf2-2cbf", "{}"),
            ["--resume", "a08bbcf2-2cbf", "--settings", "{}"]
        );
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

    /// A process that has set its signal handling up, reaped once it exits so
    /// it does not linger as a zombie.
    fn running(script: &str) -> u32 {
        use std::io::BufRead;
        let mut child = std::process::Command::new("perl")
            .args([
                "-e",
                &format!("$| = 1; {script}; print qq(ready\\n); sleep 30"),
            ])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let pid = child.id();
        std::thread::spawn(move || child.wait());
        pid
    }

    const GRACE: Duration = Duration::from_millis(300);

    #[tokio::test]
    async fn a_process_is_stopped_by_an_interrupt() {
        let pid = running("");
        stop(pid, GRACE).await.unwrap();
        assert!(!alive(pid));
    }

    #[tokio::test]
    async fn a_process_ignoring_the_interrupt_is_terminated() {
        let pid = running(r#"$SIG{INT} = "IGNORE""#);
        stop(pid, GRACE).await.unwrap();
        assert!(!alive(pid));
    }

    #[tokio::test]
    async fn a_process_that_will_not_exit_is_reported_and_left_running() {
        let pid = running(r#"$SIG{INT} = $SIG{TERM} = "IGNORE""#);
        let got = stop(pid, GRACE).await;
        let still = alive(pid);
        unsafe { libc::kill(pid as i32, libc::SIGKILL) };
        assert!(matches!(got, Err(Error::StillRunning { .. })));
        assert!(still);
    }

    /// A process of root's other than init, which this user may not signal.
    fn someone_else_s() -> Option<u32> {
        let out = std::process::Command::new("ps")
            .args(["-A", "-o", "pid=,uid="])
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout).lines().find_map(|l| {
            let mut f = l.split_whitespace();
            let pid: u32 = f.next()?.parse().ok()?;
            (f.next()? == "0" && pid > 1).then_some(pid)
        })
    }

    #[tokio::test]
    async fn a_process_this_user_may_not_signal_is_reported_at_once() {
        // SAFETY: geteuid(2) takes nothing and cannot fail.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let Some(pid) = someone_else_s() else { return };
        let started = Instant::now();
        let got = stop(pid, GRACE).await;
        assert!(matches!(got, Err(Error::NotPermitted { .. })), "{got:?}");
        assert!(started.elapsed() < GRACE);
    }

    #[tokio::test]
    async fn no_signal_is_sent_for_a_pid_that_names_a_group() {
        assert!(matches!(
            stop(0, GRACE).await,
            Err(Error::NoProcess { pid: 0 })
        ));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("adroit-claude-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_repository_of_an_ordinary_checkout_holds_its_git_directory() {
        assert_eq!(
            repo_root(Path::new("/src/api/.git"), None),
            PathBuf::from("/src/api")
        );
    }

    #[test]
    fn a_bare_repository_is_its_own_root() {
        assert_eq!(
            repo_root(Path::new("/src/api.git"), None),
            PathBuf::from("/src/api.git")
        );
    }

    #[test]
    fn a_repository_naming_its_work_tree_has_it_as_the_root() {
        assert_eq!(
            repo_root(
                Path::new("/src/app/.git/modules/lib"),
                Some(Path::new("/src/app/lib"))
            ),
            PathBuf::from("/src/app/lib")
        );
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(["-c", "protocol.file.allow=always"])
            .args(["-c", "user.name=a", "-c", "user.email=a@b"])
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    /// A repository `app` holding `lib` as a submodule; the submodule's path.
    fn submodule(name: &str) -> PathBuf {
        let dir = scratch(name).canonicalize().unwrap();
        for repo in ["lib", "app"] {
            git(&dir, &["init", "-q", "-b", "trunk", repo]);
            git(
                &dir.join(repo),
                &["commit", "-q", "--allow-empty", "-m", "c"],
            );
        }
        let lib = dir.join("lib");
        git(
            &dir.join("app"),
            &["submodule", "-q", "add", lib.to_str().unwrap(), "lib"],
        );
        dir.join("app").join("lib")
    }

    #[tokio::test]
    async fn a_submodule_is_inspected_as_its_own_main_checkout() {
        let sub = submodule("submodule");
        let w = inspect(&sub).await.unwrap();
        assert_eq!(w.path.canonicalize().unwrap(), sub);
        assert_eq!(w.repo.canonicalize().unwrap(), sub);
    }

    #[tokio::test]
    async fn a_submodule_lists_its_checkout_rather_than_its_git_directory() {
        let sub = submodule("submodule-list");
        let w = inspect(&sub).await.unwrap();
        let listed = git::worktrees(&w.repo).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, w.repo);
        assert_eq!(listed[0].repo, w.repo);
    }

    #[tokio::test]
    async fn a_git_checkout_is_inspected_as_a_worktree_of_itself() {
        let dir = scratch("git");
        let ok = std::process::Command::new("git")
            .args(["init", "-q", "-b", "trunk"])
            .current_dir(&dir)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let w = inspect(&dir).await.unwrap();
        let real = dir.canonicalize().unwrap();
        assert_eq!(w.path.canonicalize().unwrap(), real);
        assert_eq!(w.repo.canonicalize().unwrap(), real);
        assert_eq!(w.branch.as_deref(), Some("trunk"));
    }

    #[tokio::test]
    async fn a_directory_outside_git_is_not_a_worktree() {
        let dir = scratch("nogit");
        assert!(inspect(&dir).await.is_none());
    }
}
