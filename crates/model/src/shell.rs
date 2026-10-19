use std::path::PathBuf;

/// A shell adroit started in a worktree, or in a directory outside git.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
    /// The daemon's id for it.
    pub id: u128,
    pub name: String,
    /// The directory it was started in.
    pub worktree: PathBuf,
}
