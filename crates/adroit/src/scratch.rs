//! Scratch sessions: each runs in a directory of its own under the scratch
//! root, made when the session starts and named by the local time it did.

use std::io;
use std::path::{Path, PathBuf};

/// A local date and time, to the minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moment {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

impl Moment {
    /// Now, in the local time zone.
    pub fn now() -> Moment {
        // SAFETY: `time` and `localtime_r` only write to the values passed.
        let tm = unsafe {
            let t = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            libc::localtime_r(&t, &mut tm);
            tm
        };
        Moment {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u32,
            day: tm.tm_mday as u32,
            hour: tm.tm_hour as u32,
            minute: tm.tm_min as u32,
        }
    }

    /// The name a scratch directory made at this moment is given.
    pub fn stamp(&self) -> String {
        format!(
            "{:04}{:02}{:02}-{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute
        )
    }
}

/// The names to try for a directory stamped `stamp`, in order: the stamp,
/// then the stamp with `-2`, `-3`, ….
fn names(stamp: &str) -> impl Iterator<Item = String> + '_ {
    std::iter::once(stamp.to_string()).chain((2..).map(move |n| format!("{stamp}-{n}")))
}

/// Make a new directory under `root`, making `root` too, named by `stamp` or,
/// when that is taken, by the first of its numbered names that is free.
pub fn make(root: &Path, stamp: &str) -> io::Result<PathBuf> {
    std::fs::create_dir_all(root)?;
    for name in names(stamp) {
        let dir = root.join(name);
        // Made, not looked for first: two sessions started in the same minute
        // would otherwise both take the name.
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!("the names never run out")
}

/// Where a session started from a draft in `worktree` runs: a new directory
/// for a draft in the scratch root itself, otherwise the draft's own.
pub fn start_dir(worktree: &Path, root: &Path, at: Moment) -> io::Result<PathBuf> {
    if worktree == root {
        make(root, &at.stamp())
    } else {
        Ok(worktree.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: Moment = Moment {
        year: 2026,
        month: 3,
        day: 9,
        hour: 7,
        minute: 5,
    };

    #[test]
    fn a_scratch_directory_is_named_by_the_local_date_and_time_to_the_minute() {
        assert_eq!(AT.stamp(), "20260309-0705");
    }

    #[test]
    fn a_new_directory_is_made_under_the_root_made_with_it() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("adroit").join("scratch");
        let dir = make(&root, "20260309-0705").unwrap();
        assert_eq!(dir, root.join("20260309-0705"));
        assert!(dir.is_dir());
    }

    #[test]
    fn a_taken_name_gets_the_next_free_number() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join("20260309-0705")).unwrap();
        std::fs::create_dir(root.join("20260309-0705-2")).unwrap();
        let dir = make(root, "20260309-0705").unwrap();
        assert_eq!(dir, root.join("20260309-0705-3"));
    }

    #[test]
    fn a_draft_in_the_scratch_root_starts_in_a_new_directory_of_its_own() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("scratch");
        let first = start_dir(&root, &root, AT).unwrap();
        let second = start_dir(&root, &root, AT).unwrap();
        assert_eq!(first, root.join("20260309-0705"));
        assert_eq!(second, root.join("20260309-0705-2"));
    }

    #[test]
    fn a_draft_anywhere_else_starts_where_it_is_and_makes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("scratch");
        let inside = root.join("20260309-0705");
        assert_eq!(start_dir(&inside, &root, AT).unwrap(), inside);
        assert_eq!(
            start_dir(Path::new("/w/api"), &root, AT).unwrap(),
            PathBuf::from("/w/api")
        );
        assert!(!root.exists());
    }
}
