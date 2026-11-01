//! The startup lease where it cannot be held: the lease replaces itself with
//! the program it holds, which only `exec` does.

use crate::{Error, Result, Spawn};
use std::path::Path;
use std::time::Duration;

pub(crate) fn leased(_: Spawn, _: &Path, _: Duration) -> Result<Spawn> {
    Err(unsupported())
}

pub fn run(_: &[String]) -> i32 {
    eprintln!("adroit: startup lease: {}", unsupported());
    1
}

pub(crate) async fn release(_: &Path) -> Result<()> {
    Err(unsupported())
}

fn unsupported() -> Error {
    Error::Io(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "a startup lease is not supported on this platform",
    ))
}
