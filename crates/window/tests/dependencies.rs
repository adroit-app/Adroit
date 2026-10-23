//! The window reaches the work, the agents and the repositories only through
//! the work host's socket: neither it nor the view state and protocol it is
//! built on depend, however deep, on the crates that hold the work, ask
//! agents or run git and jj.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The crates the window must not reach.
const KEPT_OFF: [&str; 3] = ["adroit-host", "adroit-agents", "adroit-vcs"];

/// The window's crate and those it holds the work by.
const WINDOW_SIDE: [&str; 3] = ["adroit-window", "adroit-core", "adroit-protocol"];

/// The dependency tables a crate builds with; a test's own are read for the
/// crate the walk starts from alone.
const BUILT: [&str; 2] = ["dependencies", "build-dependencies"];
const TESTED: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

/// Every crate `manifest` depends on in the `tables` named, those for one
/// target too.
fn named(manifest: &toml::Table, tables: &[&str]) -> BTreeSet<String> {
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

/// Every crate of the workspace, among `manifests` by name, that `from`
/// reaches: what it depends on, its tests' included, and what those depend
/// on to build, however deep.
fn reached(manifests: &BTreeMap<String, toml::Table>, from: &str) -> BTreeSet<String> {
    let mut reached = BTreeSet::new();
    let mut next: Vec<String> = manifests
        .get(from)
        .map(|m| named(m, &TESTED).into_iter().collect())
        .unwrap_or_default();
    while let Some(name) = next.pop() {
        if !reached.insert(name.clone()) {
            continue;
        }
        if let Some(manifest) = manifests.get(&name) {
            next.extend(named(manifest, &BUILT));
        }
    }
    reached
}

/// Every crate of the workspace's manifest, by the name it goes by.
fn workspace() -> BTreeMap<String, toml::Table> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    std::fs::read_dir(&crates)
        .expect("the crates folder reads")
        .filter_map(|entry| std::fs::read_to_string(entry.ok()?.path().join("Cargo.toml")).ok())
        .map(|text| {
            let manifest: toml::Table = text.parse().expect("a manifest reads as TOML");
            let name = manifest["package"]["name"]
                .as_str()
                .expect("a crate is named")
                .to_string();
            (name, manifest)
        })
        .collect()
}

fn manifest(text: &str) -> toml::Table {
    text.parse().unwrap()
}

#[test]
fn every_dependency_table_is_read_those_of_a_target_too() {
    let manifest = manifest(
        r#"
        [dependencies]
        adroit-core.workspace = true

        [dev-dependencies]
        tempfile.workspace = true

        [target.'cfg(target_os = "macos")'.dependencies]
        adroit-vcs.workspace = true
    "#,
    );
    assert_eq!(
        named(&manifest, &TESTED),
        BTreeSet::from(["adroit-core".into(), "adroit-vcs".into(), "tempfile".into()])
    );
}

#[test]
fn a_crate_reaches_what_its_dependencies_build_with_but_not_what_their_tests_do() {
    let manifests = BTreeMap::from([
        ("a".to_string(), manifest("[dev-dependencies]\nb = '1'\n")),
        (
            "b".to_string(),
            manifest("[dependencies]\nc = '1'\n[dev-dependencies]\nd = '1'\n"),
        ),
        (
            "c".to_string(),
            manifest("[target.'cfg(unix)'.build-dependencies]\ne = '1'\n"),
        ),
    ]);
    assert_eq!(
        reached(&manifests, "a"),
        BTreeSet::from(["b".into(), "c".into(), "e".into()])
    );
}

#[test]
fn the_window_and_what_it_holds_the_work_by_reach_neither_the_work_the_agents_nor_the_repositories()
{
    let manifests = workspace();
    // The binary starts the hosts, so it reaches them all.
    let binary = reached(&manifests, "adroit");
    assert!(KEPT_OFF.iter().all(|c| binary.contains(*c)), "{binary:?}");
    for from in WINDOW_SIDE {
        assert!(manifests.contains_key(from), "{from} is in the workspace");
        let reached = reached(&manifests, from);
        for crate_name in KEPT_OFF {
            assert!(
                !reached.contains(crate_name),
                "{from} reaches {crate_name}; reach it through the work host"
            );
        }
    }
}
