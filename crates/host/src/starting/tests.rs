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

/// An agent session the terminal host runs in terminal `id`, called `name`.
fn agent(id: u128, name: &str) -> adroit_term::Session {
    use adroit_term::tags::tagged;
    adroit_term::Session {
        id,
        tags: [
            tagged(crate::sessions::KIND, "claude"),
            tagged(crate::sessions::CWD, "/repo"),
            tagged(crate::sessions::NAME, name),
        ]
        .concat(),
        title: None,
        exited: false,
        status: None,
        sequence: 1,
    }
}

#[test]
fn a_session_is_named_from_its_prompt_unless_given_one() {
    assert_eq!(
        name_for(None, "Find why the login test is flaky", &[]),
        "find-login-test"
    );
    assert_eq!(name_for(Some("Review API"), "anything", &[]), "review-api");
}

#[test]
fn a_name_another_agent_session_goes_by_gets_a_number() {
    assert_eq!(
        name_for(Some("review"), "anything", &[agent(1, "review")]),
        "review-2"
    );
}

#[test]
fn a_session_keeps_the_name_it_already_goes_by() {
    let listed = [agent(1, "review"), agent(2, "review-2")];

    assert_eq!(fresh_name("review", &listed, Some(1)), "review");
    assert_eq!(fresh_name("review", &listed, Some(3)), "review-3");
}
