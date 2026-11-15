use std::path::PathBuf;

use adroit_model::EnvironmentId;

/// Where the middle of the window is: the worktree, or the directory outside
/// git, its tabs are of, in the environment it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub environment: EnvironmentId,
    pub worktree: PathBuf,
}

impl Place {
    pub fn at(environment: &EnvironmentId, worktree: impl Into<PathBuf>) -> Place {
        Place {
            environment: environment.clone(),
            worktree: worktree.into(),
        }
    }
}
