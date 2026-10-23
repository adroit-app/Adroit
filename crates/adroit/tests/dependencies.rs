//! The window reaches the agents and the repositories only through the work
//! host: neither it nor the view state and protocol it is built on name the
//! crates that ask agents or run git and jj.

use std::collections::BTreeSet;
use std::path::Path;

/// The crates the window must not name.
const KEPT_OFF: [&str; 2] = ["adroit-agents", "adroit-vcs"];

/// The window's crate and those it holds the work by.
const WINDOW_SIDE: [&str; 3] = [".", "../core", "../protocol"];

/// Every crate `manifest` depends on, in any of its dependency tables, those
/// for one target too.
fn named(manifest: &str) -> BTreeSet<String> {
    let manifest: toml::Table = manifest.parse().expect("a manifest reads as TOML");
    let tables = ["dependencies", "dev-dependencies", "build-dependencies"];
    let own = tables.iter().filter_map(|t| manifest.get(*t));
    let targets = manifest
        .get("target")
        .and_then(|t| t.as_table())
        .into_iter()
        .flat_map(|targets| targets.values())
        .filter_map(|target| target.as_table())
        .flat_map(|target| tables.iter().filter_map(|t| target.get(*t)));
    own.chain(targets)
        .filter_map(|deps| deps.as_table())
        .flat_map(|deps| deps.keys().cloned())
        .collect()
}

#[test]
fn every_dependency_table_is_read_those_of_a_target_too() {
    let manifest = r#"
        [dependencies]
        adroit-core.workspace = true

        [dev-dependencies]
        tempfile.workspace = true

        [target.'cfg(target_os = "macos")'.dependencies]
        adroit-vcs.workspace = true
    "#;
    assert_eq!(
        named(manifest),
        BTreeSet::from(["adroit-core".into(), "adroit-vcs".into(), "tempfile".into()])
    );
}

#[test]
fn the_window_and_what_it_holds_the_work_by_name_neither_the_agents_nor_the_repositories() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    for dir in WINDOW_SIDE {
        let path = here.join(dir).join("Cargo.toml");
        let manifest = std::fs::read_to_string(&path).expect("the manifest reads");
        let named = named(&manifest);
        for crate_name in KEPT_OFF {
            assert!(
                !named.contains(crate_name),
                "{} depends on {crate_name}; reach it through the work host",
                path.display()
            );
        }
    }
}
