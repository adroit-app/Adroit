use adroit::candidates::capture_files;
use adroit_model::CandidateFileState;

#[test]
fn a_manifest_identifies_contents_permissions_links_and_deleted_files_in_stable_order() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "alpha").unwrap();
    std::fs::write(dir.path().join("run.sh"), "echo hi\n").unwrap();
    std::fs::set_permissions(
        dir.path().join("run.sh"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    symlink("a.txt", dir.path().join("link")).unwrap();
    let inputs = vec![
        "run.sh".into(),
        "link".into(),
        "gone.txt".into(),
        "a.txt".into(),
    ];
    let captured = capture_files(dir.path(), &inputs).unwrap();
    let mut reversed = inputs.clone();
    reversed.reverse();
    assert_eq!(capture_files(dir.path(), &reversed).unwrap(), captured);
    assert_eq!(
        captured
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        vec!["a.txt", "gone.txt", "link", "run.sh"]
    );
    assert!(matches!(
        captured.files[1].state,
        CandidateFileState::Missing
    ));
    assert!(
        matches!(&captured.files[2].state,CandidateFileState::Symlink {target} if target == "a.txt")
    );
    assert!(matches!(
        captured.files[3].state,
        CandidateFileState::File {
            executable: true,
            ..
        }
    ));
    std::fs::write(dir.path().join("a.txt"), "changed").unwrap();
    assert_ne!(
        capture_files(dir.path(), &inputs).unwrap().fingerprint,
        captured.fingerprint
    );
}

#[test]
fn paths_outside_the_working_area_and_symlinked_parent_directories_are_refused() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "Do not read").unwrap();
    symlink(outside.path(), dir.path().join("external")).unwrap();
    for path in [
        "../secret",
        "/etc/passwd",
        "external/secret",
        ".git/config",
        ".jj/repo/config.toml",
        ".",
        "a/../secret",
    ] {
        assert!(capture_files(dir.path(), &[path.into()]).is_err(), "{path}");
    }
    // A link itself is evidence; its target is never opened.
    let evidence = capture_files(dir.path(), &["external".into()]).unwrap();
    assert!(matches!(
        evidence.files[0].state,
        CandidateFileState::Symlink { .. }
    ));
}

#[test]
fn excluded_files_do_not_invalidate_scoped_evidence_but_new_declared_files_do() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output"), "Result").unwrap();
    let inputs = vec!["output".into(), "new/input".into()];
    let before = capture_files(dir.path(), &inputs).unwrap();
    std::fs::write(dir.path().join("unrelated"), "Other Leader edits").unwrap();
    assert_eq!(capture_files(dir.path(), &inputs).unwrap(), before);
    std::fs::create_dir(dir.path().join("new")).unwrap();
    std::fs::write(dir.path().join("new/input"), "New verification input").unwrap();
    assert_ne!(
        capture_files(dir.path(), &inputs).unwrap().fingerprint,
        before.fingerprint
    );
}

#[test]
fn ambiguous_empty_duplicate_and_directory_manifests_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("folder")).unwrap();
    for paths in [
        vec![],
        vec!["".into()],
        vec!["a".into(), "a".into()],
        vec!["folder".into()],
    ] {
        assert!(capture_files(dir.path(), &paths).is_err());
    }
}

#[test]
fn alternate_path_spellings_and_case_insensitive_vcs_metadata_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    for path in ["a//b", "a/./b", "gone/", ".GIT/config", "nested/.JJ/data"] {
        assert!(capture_files(dir.path(), &[path.into()]).is_err(), "{path}");
    }
}
