//! What the daemon tests share.

/// A folder of the test's own for a socket: under `/tmp`, since a socket's
/// path has to be short, and open to no one else, since the daemon listens
/// only in such a folder.
pub fn socket_dir() -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix("adroit-")
        .tempdir_in("/tmp")
        .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

/// `script` run by `/bin/sh` in `/tmp` on a 40 by 6 screen.
pub fn sh(script: &str) -> adroit_term::Spawn {
    adroit_term::Spawn {
        program: "/bin/sh".into(),
        args: vec!["-c".into(), script.into()],
        cwd: "/tmp".into(),
        cols: 40,
        rows: 6,
        ..adroit_term::Spawn::default()
    }
}
