# Run `nix develop` (or use direnv) first so every tool is on PATH.

set shell := ["bash", "-euo", "pipefail", "-c"]

# List the recipes.
default:
    @just --list

# Format every source.
fmt:
    cargo fmt --all
    nix fmt flake.nix

# Check formatting without changing files.
fmt-check:
    cargo fmt --all --check

# Lint with every warning as an error.
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Run the tests.
test:
    #!/usr/bin/env bash
    set -euo pipefail
    task_root=$(mktemp -d "${TMPDIR:-/tmp}/adroit-tests.XXXXXX")
    trap 'rm -rf "$task_root"' EXIT
    XDG_STATE_HOME="$task_root/state" XDG_CONFIG_HOME="$task_root/config" XDG_DATA_HOME="$task_root/data" cargo test --workspace

# Everything CI runs.
ci: fmt-check lint test

# Build target/macos/Adroit.app, the window as a macOS application.
[macos]
build-macos:
    cargo build --release -p adroit
    assets/macos/bundle.sh

# Put the app built by `build-macos` in /Applications, replacing one there.
[macos]
[confirm("Replace /Applications/Adroit.app?")]
install-macos: build-macos
    rm -rf /Applications/Adroit.app
    cp -R target/macos/Adroit.app /Applications/
