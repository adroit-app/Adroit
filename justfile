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
    for task_variable in "${!ADROIT_@}"; do
        unset "$task_variable"
    done
    unset CODEX_THREAD_ID
    task_root=$(mktemp -d "${TMPDIR:-/tmp}/adroit-tests.XXXXXX")
    trap 'rm -rf "$task_root"' EXIT
    XDG_STATE_HOME="$task_root/state" XDG_CONFIG_HOME="$task_root/config" XDG_DATA_HOME="$task_root/data" cargo test --workspace

# Write the references in docs/spec/reference/ from the code.
references:
    cargo run -q -p adroit-core --example references

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
    cp -R "${CARGO_TARGET_DIR:-target}/macos/Adroit.app" /Applications/

# Make the identity the macOS app is signed with, "Adroit Release", into a new folder to keep.
[macos]
macos-make-identity folder:
    assets/macos/make-identity.sh {{folder}}

# Import the identity in a folder `macos-make-identity` made into the login keychain, so `build-macos` signs with it.
[macos]
macos-import-identity folder:
    assets/macos/import-identity.sh {{folder}}
