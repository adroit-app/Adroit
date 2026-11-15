use super::*;
use crate::testing::repository;

async fn new_worktree(dir: &std::path::Path) -> Worktree {
    adroit_vcs::worktree::create(&repository(dir), "review", None, "review")
        .await
        .unwrap()
}

#[tokio::test]
async fn a_start_refused_before_its_agent_was_asked_takes_its_new_worktree_back() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = new_worktree(dir.path()).await;
    let path = worktree.path.clone();

    let refused = settled::<()>(
        Some(worktree),
        Err(Failure::new(
            Stage::Refused,
            "the opening session has ended",
        )),
    )
    .await
    .unwrap_err();

    assert_eq!(refused.stage, Stage::Refused);
    assert!(!path.exists());
}

#[tokio::test]
async fn a_start_whose_agent_did_not_start_takes_its_new_worktree_back() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = new_worktree(dir.path()).await;
    let path = worktree.path.clone();

    settled::<()>(
        Some(worktree),
        Err(Failure::new(Stage::NotStarted, "cannot start review")),
    )
    .await
    .unwrap_err();

    assert!(!path.exists());
}

#[tokio::test]
async fn a_start_that_may_have_run_keeps_its_new_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = new_worktree(dir.path()).await;
    let path = worktree.path.clone();

    let failed = settled::<()>(
        Some(worktree),
        Err(Failure::new(
            Stage::MayHaveRun,
            "cannot start review: the work machine did not answer",
        )),
    )
    .await
    .unwrap_err();

    assert_eq!(failed.stage, Stage::MayHaveRun);
    assert!(path.is_dir());
}

#[tokio::test]
async fn a_started_session_keeps_its_new_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = new_worktree(dir.path()).await;
    let path = worktree.path.clone();

    settled(Some(worktree), Ok("review")).await.unwrap();

    assert!(path.is_dir());
}
