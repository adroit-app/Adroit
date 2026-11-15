//! Starting an agent session headless, as `adroit session start` or another
//! session asks: what a start that failed leaves behind.

use adroit_vcs::worktree::Worktree;

/// How far a failed start got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    /// Refused before its agent was asked to run.
    Refused,
    /// Its agent was asked to run and did not.
    NotStarted,
    /// Its agent may be running on the work machine all the same.
    MayHaveRun,
}

/// Why a start failed.
#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) stage: Stage,
    pub(crate) message: String,
}

impl Failure {
    pub(crate) fn new(stage: Stage, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
        }
    }
}

/// `launched`, with the worktree made for it taken back unless its agent may
/// have run, so a start that never ran leaves no directory, branch or change
/// behind.
pub(crate) async fn settled<T>(
    worktree: Option<Worktree>,
    launched: Result<T, Failure>,
) -> Result<T, Failure> {
    match launched {
        Err(mut failure) if failure.stage != Stage::MayHaveRun => {
            if let Some(worktree) = worktree
                && let Err(left) = adroit_vcs::worktree::remove(&worktree).await
            {
                failure.message = format!(
                    "{}; the worktree made for it is left at {}: {left}",
                    failure.message,
                    worktree.path.display()
                );
            }
            Err(failure)
        }
        launched => launched,
    }
}

#[cfg(test)]
mod tests;
