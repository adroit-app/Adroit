//! Where adroit looks for things on disk.

use std::path::PathBuf;

pub use adroit_model::paths::*;

/// Where scratch sessions run, each in a directory of its own, as the
/// settings say.
pub fn scratch_dir() -> PathBuf {
    crate::settings::scratch_folder(&crate::settings::current(), &home())
}
