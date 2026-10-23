//! Paths as the window writes them for people to read.

use std::path::Path;

/// `path` with the home directory written as `~`.
pub fn tilde(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_home_directory_is_written_as_a_tilde() {
        let home = Path::new("/Users/me");
        assert_eq!(tilde(Path::new("/Users/me/src/x"), home), "~/src/x");
        assert_eq!(tilde(Path::new("/Users/me"), home), "~");
        assert_eq!(tilde(Path::new("/Users/meow"), home), "/Users/meow");
        assert_eq!(tilde(Path::new("/opt/x"), home), "/opt/x");
    }
}
