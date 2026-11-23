//! Where adroit looks for things on disk.

use std::path::PathBuf;

pub use adroit_model::paths::*;

/// Where scratch sessions run, as the
/// settings say.
pub fn scratch_dir() -> PathBuf {
    crate::settings::scratch_folder(&crate::settings::current(), &home())
}

/// Work files belong to the selected environment; service sockets stay shared.
pub fn config_dir() -> PathBuf {
    crate::environment::current()
        .map_or_else(adroit_model::paths::config_dir, |c| c.root.join("config"))
}

pub fn instance_state() -> PathBuf {
    crate::environment::current().map_or_else(adroit_model::paths::instance_state, |c| {
        c.root.join("state")
    })
}

pub fn data_dir() -> PathBuf {
    crate::environment::current()
        .map_or_else(adroit_model::paths::data_dir, |c| c.root.join("data"))
}

pub fn keys_path() -> PathBuf {
    if crate::environment::current().is_some() {
        config_dir().join("keys.toml")
    } else {
        adroit_model::paths::keys_path()
    }
}

/// Where throwaway work runs.
pub fn scratch() -> adroit_model::Scratch {
    adroit_model::Scratch {
        root: scratch_dir(),
        temporary: temporary_dirs(),
    }
}

/// The temporary directories, by the paths they are reached by and those
/// they resolve to: a directory in one outside git is throwaway work that
/// Scratch holds.
fn temporary_dirs() -> Vec<PathBuf> {
    let mut named = vec![std::env::temp_dir()];
    if cfg!(unix) {
        named.push(PathBuf::from("/tmp"));
    }
    let mut dirs: Vec<PathBuf> = named
        .into_iter()
        .flat_map(|dir| {
            let resolved = dir.canonicalize().ok();
            std::iter::once(dir).chain(resolved)
        })
        .collect();
    dirs.sort();
    dirs.dedup();
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_temporary_directories_hold_the_process_s_own_both_as_named_and_resolved() {
        let dirs = temporary_dirs();
        let own = std::env::temp_dir();
        assert!(dirs.contains(&own));
        assert!(dirs.contains(&own.canonicalize().unwrap()));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn on_macos_the_temporary_directories_hold_tmp_and_where_it_leads() {
        let dirs = temporary_dirs();
        assert!(dirs.contains(&PathBuf::from("/tmp")));
        assert!(dirs.contains(&PathBuf::from("/private/tmp")));
    }
}
