//! The repositories ghq keeps, which a new session may be started in.

use std::path::PathBuf;

use crate::run;

/// The repositories ghq keeps, or none when ghq is not installed.
pub async fn list() -> Vec<PathBuf> {
    lines(run("ghq", &["list", "--full-path"], None).await.ok())
}

/// The folders ghq keeps repositories under.
pub async fn roots() -> Vec<PathBuf> {
    lines(run("ghq", &["root", "--all"], None).await.ok())
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
}
