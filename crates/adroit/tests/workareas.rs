use adroit::workareas::{WorkAreaRequest, plan, prepare};
use adroit_model::{WorkAreaBackend, WorkAreaId, WorkAreaState};
use std::{path::Path, process::Command};

fn jj(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("jj")
        .args([
            "--no-pager",
            "--config",
            "user.name=Test",
            "--config",
            "user.email=test@example.invalid",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[tokio::test]
async fn a_plain_directory_is_used_without_initializing_or_copying_it() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("input.txt"), "Existing input").unwrap();
    let destination = dir.path().join("unused");
    let planned = plan(WorkAreaRequest {
        id: WorkAreaId::new(),
        source: dir.path().into(),
        destination: Some(destination.clone()),
        revision: None,
        description: "Classify inputs".into(),
    })
    .await
    .unwrap();
    assert_eq!(planned.backend, WorkAreaBackend::Directory);
    assert_eq!(planned.path, dir.path().canonicalize().unwrap());
    assert_eq!(planned.state, WorkAreaState::Preparing);
    let area = prepare(&planned).await.unwrap();
    assert_eq!(area.state, WorkAreaState::Ready);
    assert!(area.path.join("input.txt").is_file());
    assert!(!destination.exists());
    assert!(!dir.path().join(".jj").exists());
    assert!(!dir.path().join(".git").exists());
}

#[tokio::test]
async fn a_jj_worker_gets_a_named_native_workspace_at_the_recorded_base() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    jj(&source, &["git", "init", "--colocate"]);
    jj(
        &source,
        &["describe", "-m", "test: prepare a workspace source"],
    );
    std::fs::write(source.join("input.txt"), "Existing input").unwrap();
    let base = jj(
        &source,
        &["log", "-r", "@", "--no-graph", "-T", "commit_id"],
    );
    let destination = dir.path().join("worker");
    let planned = plan(WorkAreaRequest {
        id: WorkAreaId::new(),
        source: source.clone(),
        destination: Some(destination.clone()),
        revision: Some(base.clone()),
        description: "adroit: implement the assigned task".into(),
    })
    .await
    .unwrap();
    assert_eq!(planned.backend, WorkAreaBackend::Jj);
    assert_eq!(planned.base_revision.as_deref(), Some(base.as_str()));
    assert!(!destination.exists());
    let ready = prepare(&planned).await.unwrap();
    assert!(ready.path.join(".jj").exists());
    assert_eq!(
        std::fs::read_to_string(ready.path.join("input.txt")).unwrap(),
        "Existing input"
    );
    assert_eq!(
        jj(
            &ready.path,
            &["log", "-r", "@-", "--no-graph", "-T", "commit_id"]
        ),
        base
    );
    assert_eq!(
        jj(
            &ready.path,
            &[
                "log",
                "-r",
                "@",
                "--no-graph",
                "-T",
                "description.first_line()"
            ]
        ),
        "adroit: implement the assigned task"
    );
    let from_worker = plan(WorkAreaRequest {
        id: WorkAreaId::new(),
        source: ready.path.clone(),
        destination: Some(dir.path().join("another")),
        revision: None,
        description: "Review".into(),
    })
    .await
    .unwrap();
    assert_eq!(planned.repository, from_worker.repository);
    assert_eq!(
        std::fs::read_to_string(source.join("input.txt")).unwrap(),
        "Existing input"
    );
}

#[tokio::test]
async fn recognized_broken_vcs_and_existing_destinations_never_fall_back_to_the_source() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".jj")).unwrap();
    assert!(
        plan(WorkAreaRequest {
            id: WorkAreaId::new(),
            source: dir.path().into(),
            destination: None,
            revision: None,
            description: "Implement".into()
        })
        .await
        .is_err()
    );
    assert!(dir.path().join(".jj").is_dir());
}

#[tokio::test]
async fn an_existing_destination_is_left_intact() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    jj(&source, &["git", "init", "--colocate"]);
    jj(
        &source,
        &["describe", "-m", "test: prepare a destination collision"],
    );
    let destination = dir.path().join("worker");
    let planned = plan(WorkAreaRequest {
        id: WorkAreaId::new(),
        source,
        destination: Some(destination.clone()),
        revision: None,
        description: "Implement".into(),
    })
    .await
    .unwrap();
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("human.txt"), "Keep me").unwrap();
    assert!(prepare(&planned).await.is_err());
    assert_eq!(
        std::fs::read_to_string(destination.join("human.txt")).unwrap(),
        "Keep me"
    );
}

#[tokio::test]
async fn the_default_jj_base_includes_the_sources_current_working_changes() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    jj(&source, &["git", "init", "--colocate"]);
    jj(
        &source,
        &["describe", "-m", "test: retain current source inputs"],
    );
    std::fs::write(source.join("current.txt"), "Current work").unwrap();
    let planned = plan(WorkAreaRequest {
        id: WorkAreaId::new(),
        source,
        destination: Some(dir.path().join("worker")),
        revision: None,
        description: "Review the current work".into(),
    })
    .await
    .unwrap();
    let ready = prepare(&planned).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(ready.path.join("current.txt")).unwrap(),
        "Current work"
    );
}

#[tokio::test]
async fn colocated_repositories_respect_the_configured_worktree_base_directory() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    jj(&source, &["git", "init", "--colocate"]);
    jj(
        &source,
        &[
            "describe",
            "-m",
            "test: honor configured workspace placement",
        ],
    );
    std::fs::OpenOptions::new()
        .append(true)
        .open(source.join(".git/config"))
        .unwrap()
        .write_all(b"\n[wt]\n basedir = ../worker-areas\n")
        .unwrap();
    let id = WorkAreaId::new();
    let planned = plan(WorkAreaRequest {
        id,
        source,
        destination: None,
        revision: None,
        description: "Implement".into(),
    })
    .await
    .unwrap();
    assert_eq!(
        planned.path,
        dir.path()
            .canonicalize()
            .unwrap()
            .join("worker-areas")
            .join(format!("adroit-{id}"))
    );
}

#[tokio::test]
async fn a_git_only_source_resolves_its_worktree_and_complete_base_without_initialization() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    jj(&source, &["git", "init", "--colocate"]);
    jj(&source, &["describe", "-m", "test: prepare a Git source"]);
    std::fs::write(source.join("input.txt"), "Existing input").unwrap();
    let base = jj(
        &source,
        &["log", "-r", "@", "--no-graph", "-T", "commit_id"],
    );
    jj(
        &source,
        &["new", "-m", "test: inspect the recorded Git base"],
    );
    std::fs::rename(source.join(".jj"), dir.path().join("jj-metadata")).unwrap();
    let planned = plan(WorkAreaRequest {
        id: WorkAreaId::new(),
        source: source.clone(),
        destination: Some(dir.path().join("worker")),
        revision: None,
        description: "Implement".into(),
    })
    .await
    .unwrap();
    assert_eq!(planned.backend, WorkAreaBackend::Git);
    assert_eq!(planned.base_revision.as_deref(), Some(base.as_str()));
    assert_eq!(planned.source_root, source.canonicalize().unwrap());
    assert!(!source.join(".jj").exists());
    assert!(!planned.path.exists());
}

#[tokio::test]
async fn an_explicit_destination_wins_over_an_invalid_default_placement() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    jj(&source, &["git", "init", "--colocate"]);
    jj(
        &source,
        &[
            "describe",
            "-m",
            "test: prefer explicit working-area placement",
        ],
    );
    std::fs::OpenOptions::new()
        .append(true)
        .open(source.join(".git/config"))
        .unwrap()
        .write_all(b"\n[wt]\n basedir = \n")
        .unwrap();
    let destination = dir.path().join("worker");
    let area = plan(WorkAreaRequest {
        id: WorkAreaId::new(),
        source,
        destination: Some(destination.clone()),
        revision: None,
        description: "Implement".into(),
    })
    .await
    .unwrap();
    assert_eq!(
        area.path,
        destination
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .join("worker")
    );
}

#[tokio::test]
async fn inspecting_an_existing_directory_returns_it_ready_without_creating_any_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("input.txt"), "Input").unwrap();
    let area = adroit::workareas::existing(WorkAreaId::new(), dir.path().into(), "Default".into())
        .await
        .unwrap();
    assert_eq!(area.backend, WorkAreaBackend::Directory);
    assert_eq!(area.state, WorkAreaState::Ready);
    assert_eq!(area.path, dir.path().canonicalize().unwrap());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn inspecting_an_existing_jj_workspace_preserves_its_files_and_history() {
    let dir = tempfile::tempdir().unwrap();
    jj(dir.path(), &["git", "init", "--colocate"]);
    jj(
        dir.path(),
        &["describe", "-m", "test: establish the default workspace"],
    );
    let before = jj(
        dir.path(),
        &["log", "-r", "@", "--no-graph", "-T", "commit_id"],
    );
    std::fs::write(dir.path().join("unsnapshotted.txt"), "Human edits").unwrap();
    let operations = jj(
        dir.path(),
        &[
            "--ignore-working-copy",
            "op",
            "log",
            "--no-graph",
            "-T",
            "id ++ \"\\n\"",
        ],
    );
    let area = adroit::workareas::existing(WorkAreaId::new(), dir.path().into(), "Default".into())
        .await
        .unwrap();
    assert_eq!(area.backend, WorkAreaBackend::Jj);
    assert_eq!(area.state, WorkAreaState::Ready);
    assert_eq!(area.path, dir.path().canonicalize().unwrap());
    assert_eq!(area.base_revision.as_deref(), Some(before.trim()));
    assert!(area.native_name.is_none());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("unsnapshotted.txt")).unwrap(),
        "Human edits"
    );
    assert_eq!(
        jj(
            dir.path(),
            &[
                "--ignore-working-copy",
                "op",
                "log",
                "--no-graph",
                "-T",
                "id ++ \"\\n\""
            ]
        ),
        operations
    );
}

#[tokio::test]
async fn inspecting_a_workspace_does_not_publish_a_merge_of_concurrent_operations() {
    let dir = tempfile::tempdir().unwrap();
    jj(dir.path(), &["git", "init", "--colocate"]);
    jj(
        dir.path(),
        &["describe", "-m", "test: establish the operation fork"],
    );
    let base = jj(
        dir.path(),
        &["op", "log", "--no-graph", "--limit", "1", "-T", "id"],
    );
    jj(
        dir.path(),
        &[
            "--at-operation",
            base.trim(),
            "describe",
            "-m",
            "test: first concurrent description",
        ],
    );
    jj(
        dir.path(),
        &[
            "--at-operation",
            base.trim(),
            "describe",
            "-m",
            "test: second concurrent description",
        ],
    );
    let heads = || {
        let output = Command::new("jj")
            .args([
                "--no-pager",
                "--ignore-working-copy",
                "--at-operation=@",
                "op",
                "log",
            ])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "the operation fork must still be unresolved"
        );
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("more than one operation"), "{error}");
        error
    };
    let before = heads();
    let inspected = adroit::workareas::existing(
        WorkAreaId::new(),
        dir.path().into(),
        "Inspect concurrent default".into(),
    )
    .await;
    assert!(
        inspected.is_err(),
        "inspection must refuse ambiguous operation heads instead of publishing a merge"
    );
    assert_eq!(heads(), before);
}

#[test]
fn machine_inspection_ignores_forced_color_without_rewriting_native_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "native_color_inspection_fixture"])
        .env("ADROIT_TEST_NATIVE_COLOR_ROOT", dir.path())
        .env("XDG_CONFIG_HOME", dir.path().join("config"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
#[ignore = "subprocess fixture with its own native configuration directory"]
async fn native_color_inspection_fixture() {
    let root = std::path::PathBuf::from(
        std::env::var_os("ADROIT_TEST_NATIVE_COLOR_ROOT").expect("subprocess fixture root"),
    );
    assert_eq!(
        std::env::var_os("XDG_CONFIG_HOME").unwrap(),
        root.join("config")
    );
    let source = root.join("source");
    std::fs::create_dir(&source).unwrap();
    jj(&source, &["git", "init", "--colocate"]);
    jj(
        &source,
        &[
            "describe",
            "-m",
            "test: inspect a colored native configuration",
        ],
    );
    jj(&source, &["config", "set", "--repo", "ui.color", "always"]);
    let config_path = jj(&source, &["config", "path", "--repo"]);
    let config_path = Path::new(config_path.trim());
    assert!(config_path.starts_with(root.join("config")));
    let config = std::fs::read(config_path).unwrap();
    let area = adroit::workareas::existing(
        WorkAreaId::new(),
        source,
        "Inspect configured default".into(),
    )
    .await
    .unwrap();
    assert!(matches!(
        area.base_revision.as_ref().unwrap().len(),
        40 | 64
    ));
    assert_eq!(std::fs::read(config_path).unwrap(), config);
}
