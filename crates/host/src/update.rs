//! Updating the Adroit this machine runs, through the Homebrew that
//! installed it.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

const CASK: &str = "adroit";
/// How long Homebrew is given for each thing it is asked.
const BREW: Duration = Duration::from_secs(8 * 60);
/// How long an Adroit just installed is given to say its version: macOS
/// scans it before it runs for the first time.
const STARTS: Duration = Duration::from_secs(60);

/// What came of an update.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Updated {
    Already(String),
    To(String),
}

/// The Homebrew of this machine, as its services may not have it on PATH.
pub(crate) fn brew() -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain(["/opt/homebrew/bin".into(), "/usr/local/bin".into()])
        .map(|dir| dir.join("brew"))
        .find(|brew| brew.is_file())
}

/// Have `brew` upgrade the Adroit that `program` is of, and check that the
/// one it installed starts.
pub(crate) async fn update(brew: &Path, program: &Path) -> Result<Updated, String> {
    let before = installed(brew, program).await?;
    asked(brew, &["update", "--quiet"], BREW).await?;
    asked(brew, &["upgrade", "--cask", CASK], BREW).await?;
    let after = version(brew).await?;
    if after == before {
        return Ok(Updated::Already(after));
    }
    asked(program, &["--version"], STARTS)
        .await
        .map_err(|why| format!("Adroit {after} is installed but does not start: {why}"))?;
    Ok(Updated::To(after))
}

/// The version of the cask, refused where `program` is not of it.
async fn installed(brew: &Path, program: &Path) -> Result<String, String> {
    let version = version(brew).await?;
    let caskroom = asked(brew, &["--caskroom"], BREW).await?;
    let app = Path::new(caskroom.trim())
        .join(CASK)
        .join(&version)
        .join("Adroit.app");
    let of_it = match (app.canonicalize(), program.canonicalize()) {
        (Ok(app), Ok(program)) => program.starts_with(app),
        _ => false,
    };
    if !of_it {
        return Err(format!(
            "{} was not installed by Homebrew, which alone can update it",
            program.display()
        ));
    }
    Ok(version)
}

async fn version(brew: &Path) -> Result<String, String> {
    let listed = asked(brew, &["list", "--cask", "--versions", CASK], BREW).await?;
    listed
        .split_whitespace()
        .last()
        .map(str::to_string)
        .ok_or_else(|| "Homebrew did not install Adroit on this machine".into())
}

/// What `program` prints, run with `args`, or why it failed.
async fn asked(program: &Path, args: &[&str], limit: Duration) -> Result<String, String> {
    let name = || {
        let args = args.join(" ");
        format!("{} {args}", program.display())
    };
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .env("NONINTERACTIVE", "1")
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let out = tokio::time::timeout(limit, command.output())
        .await
        .map_err(|_| format!("{} did not end in time", name()))?
        .map_err(|error| format!("cannot run {}: {error}", name()))?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr);
        let last = said.lines().rfind(|l| !l.trim().is_empty());
        return Err(format!("{} failed: {}", name(), last.unwrap_or_default()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    /// A Homebrew that installed 1.0 at `Adroit.app`, upgraded to `to` by
    /// `brew upgrade`, whose Adroit answers `--version` as `starts` does.
    fn machine(to: &str, starts: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        adroit_sys::fs::set_mode(dir.path(), 0o700).unwrap();
        let root = dir.path().display();
        let app = dir.path().join("Applications/Adroit.app/Contents/MacOS");
        std::fs::create_dir_all(&app).unwrap();
        for version in ["1.0", to] {
            let cask = dir.path().join("Caskroom/adroit").join(version);
            std::fs::create_dir_all(&cask).unwrap();
            let link = cask.join("Adroit.app");
            if !link.exists() {
                std::os::unix::fs::symlink(dir.path().join("Applications/Adroit.app"), link)
                    .unwrap();
            }
        }
        std::fs::write(dir.path().join("version"), "1.0").unwrap();
        let script = |path: &Path, body: &str| {
            std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
            adroit_sys::fs::set_mode(path, 0o700).unwrap();
        };
        let brew = dir.path().join("brew");
        script(
            &brew,
            &format!(
                "case \"$1\" in\n\
                 --caskroom) echo {root}/Caskroom ;;\n\
                 list) echo \"adroit $(cat {root}/version)\" ;;\n\
                 upgrade) printf {to} > {root}/version ;;\n\
                 esac"
            ),
        );
        let program = app.join("adroit");
        script(&program, starts);
        (dir, brew, program)
    }

    #[tokio::test]
    async fn a_newer_adroit_is_installed_and_one_already_newest_is_left() {
        let (_dir, brew, program) = machine("2.0", "echo adroit");
        assert_eq!(update(&brew, &program).await, Ok(Updated::To("2.0".into())));
        let (_dir, brew, program) = machine("1.0", "echo adroit");
        assert_eq!(
            update(&brew, &program).await,
            Ok(Updated::Already("1.0".into()))
        );
    }

    #[tokio::test]
    async fn an_adroit_that_does_not_start_once_installed_is_told() {
        let (_dir, brew, program) = machine("2.0", "echo refused >&2; exit 1");
        let failed = update(&brew, &program).await.unwrap_err();
        assert!(failed.contains("does not start"), "{failed}");
        assert!(failed.contains("refused"), "{failed}");
    }

    #[tokio::test]
    async fn an_adroit_homebrew_did_not_install_is_refused_and_nothing_is_upgraded() {
        let (dir, brew, program) = machine("2.0", "echo adroit");
        let other = dir.path().join("adroit");
        std::fs::copy(&program, &other).unwrap();
        assert!(update(&brew, &other).await.is_err());
        let version = std::fs::read_to_string(dir.path().join("version")).unwrap();
        assert_eq!(version, "1.0");
    }
}
