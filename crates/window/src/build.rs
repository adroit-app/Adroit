//! The build of adroit this window is, as the program that opened it says:
//! a work host of another build is replaced, and a terminal host's build is
//! told beside it.

use std::sync::OnceLock;

/// What the program says of the build it is.
#[derive(Debug, Clone)]
pub struct Build {
    /// Its version, and what git said of its source.
    pub build: String,
    /// What git said of its source, as a debug build's title shows it.
    pub described: Option<&'static str>,
}

static THIS: OnceLock<Build> = OnceLock::new();

/// Take `build` as this window's, before the window opens.
pub(crate) fn settle(build: Build) {
    let _ = THIS.set(build);
}

/// This window's build; its crate's version until one is settled.
pub(crate) fn build() -> String {
    THIS.get()
        .map(|b| b.build.clone())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").into())
}

/// What git said of this window's source.
pub(crate) fn described() -> Option<&'static str> {
    THIS.get().and_then(|b| b.described)
}
