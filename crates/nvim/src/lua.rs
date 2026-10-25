//! The Lua adroit gives its Neovims, as modules under `adroit.` that
//! `require` finds before the user's configuration runs, and the Neovim
//! they run in.

/// Every module of adroit's, by the name `require` takes, with its source.
const MODULES: &[(&str, &str)] = &[
    ("adroit.hold", include_str!("../lua/adroit/hold.lua")),
    ("adroit.config", include_str!("../lua/adroit/config.lua")),
    ("adroit.offer", include_str!("../lua/adroit/offer.lua")),
    ("adroit.server", include_str!("../lua/adroit/server.lua")),
    ("adroit.keys", include_str!("../lua/adroit/keys.lua")),
    (
        "adroit.complete",
        include_str!("../lua/adroit/complete.lua"),
    ),
    ("adroit.prompt", include_str!("../lua/adroit/prompt.lua")),
    (
        "adroit.prompt_defaults",
        include_str!("../lua/adroit/prompt_defaults.lua"),
    ),
    ("adroit.reader", include_str!("../lua/adroit/reader.lua")),
    (
        "adroit.reader_defaults",
        include_str!("../lua/adroit/reader_defaults.lua"),
    ),
];

/// The Neovim adroit runs: the program `ADROIT_NVIM` names, else `nvim`
/// found on `PATH`. Tests run it too, so that a Neovim to be shipped is
/// tested by naming it.
pub fn program() -> String {
    std::env::var("ADROIT_NVIM").unwrap_or_else(|_| "nvim".into())
}

/// The version of the Neovim adroit runs, as it names itself, such as
/// `0.12.6`; none when it cannot be run.
pub async fn version() -> Option<String> {
    let out = tokio::process::Command::new(program())
        .arg("--version")
        .kill_on_drop(true)
        .output()
        .await
        .ok()?;
    named(&String::from_utf8_lossy(&out.stdout))
}

/// The version `nvim --version` printed as `out` names.
fn named(out: &str) -> Option<String> {
    let first = out.lines().next()?;
    first.strip_prefix("NVIM v").map(str::to_string)
}

/// Lua that lets `require` find every module of adroit's, one `:lua`
/// command long, as it has to be given before the user's configuration.
pub(crate) fn preload() -> String {
    MODULES
        .iter()
        .map(|(name, source)| {
            let level = "=".repeat(level(source));
            format!(
                "package.preload[{name:?}] = assert(loadstring([{level}[\n{source}]{level}], \"@{name}\"))\n"
            )
        })
        .collect()
}

/// Lua that requires `module` once every module is found, as a `:lua`
/// command takes it.
pub(crate) fn require(module: &str) -> String {
    format!("{}require({module:?})", preload())
}

/// The fewest `=` a long bracket needs to hold `source` whole.
fn level(source: &str) -> usize {
    (0..)
        .find(|n| !source.contains(&format!("]{}]", "=".repeat(*n))))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use super::*;

    #[test]
    fn the_version_is_what_neovim_names_itself_by() {
        assert_eq!(
            named("NVIM v0.12.6\nBuild type: Release\n"),
            Some("0.12.6".into())
        );
        assert_eq!(
            named("NVIM v0.13.0-dev-412+g1a2b3c4\n"),
            Some("0.13.0-dev-412+g1a2b3c4".into())
        );
        assert_eq!(named("vim 9.1\n"), None);
    }

    #[tokio::test]
    async fn the_neovim_adroit_runs_tells_its_version() {
        let version = version().await.expect("the Neovim adroit runs");
        assert!(version.starts_with("0."), "{version}");
    }

    #[test]
    fn a_long_bracket_holds_a_source_that_closes_shorter_ones() {
        assert_eq!(level("x"), 0);
        assert_eq!(level("t[1]] ]=]"), 2);
    }

    fn specs() -> Vec<PathBuf> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("lua/spec");
        let mut found: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.to_string_lossy().ends_with("_spec.lua"))
            .collect();
        found.sort();
        found
    }

    /// Every Lua spec, run in the Neovim adroit runs with adroit's modules
    /// found, passes: its rules hold for that Neovim.
    #[tokio::test]
    async fn every_lua_spec_passes_in_the_neovim_adroit_runs() {
        let runner = Path::new(env!("CARGO_MANIFEST_DIR")).join("lua/spec/runner.lua");
        let specs = specs();
        assert!(!specs.is_empty());
        let mut failed = Vec::new();
        for spec in specs {
            let run = tokio::process::Command::new(program())
                .args(["--clean", "--headless", "--cmd"])
                .arg(format!("lua {}", preload()))
                .arg("-l")
                .arg(&runner)
                .arg(&spec)
                .kill_on_drop(true)
                .output();
            let out = tokio::time::timeout(Duration::from_secs(30), run)
                .await
                .unwrap_or_else(|_| panic!("{} timed out", spec.display()))
                .expect("the Neovim adroit runs");
            if !out.status.success() {
                failed.push(format!(
                    "{}:\n{}{}",
                    spec.display(),
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
        }
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }
}
