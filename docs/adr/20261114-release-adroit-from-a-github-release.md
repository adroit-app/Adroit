# Release Adroit from a GitHub Release

Status: Accepted

## Context

Adroit could only be had by building it from source, and the version in
`Cargo.toml` stayed the placeholder `0.0.0`. Building the macOS app needs the
Nix development shell or a Rust toolchain, the Neovim the app carries, and
the signing identity in the keychain, which a user who only wants to run
Adroit has none of.

## Decision

Publishing a GitHub Release is what makes a release, and its tag is the only
record of the version. Publishing it builds Adroit.app on macOS, stamps the
tag's version over the placeholder, signs the app with the identity every
build is signed with
([20261023](20261023-sign-the-macos-app-with-one-self-signed-certificate.md)),
and attaches it in a disk image. The Homebrew cask in `adroit-app/homebrew-tap`
then follows the release, and the flake fetched at the tag is built into the
`adroit` Cachix cache. CI runs the same packaging with a placeholder version
on every push, so a release is never its first run.

The identity is kept in 1Password and in the secrets of the repository's
environment "release", which only the release's packaging job reads.

## Alternatives considered

- Keep the version in `Cargo.toml` and bump it in a commit. A release would
  then need a commit and a tag that must agree, and the flake would build the
  bumped number from any commit after it.
- Packages for Linux and Windows too. Adroit runs there from source, but no
  user has asked for a package yet, and each would need its own way to carry
  Neovim.
- An Apple Developer ID and notarization, which would let Gatekeeper open the
  disk image. The project holds no paid membership; the cask removes the
  quarantine instead.

## Consequences

- `brew install --cask adroit-app/tap/adroit` installs the latest release.
- A release's assets appear a few minutes after its notes.
- A downloaded disk image is refused by Gatekeeper until its quarantine is
  removed or opening it is allowed in System Settings.
- The packages ship `THIRD-PARTY-LICENSES.txt`, which `just ci` keeps in step
  with `Cargo.lock`.
