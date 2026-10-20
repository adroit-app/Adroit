use adroit::{
    candidates::{capture_candidate, capture_files},
    workareas,
};
use adroit_model::*;
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

fn artifact(
    area: &WorkArea,
    revision: Option<String>,
    fingerprint: Option<String>,
) -> TaskArtifact {
    TaskArtifact::Change {
        backend: area.backend,
        work_area_id: area.id,
        revision,
        manifest: vec!["output.txt".into()],
        candidate_fingerprint: fingerprint,
    }
}

#[tokio::test]
async fn directory_candidates_need_the_reported_output_fingerprint_and_capture_extra_inputs() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    std::fs::write(dir.path().join("rules.txt"), "Verification input").unwrap();
    let area = workareas::existing(WorkAreaId::new(), dir.path().into(), "Candidate".into())
        .await
        .unwrap();
    let fingerprint = capture_files(dir.path(), &["output.txt".into()])
        .unwrap()
        .fingerprint;
    let reported = artifact(&area, None, Some(fingerprint));
    let proof = capture_candidate(&area, &reported, &["rules.txt".into()])
        .await
        .unwrap();
    assert_eq!(proof.work_area_id, area.id);
    assert_eq!(proof.revision, None);
    assert_eq!(proof.files.files.len(), 2);
    std::fs::write(dir.path().join("rules.txt"), "New verification input").unwrap();
    assert_ne!(
        capture_candidate(&area, &reported, &["rules.txt".into()])
            .await
            .unwrap()
            .files
            .fingerprint,
        proof.files.fingerprint
    );
    std::fs::write(dir.path().join("output.txt"), "Changed after the report").unwrap();
    assert!(capture_candidate(&area, &reported, &[]).await.is_err());
}

#[tokio::test]
async fn a_jj_candidate_accepts_an_empty_successor_but_rejects_new_edits_without_integrating_history()
 {
    let dir = tempfile::tempdir().unwrap();
    jj(dir.path(), &["git", "init", "--colocate"]);
    jj(
        dir.path(),
        &["describe", "-m", "test: establish the candidate"],
    );
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let revision = jj(
        dir.path(),
        &["log", "-r", "@", "--no-graph", "-T", "commit_id"],
    );
    jj(dir.path(), &["new", "-m", "test: keep an empty successor"]);
    let area = workareas::existing(WorkAreaId::new(), dir.path().into(), "Candidate".into())
        .await
        .unwrap();
    let reported = artifact(&area, Some(revision.clone()), None);
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
    let proof = capture_candidate(&area, &reported, &[]).await.unwrap();
    assert_eq!(proof.revision.as_ref(), Some(&revision));
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
    std::fs::write(dir.path().join("output.txt"), "Human changed the candidate").unwrap();
    assert!(capture_candidate(&area, &reported, &[]).await.is_err());
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
    assert_eq!(
        std::fs::read_to_string(dir.path().join("output.txt")).unwrap(),
        "Human changed the candidate"
    );
}

#[tokio::test]
async fn git_candidates_reject_both_tracked_edits_and_unrecorded_new_files() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    jj(&source, &["git", "init", "--colocate"]);
    jj(
        &source,
        &["describe", "-m", "test: establish a Git candidate"],
    );
    std::fs::write(source.join("output.txt"), "Result").unwrap();
    let revision = jj(
        &source,
        &["log", "-r", "@", "--no-graph", "-T", "commit_id"],
    );
    jj(
        &source,
        &["new", "-m", "test: inspect the recorded Git candidate"],
    );
    std::fs::rename(source.join(".jj"), dir.path().join("jj-metadata")).unwrap();
    let area = workareas::existing(WorkAreaId::new(), source.clone(), "Candidate".into())
        .await
        .unwrap();
    let reported = artifact(&area, Some(revision.clone()), None);
    assert_eq!(
        capture_candidate(&area, &reported, &[])
            .await
            .unwrap()
            .revision,
        Some(revision)
    );
    std::fs::write(source.join("unrecorded.txt"), "Not part of the commit").unwrap();
    assert!(capture_candidate(&area, &reported, &[]).await.is_err());
    std::fs::remove_file(source.join("unrecorded.txt")).unwrap();
    std::fs::write(source.join("output.txt"), "Modified tracked file").unwrap();
    assert!(capture_candidate(&area, &reported, &[]).await.is_err());
}
