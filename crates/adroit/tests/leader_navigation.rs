use adroit::leader_navigation::{RowRole, project};
use adroit::nav::Item;
use adroit_model::*;
use std::collections::HashMap;

fn fixture() -> (Tree, ControlSnapshot) {
    let mut snapshot = ControlSnapshot::default();
    let mut sessions = Vec::new();
    for index in 0..2 {
        let run = RunId::new();
        let leader = LogicalSessionId::new();
        let binding = SessionBinding::leader(leader, Some(run));
        let native = Session {
            id: format!("leader-{index}"),
            binding: Some(binding.clone()),
            agent: Agent::Codex,
            name: format!("Leader {index}"),
            cwd: "/repo".into(),
            ..Default::default()
        };
        snapshot.sessions.push(ManagedSession {
            binding,
            agent: Agent::Codex,
            generation: 1,
            state: ManagedSessionState::Running,
            permissions: AgentPermissions::Default,
            cwd: Some(native.cwd.clone()),
            terminal_id: Some(native.id.clone()),
            conversation_id: None,
        });
        snapshot.runs.push(RunRecord {
            id: run,
            leader_id: leader,
            brief: RunBrief {
                title: format!("Request {index}"),
                goal: "Independent request".into(),
                ..Default::default()
            },
            plan_revision: 1,
            candidate_id: None,
            state: RunState::Active,
        });
        sessions.push(native);
        let binding = SessionBinding::worker(
            LogicalSessionId::new(),
            leader,
            run,
            TaskId::new(),
            AttemptId::new(),
        )
        .unwrap();
        let native = Session {
            id: format!("worker-{index}"),
            binding: Some(binding.clone()),
            agent: Agent::Claude,
            name: format!("Worker {index}"),
            cwd: format!("/worktrees/{index}").into(),
            ..Default::default()
        };
        snapshot.sessions.push(ManagedSession {
            binding,
            agent: Agent::Claude,
            generation: 1,
            state: ManagedSessionState::Running,
            permissions: AgentPermissions::Default,
            cwd: Some(native.cwd.clone()),
            terminal_id: Some(native.id.clone()),
            conversation_id: None,
        });
        sessions.push(native);
    }
    (build(&sessions, &[], &HashMap::new(), &[], None), snapshot)
}

#[test]
fn independent_leaders_at_the_same_root_group_workers_by_current_ownership() {
    let (tree, snapshot) = fixture();
    let rows = project(&tree, &snapshot);
    assert_eq!(
        rows.iter().map(|row| row.item.clone()).collect::<Vec<_>>(),
        vec![
            Item::Session("leader-0".into()),
            Item::Session("worker-0".into()),
            Item::Session("leader-1".into()),
            Item::Session("worker-1".into())
        ]
    );
    assert_eq!(rows[0].role, RowRole::Leader);
    assert_eq!(rows[1].role, RowRole::Worker);
    assert_eq!(rows[0].run_id, rows[1].run_id);
    assert_ne!(rows[0].run_id, rows[2].run_id);
    assert_eq!(tree.sessions().count(), 4);
}

#[test]
fn an_old_native_terminal_cannot_claim_a_new_generation_or_duplicate_its_leader() {
    let (mut tree, mut snapshot) = fixture();
    let mut old = tree
        .sessions()
        .find(|(_, session)| session.id == "leader-0")
        .unwrap()
        .1
        .clone();
    old.id = "old-terminal".into();
    tree.workspaces[0].worktrees[0].sessions.push(old);
    snapshot.sessions[0].generation = 2;
    let rows = project(&tree, &snapshot);
    assert_eq!(
        rows.iter()
            .filter(|row| row.role == RowRole::Leader)
            .count(),
        2
    );
    let old = rows
        .iter()
        .find(|row| row.item == Item::Session("old-terminal".into()))
        .unwrap();
    assert_eq!(old.role, RowRole::Unassigned);
    assert!(old.run_id.is_none());
    assert!(old.reason.as_deref().unwrap().contains("current"));
}

#[test]
fn missing_controller_ownership_leaves_every_native_session_accessible_as_unassigned() {
    let (tree, _) = fixture();
    let rows = project(&tree, &ControlSnapshot::default());
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|row| row.role == RowRole::Unassigned));
}

#[test]
fn shell_and_neovim_are_terminal_rows_in_their_request_and_never_workers() {
    let (mut tree, snapshot) = fixture();
    let run = snapshot.runs[0].id;
    for (id, program) in [(41, TerminalProgram::Shell), (42, TerminalProgram::Neovim)] {
        tree.workspaces[0].worktrees[0]
            .terminals
            .push(TerminalSession {
                id,
                program,
                name: format!("Tool {id}"),
                cwd: "/repo".into(),
                binding: Some(TerminalBinding {
                    run_id: run,
                    worker_id: None,
                }),
                ..Default::default()
            });
    }
    let rows = project(&tree, &snapshot);
    assert_eq!(
        rows.iter()
            .filter(|row| row.role == RowRole::Worker)
            .count(),
        2
    );
    let tools: Vec<_> = rows
        .iter()
        .filter(|row| row.role == RowRole::Terminal)
        .collect();
    assert_eq!(tools.len(), 2);
    assert!(tools.iter().all(|row| row.run_id == Some(run)));
}

#[test]
fn a_terminal_uses_the_selected_agent_context_without_guessing_from_its_directory() {
    use adroit::leader_navigation::terminal_binding;
    let (tree, snapshot) = fixture();
    let leader = terminal_binding(&tree, &snapshot, &Item::Session("leader-1".into())).unwrap();
    assert_eq!(leader.run_id, snapshot.runs[1].id);
    assert!(leader.worker_id.is_none());
    let worker = terminal_binding(&tree, &snapshot, &Item::Session("worker-0".into())).unwrap();
    assert_eq!(worker.run_id, snapshot.runs[0].id);
    assert_eq!(
        worker.worker_id,
        Some(snapshot.sessions[1].binding.logical_id())
    );
    assert!(terminal_binding(&tree, &snapshot, &Item::Workspace("/repo".into())).is_none());
    assert!(
        terminal_binding(
            &tree,
            &ControlSnapshot::default(),
            &Item::Session("leader-1".into())
        )
        .is_none()
    );
}
