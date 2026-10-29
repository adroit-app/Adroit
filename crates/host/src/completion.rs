//! Directory completion read on the machine that owns the work.

use adroit_protocol::{HostPath, PathEntry};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

static READERS: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(8)));

pub(crate) async fn read(root: Option<HostPath>, typed: String) -> Result<Vec<PathEntry>, String> {
    let permit = READERS
        .clone()
        .try_acquire_owned()
        .map_err(|_| "too many path completion requests".to_string())?;
    tokio::time::timeout(
        Duration::from_secs(2),
        crate::environment::spawn_blocking(move || {
            let _permit = permit;
            entries(root.as_ref(), &typed)
        }),
    )
    .await
    .map_err(|_| "path completion timed out".to_string())?
    .map_err(|e| e.to_string())?
}

fn entries(root: Option<&HostPath>, typed: &str) -> Result<Vec<PathEntry>, String> {
    if typed.len() > 16384 || typed.contains('\0') {
        return Err("invalid completion path".into());
    }
    let root = match root {
        Some(root) => Some(
            root.on_local()
                .filter(|p| p.is_absolute())
                .ok_or("completion root is not on this work host or is not absolute")?,
        ),
        None => None,
    };
    let cut = typed.rfind('/').map_or(0, |p| p + 1);
    let (head, want) = typed.split_at(cut);
    let directory: PathBuf = if Path::new(head).is_absolute() {
        head.into()
    } else if let Some(root) = root {
        root.join(head)
    } else {
        return Ok(Vec::new());
    };
    let want = want.to_lowercase();
    let mut offered = Vec::new();
    let mut size = 0;
    for entry in std::fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .take(4096)
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name.starts_with('.') || !name.to_lowercase().starts_with(&want) {
            continue;
        }
        let directory = entry.path().is_dir();
        let path = format!("{head}{name}{}", if directory { "/" } else { "" });
        size += path.len();
        if size > 65536 {
            return Err("path completion exceeds its output limit".into());
        }
        offered.push(PathEntry { path, directory });
    }
    offered.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase()))
            .then_with(|| a.path.cmp(&b.path))
    });
    offered.truncate(200);
    Ok(offered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_and_absolute_paths_are_filtered_on_the_work_machine() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("README.md"), "").unwrap();
        std::fs::write(dir.path().join(".hidden"), "").unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "").unwrap();
        let root = HostPath::local(dir.path());
        let top = entries(Some(&root), "").unwrap();
        assert_eq!(
            top,
            vec![
                PathEntry {
                    path: "src/".into(),
                    directory: true
                },
                PathEntry {
                    path: "README.md".into(),
                    directory: false
                }
            ]
        );
        assert_eq!(
            entries(Some(&root), "src/M").unwrap(),
            vec![PathEntry {
                path: "src/main.rs".into(),
                directory: false
            }]
        );
        let absolute = format!("{}/src/m", dir.path().display());
        assert_eq!(
            entries(None, &absolute).unwrap(),
            vec![PathEntry {
                path: format!("{}/src/main.rs", dir.path().display()),
                directory: false
            }]
        );
        assert!(entries(None, "src/m").unwrap().is_empty());
    }

    #[test]
    fn completion_is_bounded_and_refuses_ambiguous_host_or_path_inputs() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..240 {
            std::fs::write(dir.path().join(format!("entry{i:03}")), "").unwrap();
        }
        assert_eq!(
            entries(Some(&HostPath::local(dir.path())), "")
                .unwrap()
                .len(),
            200
        );
        assert!(entries(Some(&HostPath::local("relative")), "s").is_err());
        assert!(entries(None, &"x".repeat(17000)).is_err());
        assert!(entries(None, "/tmp/\0").is_err());
    }
}
