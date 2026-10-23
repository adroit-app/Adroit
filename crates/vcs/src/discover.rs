//! Which worktree and repository a directory lies in, as git names them.

use std::path::{Path, PathBuf};
use std::time::Duration;

use adroit_model::Worktree;

use crate::run;

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

/// The repository a directory belongs to, as Git names its shared directory,
/// so worktrees of one repository answer the same.
pub async fn common_dir(dir: &Path) -> Option<PathBuf> {
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new("git")
            .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
            .current_dir(dir)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    output
        .status
        .success()
        .then(|| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()))
}

/// The git directory shared by every worktree of the repository when `dir`
/// lies in a linked worktree, whose `.git` is a file naming its own git
/// directory.
pub fn linked_common_dir(dir: &Path) -> Option<PathBuf> {
    let dot_git = dir
        .ancestors()
        .map(|ancestor| ancestor.join(".git"))
        .find(|candidate| candidate.exists())?;
    let pointer = std::fs::read_to_string(&dot_git).ok()?;
    let gitdir = dot_git
        .parent()?
        .join(pointer.trim().strip_prefix("gitdir:")?.trim());
    let common = match std::fs::read_to_string(gitdir.join("commondir")) {
        Ok(relative) => gitdir.join(relative.trim()),
        Err(_) => gitdir,
    };
    common.canonicalize().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("adroit-vcs-{name}-{}", std::process::id()));
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
