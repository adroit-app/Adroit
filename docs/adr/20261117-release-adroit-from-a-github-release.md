# Release Adroit from a GitHub Release

Status: Accepted

## Context

Adroit could only be had by building it from source, and the version in
`Cargo.toml` stayed the placeholder `0.0.0`. Building it needs the Nix
development shell or a Rust toolchain with the libraries the window links,
the Neovim its prompt and console reader are tested in, and, for the macOS
app, the signing identity in the keychain, none of which a user who only
wants to run Adroit has.

## Decision

Publishing a GitHub Release is what makes a release, and its tag is the only
record of the version. Publishing it builds Adroit on macOS, Windows and
Linux, stamps the tag's version over the placeholder, and attaches what each
system runs: Adroit.app in a disk image, signed with the identity every macOS
build is signed with
([20261023](20261023-sign-the-macos-app-with-one-self-signed-certificate.md)),
an unsigned zip for Windows, and a tarball for Linux. Each carries the Neovim
the tests run in, beside the executable. The Homebrew cask in
`adroit-app/homebrew-tap` then follows the release, and the flake fetched at
the tag is built into the `adroit` Cachix cache. CI runs the same packaging
with a placeholder version on every push, so a release is never its first
run.

The identity is kept in 1Password and in the secrets of the repository's
environment "release", which only the macOS packaging job reads.

## Alternatives considered

- Keep the version in `Cargo.toml` and bump it in a commit. A release would
  then need a commit and a tag that must agree, and the flake would build the
  bumped number from any commit after it.
- Leave Windows and Linux to building from source and to Nix. Neither suits
  a user who only wants to run Adroit, and Nix does not run on Windows.
- Windows and Linux archives that run the `nvim` on `PATH`. They would be
  smaller, but the prompt and the reader would run whichever version the
  user has, which the tests never ran in.
- An installer or a distribution's package (MSI, `.deb`, Flatpak) rather than
  an archive. Each needs its own tooling and signing; an archive unpacked
  anywhere runs as it is.
- An Apple Developer ID and notarization, which would let Gatekeeper open the
  disk image, or a code-signing certificate for Windows, which would quiet
  SmartScreen. The project holds neither; the cask removes the quarantine
  instead.

## Consequences

- `brew install --cask adroit-app/tap/adroit` installs the latest release.
- A release's assets appear a few minutes after its notes.
- A downloaded disk image is refused by Gatekeeper until its quarantine is
  removed or opening it is allowed in System Settings, and Windows warns
  before it runs a downloaded `adroit.exe`.
- The Linux tarball runs only where the system provides WebKitGTK and a glibc
  at least as new as the one it was built against.
- The packages ship `THIRD-PARTY-LICENSES.txt`, which `just ci` keeps in step
  with `Cargo.lock` for every system a release builds for.
