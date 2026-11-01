//! What the daemon tests share.

/// A folder of the test's own for a socket: one whose path is short, since a
/// socket's path has to be, and open to no one else, since the daemon listens
/// only in such a folder.
pub fn socket_dir() -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix("adroit-")
        .tempdir_in(adroit_sys::local::temp_dir())
        .unwrap();
    adroit_sys::fs::set_mode(dir.path(), 0o700).unwrap();
    dir
}

/// `script` run by `/bin/sh` in `/tmp` on a 40 by 6 screen.
#[cfg(unix)]
pub fn sh(script: &str) -> adroit_term::Spawn {
    shell("/bin/sh", "/tmp".into(), script)
}

/// `script` run by the `sh` Git for Windows carries, found on `PATH`, in the
/// temporary folder on a 40 by 6 screen.
#[cfg(windows)]
pub fn sh(script: &str) -> adroit_term::Spawn {
    shell("sh", std::env::temp_dir(), script)
}

fn shell(program: &str, cwd: std::path::PathBuf, script: &str) -> adroit_term::Spawn {
    adroit_term::Spawn {
        program: program.into(),
        args: vec!["-c".into(), script.into()],
        cwd,
        cols: 40,
        rows: 6,
        ..adroit_term::Spawn::default()
    }
}
