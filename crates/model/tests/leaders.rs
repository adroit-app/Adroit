use std::collections::HashMap;

use adroit_model::{
    Agent, AttemptId, Leaders, LogicalSessionId, ProjectionError, RunId, Session, SessionBinding,
    State, TaskId, UnassignedReason, Worktree, build,
};

fn leader(name: &str, cwd: &str) -> Session {
    Session {
        id: format!("terminal-{name}"),
        session_id: format!("conversation-{name}"),
        name: name.into(),
        cwd: cwd.into(),
        binding: Some(SessionBinding::leader(
            LogicalSessionId::new(),
            Some(RunId::new()),
        )),
        ..Session::default()
    }
}

fn worker(name: &str, cwd: &str, leader: &Session) -> Session {
    let owner = leader.binding.as_ref().unwrap();
    Session {
        id: format!("terminal-{name}"),
        name: name.into(),
        cwd: cwd.into(),
        binding: Some(
            SessionBinding::worker(
                LogicalSessionId::new(),
                owner.logical_id(),
                owner.run_id().unwrap(),
                TaskId::new(),
                AttemptId::new(),
            )
            .unwrap(),
        ),
        ..Session::default()
    }
}

#[test]
fn a_worker_at_the_repository_root_keeps_its_assigned_leader() {
    let leader = leader("plan", "/repo");
    let worker = worker("implement", "/repo", &leader);
    let tree = build(&[worker, leader], &[], &HashMap::new(), &[], None);
    let view = Leaders::from_tree(&tree).unwrap();

    assert_eq!(view.leaders.len(), 1);
    assert_eq!(view.leaders[0].session.name, "plan");
    assert_eq!(view.leaders[0].workers[0].name, "implement");
    assert!(view.unassigned.is_empty());
}

#[test]
fn two_leaders_in_one_repository_keep_their_workers_apart() {
    let first = leader("first", "/repo");
    let second = leader("second", "/repo");
    let a = worker("a", "/repo", &first);
    let b = worker("b", "/repo", &second);
    let tree = build(&[first, second, a, b], &[], &HashMap::new(), &[], None);
    let view = Leaders::from_tree(&tree).unwrap();

    assert_eq!(view.leaders.len(), 2);
    for group in &view.leaders {
        assert_eq!(group.workers.len(), 1);
        let worker = group.workers[0].binding.as_ref().unwrap();
        let leader = group.session.binding.as_ref().unwrap();
        assert_eq!(worker.leader_id(), leader.logical_id());
        assert_eq!(worker.run_id(), leader.run_id());
    }
}

#[test]
fn workspace_and_leader_views_refer_to_the_same_session() {
    let leader = leader("plan", "/repo");
    let worker = worker("implement", "/worktrees/task", &leader);
    let found = [
        (leader.cwd.clone(), Some(Worktree::plain("/repo"))),
        (
            worker.cwd.clone(),
            Some(Worktree {
                path: worker.cwd.clone(),
                repo: "/repo".into(),
                branch: Some("task".into()),
            }),
        ),
    ]
    .into_iter()
    .collect();
    let tree = build(&[leader, worker], &[], &found, &[], None);
    let view = Leaders::from_tree(&tree).unwrap();
    let group = &view.leaders[0];
    let (_, _, by_place) = tree.find(&group.workers[0].id).unwrap();

    assert!(std::ptr::eq(group.workers[0], by_place));
    assert_eq!(tree.workspaces.len(), 1);
    assert_eq!(tree.workspaces[0].worktrees.len(), 2);
}

#[test]
fn renaming_a_worker_does_not_change_its_owner_or_runtime_ids() {
    let leader = leader("plan", "/repo");
    let mut worker = worker("implement", "/repo", &leader);
    worker.session_id = "provider-conversation".into();
    let binding = worker.binding.clone();
    worker.name = "renamed".into();
    let tree = build(&[leader, worker], &[], &HashMap::new(), &[], None);
    let view = Leaders::from_tree(&tree).unwrap();
    let shown = view.leaders[0].workers[0];

    assert_eq!(shown.name, "renamed");
    assert_eq!(shown.id, "terminal-implement");
    assert_eq!(shown.session_id, "provider-conversation");
    assert_eq!(shown.binding, binding);
}

#[test]
fn an_unknown_session_is_not_assigned_by_its_directory_or_parent_conversation() {
    let leader = leader("plan", "/repo");
    let unbound = Session {
        id: "unbound".into(),
        cwd: leader.cwd.clone(),
        started_from: leader.session_id.clone(),
        ..Session::default()
    };
    let tree = build(&[leader, unbound], &[], &HashMap::new(), &[], None);
    let view = Leaders::from_tree(&tree).unwrap();

    assert!(view.leaders[0].workers.is_empty());
    assert_eq!(view.unassigned.len(), 1);
    assert_eq!(view.unassigned[0].session.id, "unbound");
    assert_eq!(view.unassigned[0].reason, UnassignedReason::Unbound);
}

#[test]
fn a_missing_leader_does_not_hide_its_worker_or_assign_another_owner() {
    let missing = leader("missing", "/repo");
    let other = leader("other", "/repo");
    let worker = worker("implement", "/repo", &missing);
    let tree = build(&[other, worker], &[], &HashMap::new(), &[], None);
    let view = Leaders::from_tree(&tree).unwrap();

    assert!(view.leaders[0].workers.is_empty());
    assert_eq!(view.unassigned[0].reason, UnassignedReason::MissingLeader);
}

#[test]
fn a_worker_from_another_run_is_not_attached_to_a_reused_leader() {
    let mut leader = leader("plan", "/repo");
    let worker = worker("implement", "/repo", &leader);
    let id = leader.binding.as_ref().unwrap().logical_id();
    leader.binding = Some(SessionBinding::leader(id, Some(RunId::new())));
    let tree = build(&[leader, worker], &[], &HashMap::new(), &[], None);
    let view = Leaders::from_tree(&tree).unwrap();

    assert!(view.leaders[0].workers.is_empty());
    assert_eq!(view.unassigned[0].reason, UnassignedReason::RunMismatch);
}

#[test]
fn a_finished_attempt_keeps_its_identity_and_reported_session_state() {
    let leader = leader("plan", "/plain-directory");
    let mut old = worker("old", "/plain-directory", &leader);
    old.state = State::Done;
    let old_work = old.binding.as_ref().unwrap().work().unwrap();
    let mut retry = worker("retry", "/plain-directory", &leader);
    let binding = retry.binding.as_ref().unwrap();
    retry.binding = Some(
        SessionBinding::worker(
            binding.logical_id(),
            binding.leader_id(),
            binding.run_id().unwrap(),
            old_work.0,
            AttemptId::new(),
        )
        .unwrap(),
    );
    let tree = build(&[leader, old, retry], &[], &HashMap::new(), &[], None);
    let view = Leaders::from_tree(&tree).unwrap();
    let workers = &view.leaders[0].workers;

    assert_eq!(workers.len(), 2);
    let old = workers.iter().find(|s| s.name == "old").unwrap();
    let retry = workers.iter().find(|s| s.name == "retry").unwrap();
    assert_eq!(old.state, State::Done);
    assert_eq!(old.binding.as_ref().unwrap().work().unwrap().0, old_work.0);
    assert_ne!(
        retry.binding.as_ref().unwrap().work().unwrap().1,
        old_work.1
    );
}

#[test]
fn duplicate_logical_ids_are_rejected_instead_of_combining_sessions() {
    let first = leader("first", "/repo");
    let mut second = leader("second", "/repo");
    second.binding = first.binding.clone();
    let id = first.binding.as_ref().unwrap().logical_id();
    let tree = build(&[first, second], &[], &HashMap::new(), &[], None);

    assert_eq!(
        Leaders::from_tree(&tree).unwrap_err(),
        ProjectionError::DuplicateLogicalId(id)
    );
}

#[test]
fn one_run_cannot_have_two_leaders() {
    let first = leader("first", "/repo");
    let mut second = leader("second", "/repo");
    let run = first.binding.as_ref().unwrap().run_id().unwrap();
    second.binding = Some(SessionBinding::leader(
        second.binding.as_ref().unwrap().logical_id(),
        Some(run),
    ));
    let tree = build(&[first, second], &[], &HashMap::new(), &[], None);

    assert_eq!(
        Leaders::from_tree(&tree).unwrap_err(),
        ProjectionError::DuplicateRunId(run)
    );
}

#[test]
fn changing_a_workers_provider_and_permissions_does_not_change_its_owner() {
    let leader = leader("plan", "/repo");
    let mut worker = worker("review", "/elsewhere", &leader);
    worker.agent = Agent::Codex;
    worker.permissions = adroit_model::AgentPermissions::ReadOnly;
    let tree = build(&[leader, worker], &[], &HashMap::new(), &[], None);
    let view = Leaders::from_tree(&tree).unwrap();

    assert_eq!(view.leaders[0].session.agent, Agent::Claude);
    assert_eq!(view.leaders[0].workers[0].agent, Agent::Codex);
    assert!(view.leaders[0].workers[0].reads_only());
    assert!(view.unassigned.is_empty());
}
