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

# Run the tests, in the Neovim `ADROIT_NVIM` names when it is set.
test:
    #!/usr/bin/env bash
    set -euo pipefail
    for task_variable in "${!ADROIT_@}"; do
        # The one the tests are asked to run in, as a Neovim to be shipped is.
        [[ $task_variable == ADROIT_NVIM ]] && continue
        unset "$task_variable"
    done
    unset CODEX_THREAD_ID
    task_root=$(mktemp -d "${TMPDIR:-/tmp}/adroit-tests.XXXXXX")
    trap 'rm -rf "$task_root"' EXIT
    XDG_STATE_HOME="$task_root/state" XDG_CONFIG_HOME="$task_root/config" XDG_DATA_HOME="$task_root/data" cargo test --workspace

# Write the references in docs/spec/reference/ from the code.
references:
    cargo run -q -p adroit-core --example references

# Fail when the release could not stamp its version over the placeholder.
stamp-check:
    tmp=$(mktemp -d) && cp Cargo.toml Cargo.lock "$tmp" && perl assets/release/stamp-version.pl 0.0.0-ci "$tmp"

# Write the licences of the crates Adroit is built from into
# THIRD-PARTY-LICENSES.txt, which the app ships. Run on macOS: cargo-about
# orders crates under one licence differently on Linux.
[macos]
licenses:
    cargo about generate --locked about.hbs -o THIRD-PARTY-LICENSES.txt

# Fail when THIRD-PARTY-LICENSES.txt is not what Cargo.lock now makes, on
# macOS, where the release builds the app that ships it.
[macos]
licenses-check:
    tmp=$(mktemp) && cargo about generate --locked about.hbs -o "$tmp" && diff -q THIRD-PARTY-LICENSES.txt "$tmp" >/dev/null || { echo "THIRD-PARTY-LICENSES.txt is stale; run just licenses" >&2; exit 1; }

# Elsewhere cargo-about writes the same licences in another order, which
# would never match the file kept.
[linux]
licenses-check:

# Everything CI runs, the tests in the Neovim the app carries.
ci: fmt-check lint stamp-check licenses-check
    just test-neovim "$(assets/neovim/fetch.sh)/bin/nvim"
    just test-headless

# What CI runs on Windows, from Git Bash: the format check, lint and the
# tests in the Neovim of the release the app carries. The executable for a
# work machine is left out, as a work machine runs Unix.
[windows]
ci-windows: fmt-check lint stamp-check
    just test-neovim "$(assets/neovim/fetch.sh)/bin/nvim"

# The executable for a work machine, with no desktop runtime linked.
test-headless:
    cargo clippy -p adroit --all-targets --no-default-features -- -D warnings
    cargo test -p adroit --no-default-features --test headless

# Run the tests in the Neovim at `nvim`, as a Neovim release is checked
# before assets/neovim/update.sh pins it for the app to carry.
test-neovim nvim:
    ADROIT_NVIM={{ nvim }} just test

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

# Build Adroit.app for release and put it in the disk image a GitHub Release
# carries, in dist/. Run outside the Nix shell, as the release does: a build
# made inside it refers into /nix/store, which the disk image refuses.
[macos]
bundle version="0.0.0-ci":
    cargo build --release --locked -p adroit
    assets/macos/bundle.sh
    assets/macos/dmg.sh {{version}} dist

# Make a new identity to sign the macOS app with, "Adroit Release", into a
# new folder. The one releases are signed with is kept in 1Password; a new
# one makes every user grant the app its permissions again.
[macos]
macos-make-identity folder:
    assets/macos/make-identity.sh {{folder}}

# Import the identity releases sign with, given as MACOS_SIGNING_P12 and
# MACOS_SIGNING_PASSWORD, into the login keychain, so `build-macos` signs with
# it too.
[macos]
macos-import-identity:
    assets/macos/import-identity.sh
