use std::path::PathBuf;

/// A shell adroit started in a worktree, or in a directory outside git.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
    /// The daemon's id for it.
    pub id: u128,
    pub name: String,
    /// The directory it was started in.
    pub worktree: PathBuf,
    pub runs: Runs,
}

/// What a shell's console runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Runs {
    #[default]
    Shell,
    Neovim,
}
