//! Finding the file a program runs from: on the `PATH` it is started with,
//! by the executable bit on Unix and by `PATHEXT` on Windows.

/// Whether `program` is one that can run when started in `cwd` with `env`:
/// one written with a `/` taken from `cwd`, any other looked up on the last
/// `PATH` `env` sets, else on this process's own.
pub fn runnable(program: &str, cwd: &std::path::Path, env: &[(String, String)]) -> bool {
    resolved(program, cwd, env).is_some()
}

/// The file `program` runs from when started in `cwd` with `env`, found as
/// [`runnable`] says.
pub fn resolved(
    program: &str,
    cwd: &std::path::Path,
    env: &[(String, String)],
) -> Option<std::path::PathBuf> {
    if program.contains('/') || cfg!(windows) && program.contains('\\') {
        return runs(&cwd.join(program));
    }
    let path = env
        .iter()
        .rev()
        .find(|(k, _)| k == "PATH")
        .map(|(_, v)| v.clone())
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
    std::env::split_paths(&path).find_map(|dir| runs(&dir.join(program)))
}

/// `path`, when it is a file that can run.
#[cfg(unix)]
fn runs(path: &std::path::Path) -> Option<std::path::PathBuf> {
    use crate::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .then(|| path.to_path_buf())
}

/// `path`, or `path` with an extension `PATHEXT` names, when it is a file:
/// Windows has no executable bit, runs a program by its extension, and lets
/// it be named without one.
#[cfg(windows)]
fn runs(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let extensions = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    let named = |p: &std::path::Path| {
        p.extension().is_some_and(|e| {
            extensions.split(';').any(|x| {
                x.trim_start_matches('.')
                    .eq_ignore_ascii_case(&e.to_string_lossy())
            })
        }) && std::fs::metadata(p).is_ok_and(|m| m.is_file())
    };
    if named(path) {
        return Some(path.to_path_buf());
    }
    extensions
        .split(';')
        .filter(|x| !x.is_empty())
        .map(|x| {
            let mut with = path.as_os_str().to_owned();
            with.push(x);
            std::path::PathBuf::from(with)
        })
        .find(|with| named(with))
}
