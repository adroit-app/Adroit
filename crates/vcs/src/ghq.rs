//! The repositories ghq keeps, which a new session may be started in.

use std::path::PathBuf;

use adroit_model::CloneTarget;

use crate::run;

/// The repositories ghq keeps, or none when ghq is not installed.
pub async fn list() -> Vec<PathBuf> {
    lines(run("ghq", &["list", "--full-path"], None).await.ok())
}

/// The folders ghq keeps repositories under.
pub async fn roots() -> Vec<PathBuf> {
    lines(run("ghq", &["root", "--all"], None).await.ok())
}

/// The program and arguments that clone `target`, run where the user can
/// answer what the clone asks.
pub fn get(target: &CloneTarget) -> (String, Vec<String>) {
    ("ghq".into(), vec!["get".into(), target.typed.clone()])
}

/// Where ghq keeps `target`, if it keeps it.
pub async fn kept(target: &CloneTarget) -> Option<PathBuf> {
    target.kept_in(&list().await).cloned()
}

fn lines(out: Option<String>) -> Vec<PathBuf> {
    out.unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listing_is_one_path_a_line() {
        assert_eq!(
            lines(Some("/a\n\n /b \n".into())),
            vec![PathBuf::from("/a"), PathBuf::from("/b")]
        );
        assert!(lines(None).is_empty());
    }

    #[test]
    fn a_repository_is_cloned_by_what_was_typed() {
        let target = CloneTarget::parse("git@github.com:o/r.git").unwrap();
        assert_eq!(
            get(&target),
            (
                "ghq".into(),
                vec!["get".into(), "git@github.com:o/r.git".into()]
            )
        );
    }
}
