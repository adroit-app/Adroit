//! The executable installed on a work machine, built without the window.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[test]
fn a_headless_build_does_not_link_the_window_or_its_desktop_runtime() {
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--offline",
            "--manifest-path",
            concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
            "--no-default-features",
            "--edges",
            "normal",
            "--prefix",
            "none",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8(output.stdout).unwrap();
    assert!(!tree.contains("adroit-window"), "{tree}");
    assert!(!tree.contains("dioxus"), "{tree}");
    assert!(!tree.contains("wry "), "{tree}");
}

#[test]
fn the_headless_executable_reports_its_version_without_starting_a_host() {
    let dir = tempfile::tempdir().unwrap();
    let output = isolated(dir.path()).arg("--version").output().unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("adroit ")
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[cfg(not(feature = "desktop"))]
#[test]
fn a_headless_build_refuses_window_and_editor_requests_without_starting_hosts() {
    for args in [
        vec![],
        vec!["--new-window"],
        vec!["editor", "/tmp/prompt.md"],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let output = isolated(dir.path()).args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("no window")
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}

/// A process whose hosts and data belong to this test alone.
fn isolated(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_adroit"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("ADROIT_") || name == "CODEX_THREAD_ID" {
            command.env_remove(name);
        }
    }
    command
        .env("XDG_STATE_HOME", dir.join("state"))
        .env("XDG_DATA_HOME", dir.join("data"))
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("ADROIT_HOST", dir.join("work.sock"))
        .env("ADROIT_DAEMON", dir.join("term.sock"))
        .env("ADROIT_INSTANCE", "headless-test");
    command
}

struct Hosts(tempfile::TempDir);

impl Hosts {
    fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("adroit-h-")
            .tempdir_in("/tmp")
            .unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        Self(dir)
    }

    fn run(&self, args: &[&str]) -> serde_json::Value {
        let output = isolated(self.0.path()).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

impl Drop for Hosts {
    fn drop(&mut self) {
        // The terminal host outlives the work host, including a failed test.
        for command in ["daemon", "host"] {
            let _ = isolated(self.0.path()).args([command, "stop"]).output();
        }
    }
}

#[test]
fn the_headless_cli_keeps_a_terminal_through_replacing_its_work_host() {
    let hosts = Hosts::new();
    let terminal = hosts.run(&[
        "terminal",
        "start",
        "--name",
        "kept",
        "--dir",
        hosts.0.path().to_str().unwrap(),
        "--program",
        "command",
        "--json",
        "--",
        "/bin/sh",
        "-c",
        "printf 'before-restart\\n'; while IFS= read -r line; do printf '%s\\n' \"$line\"; done",
    ]);
    let id = terminal["resource_id"].as_str().unwrap();
    let stopped = isolated(hosts.0.path())
        .args(["host", "stop"])
        .output()
        .unwrap();
    assert!(stopped.status.success());
    let input = hosts.run(&[
        "terminal",
        "input",
        "--terminal",
        id,
        "--text",
        "after-restart",
        "--keys",
        "Enter",
        "--json",
    ]);
    assert_eq!(input["sent"], true);
    let mut screen = String::new();
    for _ in 0..100 {
        screen = hosts
            .run(&["terminal", "screen", "--terminal", id, "--json"])
            .to_string();
        if screen.contains("before-restart") && screen.contains("after-restart") {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(screen.contains("before-restart"), "{screen}");
    assert!(screen.contains("after-restart"), "{screen}");
    hosts.run(&["terminal", "stop", "--terminal", id, "--json"]);
}
