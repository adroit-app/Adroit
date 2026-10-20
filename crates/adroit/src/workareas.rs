//! Native working areas prepared before a Worker can edit anything.
use std::{
    ffi::OsStr,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use adroit_model::{WorkArea, WorkAreaBackend, WorkAreaId, WorkAreaState};

pub struct WorkAreaRequest {
    pub id: WorkAreaId,
    pub source: PathBuf,
    pub destination: Option<PathBuf>,
    pub revision: Option<String>,
    pub description: String,
}

type Result<T> = std::result::Result<T, String>;

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
        .map_err(|_| {
            format!("{program} timed out; inspect the recorded working-area path before retrying")
        })?
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

async fn base_directory(
    root: &Path,
    repository: &Path,
    backend: WorkAreaBackend,
) -> Result<PathBuf> {
    let anchor = match backend {
        WorkAreaBackend::Jj => repository
            .parent()
            .filter(|parent| parent.file_name().is_some_and(|name| name == ".jj"))
            .and_then(Path::parent),
        WorkAreaBackend::Git => (repository.file_name().is_some_and(|name| name == ".git"))
            .then(|| repository.parent())
            .flatten(),
        WorkAreaBackend::Directory => None,
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

fn destination(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err("the working-area destination must be absolute".into());
    }
    if path
        .components()
        .any(|part| matches!(part, Component::Normal(name) if name == ".git" || name == ".jj"))
    {
        return Err("a working area cannot be created inside VCS metadata".into());
    }
    let mut parent = path;
    let mut suffix = Vec::new();
    while !exists(parent)? {
        suffix.push(
            parent
                .file_name()
                .ok_or("the destination has no file name")?
                .to_owned(),
        );
        parent = parent.parent().ok_or("the destination has no parent")?;
    }
    let mut resolved = canonical(parent)?;
    for component in suffix.into_iter().rev() {
        resolved.push(component);
    }
    if resolved
        .components()
        .any(|part| matches!(part, Component::Normal(name) if name == ".git" || name == ".jj"))
    {
        return Err("a working area cannot be created inside VCS metadata".into());
    }
    Ok(resolved)
}

async fn jj(dir: &Path, args: &[&OsStr]) -> Result<String> {
    let mut full = vec![
        OsStr::new("--no-pager"),
        OsStr::new("--ignore-working-copy"),
    ];
    full.extend_from_slice(args);
    command(dir, "jj", &full).await
}

async fn revision(
    root: &Path,
    backend: WorkAreaBackend,
    requested: &str,
    snapshot: bool,
) -> Result<String> {
    let output = match backend {
        WorkAreaBackend::Jj => {
            let args: &[&OsStr] = &[
                "log".as_ref(),
                "-r".as_ref(),
                requested.as_ref(),
                "--no-graph".as_ref(),
                "-T".as_ref(),
                "(commit_id ++ \"\\n\")".as_ref(),
            ];
            if snapshot {
                command(root, "jj", args).await?
            } else {
                jj(root, args).await?
            }
        }
        WorkAreaBackend::Git => {
            command(
                root,
                "git",
                &[
                    "rev-parse".as_ref(),
                    "--verify".as_ref(),
                    format!("{requested}^{{commit}}").as_ref(),
                ],
            )
            .await?
        }
        WorkAreaBackend::Directory => {
            return Err("ordinary directories have no VCS revision".into());
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
        _ => Err("the working-area base must resolve to exactly one complete revision".into()),
    }
}

async fn identify(id: WorkAreaId, source: PathBuf, description: String) -> Result<WorkArea> {
    if description.trim().is_empty() {
        return Err("a working area needs a task description".into());
    }
    let source = canonical(&source)?;
    if !source.is_dir() {
        return Err("the source must be an existing directory".into());
    }
    let mut backend = WorkAreaBackend::Directory;
    let mut root = source.clone();
    for parent in source.ancestors() {
        if exists(&parent.join(".jj"))? {
            backend = WorkAreaBackend::Jj;
            root = parent.into();
            break;
        }
        if exists(&parent.join(".git"))? {
            backend = WorkAreaBackend::Git;
            root = parent.into();
            break;
        }
    }
    let repository = match backend {
        WorkAreaBackend::Jj => {
            let reported = jj(&source, &["root".as_ref()]).await?;
            root = canonical(Path::new(reported.trim()))?;
            let metadata = root.join(".jj/repo");
            let repository = if metadata.is_dir() {
                metadata
            } else {
                let pointer = std::fs::read_to_string(&metadata)
                    .map_err(|error| format!("cannot read the jj repository identity: {error}"))?;
                root.join(".jj").join(pointer.trim())
            };
            Some(canonical(&repository)?)
        }
        WorkAreaBackend::Git => {
            let reported = command(
                &source,
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
            root = canonical(Path::new(top))?;
            Some(canonical(Path::new(common))?)
        }
        WorkAreaBackend::Directory => None,
    };
    Ok(WorkArea {
        id,
        backend,
        path: root.clone(),
        source,
        source_root: root,
        repository,
        base_revision: None,
        native_name: None,
        description,
        state: WorkAreaState::Ready,
        error: None,
    })
}

pub async fn existing(id: WorkAreaId, source: PathBuf, description: String) -> Result<WorkArea> {
    let mut area = identify(id, source, description).await?;
    if area.backend != WorkAreaBackend::Directory {
        let requested = if area.backend == WorkAreaBackend::Jj {
            "@"
        } else {
            "HEAD"
        };
        area.base_revision = Some(revision(&area.path, area.backend, requested, false).await?);
    }
    Ok(area)
}

pub async fn plan(request: WorkAreaRequest) -> Result<WorkArea> {
    let mut area = identify(request.id, request.source, request.description).await?;
    let backend = area.backend;
    let source = area.source.clone();
    let root = area.source_root.clone();
    let repository = area.repository.clone();
    let (path, base_revision, native_name) = if backend == WorkAreaBackend::Directory {
        if request.revision.is_some() {
            return Err("an ordinary directory cannot select a VCS revision".into());
        }
        (source.clone(), None, None)
    } else {
        let name = format!("adroit-{}", request.id);
        let path = if let Some(path) = request.destination.as_deref() {
            destination(path)?
        } else {
            let base = base_directory(
                &root,
                repository
                    .as_deref()
                    .ok_or("the repository identity is missing")?,
                backend,
            )
            .await?;
            destination(&base.join(&name))?
        };
        if exists(&path)? {
            return Err("the working-area destination already exists".into());
        }
        let default = if backend == WorkAreaBackend::Jj {
            "@"
        } else {
            "HEAD"
        };
        let base = revision(
            &root,
            backend,
            request.revision.as_deref().unwrap_or(default),
            request.revision.is_none(),
        )
        .await?;
        (path, Some(base), Some(name))
    };
    area.path = path;
    area.base_revision = base_revision;
    area.native_name = native_name;
    area.state = WorkAreaState::Preparing;
    Ok(area)
}

pub async fn prepare(area: &WorkArea) -> Result<WorkArea> {
    if area.state != WorkAreaState::Preparing {
        return Err("the working area is not preparing".into());
    }
    match area.backend {
        WorkAreaBackend::Directory => {
            if canonical(&area.path)? != area.path || !area.path.is_dir() {
                return Err("the source directory changed".into());
            }
            for parent in area.path.ancestors() {
                if exists(&parent.join(".jj"))? || exists(&parent.join(".git"))? {
                    return Err(
                        "the directory acquired VCS metadata; prepare a new working-area plan"
                            .into(),
                    );
                }
            }
        }
        backend => {
            if exists(&area.path)? {
                return Err(
                    "the working-area destination already exists; it will not be overwritten"
                        .into(),
                );
            }
            let parent = area.path.parent().ok_or("the working area has no parent")?;
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create the working-area parent: {error}"))?;
            if canonical(parent)? != parent {
                return Err("the working-area parent changed after planning".into());
            }
            let base = area
                .base_revision
                .as_deref()
                .ok_or("the working area has no recorded base")?;
            let name = area
                .native_name
                .as_deref()
                .ok_or("the working area has no native name")?;
            match backend {
                WorkAreaBackend::Jj => {
                    command(
                        &area.source_root,
                        "jj",
                        &[
                            "--no-pager".as_ref(),
                            "workspace".as_ref(),
                            "add".as_ref(),
                            "--name".as_ref(),
                            name.as_ref(),
                            "--revision".as_ref(),
                            base.as_ref(),
                            "--message".as_ref(),
                            area.description.as_ref(),
                            area.path.as_os_str(),
                        ],
                    )
                    .await?;
                }
                WorkAreaBackend::Git => {
                    command(
                        &area.source_root,
                        "git",
                        &[
                            "worktree".as_ref(),
                            "add".as_ref(),
                            "-b".as_ref(),
                            name.as_ref(),
                            area.path.as_os_str(),
                            base.as_ref(),
                        ],
                    )
                    .await?;
                }
                WorkAreaBackend::Directory => unreachable!(),
            }
            if canonical(&area.path)? != area.path {
                return Err("the working-area destination changed during preparation".into());
            }
            let parent_revision = if backend == WorkAreaBackend::Jj {
                "@-"
            } else {
                "HEAD"
            };
            if revision(&area.path, backend, parent_revision, false).await? != base {
                return Err("the prepared working area has an unexpected base".into());
            }
        }
    }
    let mut ready = area.clone();
    ready.state = WorkAreaState::Ready;
    Ok(ready)
}
