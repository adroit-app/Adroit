//! Native tree checks run outside the state owner and never integrate jj snapshots.
use super::{Result, capture_files};
use adroit_model::*;
use std::{
    path::Path,
    process::{ExitStatus, Stdio},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt};

async fn drain(
    mut stream: impl AsyncRead + Unpin,
    limit: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut bytes = Vec::new();
    let mut overflow = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            return Ok((bytes, overflow));
        }
        let keep = count.min(limit.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..keep]);
        overflow |= count > keep;
    }
}

async fn output(
    root: &Path,
    program: &str,
    args: &[&str],
) -> Result<(ExitStatus, Vec<u8>, String)> {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_EXTERNAL_DIFF",
    ] {
        command.env_remove(key);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start candidate check {program}: {error}"))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::try_join!(
            child.wait(),
            drain(stdout, 2 * 1024 * 1024),
            drain(stderr, 64 * 1024)
        )
    })
    .await;
    match result {
        Ok(Ok((status, (stdout, false), (stderr, _)))) => Ok((
            status,
            stdout,
            String::from_utf8_lossy(&stderr).trim().into(),
        )),
        result => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(match result {
                Err(_) => format!("candidate check {program} timed out"),
                Ok(Err(error)) => format!("candidate check {program} failed: {error}"),
                Ok(Ok(_)) => format!("candidate check {program} exceeded its output limit"),
            })
        }
    }
}

async fn command(root: &Path, program: &str, args: &[&str]) -> Result<Vec<u8>> {
    let (status, stdout, stderr) = output(root, program, args).await?;
    if !status.success() {
        return Err(format!("candidate check {program} failed: {stderr}"));
    }
    Ok(stdout)
}

async fn matches_revision(root: &Path, backend: WorkAreaBackend, revision: &str) -> Result<()> {
    match backend {
        WorkAreaBackend::Jj => {
            let changed = command(
                root,
                "jj",
                &[
                    "--no-pager",
                    "--color=never",
                    "--no-integrate-operation",
                    "diff",
                    "--from",
                    revision,
                    "--to",
                    "@",
                    "--name-only",
                ],
            )
            .await?;
            if !changed.is_empty() {
                return Err("the jj working tree changed from the candidate revision".into());
            }
            let conflicts = command(
                root,
                "jj",
                &[
                    "--no-pager",
                    "--ignore-working-copy",
                    "log",
                    "-r",
                    revision,
                    "--no-graph",
                    "-T",
                    "conflict",
                ],
            )
            .await?;
            if conflicts != b"false" {
                return Err("the jj candidate has unresolved conflicts".into());
            }
        }
        WorkAreaBackend::Git => {
            let (status, _, stderr) = output(
                root,
                "git",
                &[
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--quiet",
                    revision,
                    "--",
                ],
            )
            .await?;
            if !status.success() {
                return Err(format!(
                    "the Git working tree differs from the candidate or cannot be checked: {stderr}"
                ));
            }
            let untracked = command(
                root,
                "git",
                &["ls-files", "--others", "--exclude-standard", "-z"],
            )
            .await?;
            if !untracked.is_empty() {
                return Err("the Git working tree contains unrecorded files".into());
            }
        }
        WorkAreaBackend::Directory => unreachable!("directory candidates use file evidence"),
    }
    Ok(())
}

async fn paths(root: &Path, backend: WorkAreaBackend, revision: &str) -> Result<Vec<String>> {
    let bytes = match backend {
        WorkAreaBackend::Jj => {
            command(
                root,
                "jj",
                &[
                    "--no-pager",
                    "--ignore-working-copy",
                    "file",
                    "list",
                    "-r",
                    revision,
                    "-T",
                    "path ++ \"\\0\"",
                ],
            )
            .await?
        }
        WorkAreaBackend::Git => {
            command(
                root,
                "git",
                &["ls-tree", "-r", "-z", "--name-only", revision],
            )
            .await?
        }
        WorkAreaBackend::Directory => unreachable!("directory paths come from their manifest"),
    };
    bytes
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            String::from_utf8(path.into()).map_err(|_| "a candidate path is not UTF-8".into())
        })
        .collect()
}

async fn files(root: &Path, paths: Vec<String>) -> Result<CandidateFiles> {
    let root = root.to_owned();
    tokio::task::spawn_blocking(move || capture_files(&root, &paths))
        .await
        .map_err(|error| format!("candidate file capture failed: {error}"))?
}

pub async fn capture_candidate(
    area: &WorkArea,
    artifact: &TaskArtifact,
    inputs: &[String],
) -> Result<CandidateProof> {
    let TaskArtifact::Change {
        backend,
        work_area_id,
        revision,
        manifest,
        candidate_fingerprint,
    } = artifact
    else {
        return Err("a candidate needs a change artifact".into());
    };
    if area.id != *work_area_id || area.backend != *backend || area.state != WorkAreaState::Ready {
        return Err("the candidate working area is unavailable or mismatched".into());
    }
    let actual =
        crate::workareas::identify(area.id, area.path.clone(), "Inspect candidate".into()).await?;
    if actual.path != area.path
        || actual.backend != area.backend
        || actual.repository != area.repository
    {
        return Err("the candidate working-area identity changed".into());
    }
    let mut declared = manifest.clone();
    if *backend == WorkAreaBackend::Directory {
        if revision.is_some() {
            return Err("a directory candidate cannot select a VCS revision".into());
        }
        let outputs = files(&area.path, manifest.clone()).await?;
        if candidate_fingerprint.as_ref() != Some(&outputs.fingerprint) {
            return Err("the reported directory output fingerprint is stale or unsupported".into());
        }
    } else {
        let revision = revision
            .as_deref()
            .filter(|value| {
                matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            .ok_or("a native candidate needs a full immutable commit ID")?;
        matches_revision(&area.path, *backend, revision).await?;
        declared.extend(paths(&area.path, *backend, revision).await?);
    }
    declared.extend_from_slice(inputs);
    declared.sort();
    declared.dedup();
    let evidence = files(&area.path, declared).await?;
    if *backend == WorkAreaBackend::Directory {
        let outputs = files(&area.path, manifest.clone()).await?;
        if candidate_fingerprint.as_ref() != Some(&outputs.fingerprint) {
            return Err("directory outputs changed during candidate capture".into());
        }
        if outputs
            .files
            .iter()
            .any(|output| !evidence.files.contains(output))
        {
            return Err("directory outputs changed during candidate capture".into());
        }
    } else {
        matches_revision(
            &area.path,
            *backend,
            revision.as_deref().expect("validated revision"),
        )
        .await?;
    }
    Ok(CandidateProof {
        work_area_id: area.id,
        backend: *backend,
        revision: revision.clone(),
        files: evidence,
    })
}
