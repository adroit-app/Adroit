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

fn worker_fixture() -> (Session, ControlSnapshot) {
    let (tree, mut snapshot) = fixture();
    let mut session = tree.find("worker-0").unwrap().2.clone();
    session.name = format!("worker-{}", LogicalSessionId::new());
    let binding = session.binding.as_ref().unwrap();
    let (task_id, attempt_id) = binding.work().unwrap();
    snapshot.tasks.push(TaskRecord {
        id: task_id,
        run_id: binding.run_id().unwrap(),
        revision: 1,
        brief: TaskBrief {
            instructions: "\n \t\n  読みやすい\u{3000}Worker\t names  \n\nKeep full details.\n"
                .into(),
            purpose: TaskPurpose::Implement,
            ..Default::default()
        },
        dependencies: vec![],
        state: TaskState::Running,
        current_attempt: Some(attempt_id),
    });
    snapshot.attempts.push(AttemptRecord {
        id: attempt_id,
        task_id,
        task_revision: 1,
        input_versions: vec![],
        session_id: binding.logical_id(),
        target: TargetSelection::default(),
        actual_model: None,
        candidate_input: None,
        work_area_id: None,
        base_revision: None,
        state: AttemptState::Running,
    });
    (session, snapshot)
}

#[test]
fn worker_labels_normalize_the_first_nonempty_line_and_keep_full_instructions() {
    use adroit::leader_navigation::worker_presentation;
    let (session, snapshot) = worker_fixture();
    let before = session.clone();
    let display = worker_presentation(&session, &snapshot).unwrap();
    assert_eq!(display.label, "読みやすい Worker names");
    assert_eq!(display.metadata, "Worker 1");
    assert_eq!(
        display.instructions.as_deref(),
        Some(snapshot.tasks[0].brief.instructions.as_str())
    );
    assert_eq!(session, before);
}

#[test]
fn worker_labels_fail_closed_for_missing_or_mismatched_ownership_and_revision() {
    use adroit::leader_navigation::worker_presentation;
    let (session, snapshot) = worker_fixture();
    for mutation in 0..13 {
        let mut snapshot = snapshot.clone();
        match mutation {
            0 => snapshot.tasks.clear(),
            1 => snapshot.attempts.clear(),
            2 => snapshot.sessions.clear(),
            3 => snapshot.runs.clear(),
            4 => snapshot.tasks[0].revision += 1,
            5 => snapshot.tasks[0].run_id = RunId::new(),
            6 => snapshot.attempts[0].task_id = TaskId::new(),
            7 => snapshot.attempts[0].session_id = LogicalSessionId::new(),
            8 => snapshot.sessions[1].terminal_id = Some("another terminal".into()),
            9 => snapshot.runs[0].leader_id = LogicalSessionId::new(),
            10 => snapshot.sessions[1].agent = Agent::Codex,
            11 => snapshot.attempts[0].id = AttemptId::new(),
            12 => snapshot.sessions[1].binding = snapshot.sessions[3].binding.clone(),
            _ => unreachable!(),
        }
        let display = worker_presentation(&session, &snapshot).unwrap();
        assert_eq!(display.label, "Worker", "mutation {mutation}");
        assert!(display.instructions.is_none(), "mutation {mutation}");
    }
}

#[test]
fn worker_fallbacks_use_purpose_then_readable_native_names_without_machine_ids() {
    use adroit::leader_navigation::worker_presentation;
    let (mut session, mut snapshot) = worker_fixture();
    snapshot.tasks[0].brief.instructions = " \n\t\u{3000}".into();
    assert_eq!(
        worker_presentation(&session, &snapshot).unwrap().label,
        "Implementation"
    );
    snapshot.tasks.clear();
    session.name = "  Investigate\t regression  ".into();
    assert_eq!(
        worker_presentation(&session, &snapshot).unwrap().label,
        "Investigate regression"
    );
    for name in [
        LogicalSessionId::new().to_string(),
        format!("worker-{}", LogicalSessionId::new()),
        String::new(),
    ] {
        session.name = name;
        assert_eq!(
            worker_presentation(&session, &snapshot).unwrap().label,
            "Worker"
        );
    }
    session.binding = None;
    assert!(worker_presentation(&session, &snapshot).is_none());
}

#[test]
fn worker_numbers_include_hidden_completed_workers_and_attempt_history() {
    use adroit::leader_navigation::worker_presentation;
    let (session, mut snapshot) = worker_fixture();
    let mut hidden = snapshot.sessions[1].clone();
    let binding = hidden.binding.clone();
    let (task, _) = binding.work().unwrap();
    hidden.binding = SessionBinding::worker(
        LogicalSessionId::new(),
        binding.leader_id(),
        binding.run_id().unwrap(),
        task,
        AttemptId::new(),
    )
    .unwrap();
    hidden.state = ManagedSessionState::Exited;
    hidden.terminal_id = None;
    snapshot.sessions.insert(0, hidden.clone());
    let mut attempt = snapshot.attempts[0].clone();
    attempt.id = hidden.binding.work().unwrap().1;
    attempt.session_id = hidden.binding.logical_id();
    attempt.state = AttemptState::Failed;
    snapshot.attempts.insert(0, attempt);
    assert_eq!(
        worker_presentation(&session, &snapshot).unwrap().metadata,
        "Worker 2 · Attempt 2"
    );
    // A visible-tree filter or unrelated run does not determine this number.
    snapshot
        .sessions
        .retain(|record| record.binding.run_id() == binding.run_id());
    assert_eq!(
        worker_presentation(&session, &snapshot).unwrap().metadata,
        "Worker 2 · Attempt 2"
    );
}

#[test]
fn adding_later_workers_and_attempts_keeps_existing_numbers_and_labels() {
    use adroit::leader_navigation::worker_presentation;
    let (session, mut snapshot) = worker_fixture();
    let mut later = snapshot.sessions[1].clone();
    let binding = later.binding.clone();
    let (task, _) = binding.work().unwrap();
    later.binding = SessionBinding::worker(
        LogicalSessionId::new(),
        binding.leader_id(),
        binding.run_id().unwrap(),
        task,
        AttemptId::new(),
    )
    .unwrap();
    let mut attempt = snapshot.attempts[0].clone();
    attempt.id = later.binding.work().unwrap().1;
    attempt.session_id = later.binding.logical_id();
    snapshot.sessions.push(later);
    snapshot.attempts.push(attempt);
    snapshot.tasks[0].current_attempt = Some(snapshot.attempts[1].id);
    let display = worker_presentation(&session, &snapshot).unwrap();
    assert_eq!(display.metadata, "Worker 1 · Attempt 1");
    assert_eq!(display.label, "読みやすい Worker names");
}

fn assert_worker_filter(session: &Session, snapshot: &ControlSnapshot, query: &str, matches: bool) {
    use adroit::leader_navigation::narrowed;
    use adroit::nav::{self, Place};
    use adroit::tabs::Tab;
    use std::collections::HashSet;

    let (mut tree, _) = fixture();
    *tree
        .workspaces
        .iter_mut()
        .flat_map(|ws| &mut ws.worktrees)
        .flat_map(|wt| &mut wt.sessions)
        .find(|native| native.id == session.id)
        .unwrap() = session.clone();
    let before = tree.clone();
    let shown = narrowed(&tree, snapshot, query);
    let selected = Item::Session(session.id.clone());
    let workspace_rows = nav::items(&shown, &inbox(&shown), &HashSet::new());
    // Leaders mode projects the original tree, then tests native identities
    // against this same narrowed tree (and retains matching Workers' Leaders).
    let leader_rows: Vec<_> = project(&tree, snapshot)
        .into_iter()
        .filter(|row| match &row.item {
            Item::Session(id) => shown.find(id).is_some(),
            Item::Terminal(id) => shown.terminal(*id).is_some(),
            _ => false,
        })
        .map(|row| row.item)
        .collect();
    for rows in [workspace_rows, leader_rows] {
        assert_eq!(rows.contains(&selected), matches, "query {query:?}");
        if matches {
            assert_eq!(shown.find(&session.id).unwrap().2, session);
            assert_eq!(
                nav::row_kept(
                    &rows,
                    &tree,
                    Some(&selected),
                    Some(&Tab::Session(session.id.clone())),
                    &Place {
                        worktree: session.cwd.clone()
                    },
                ),
                None,
                "matching the visible label keeps the native selection"
            );
        }
    }
    assert_eq!(tree, before);
}

#[test]
fn sidebar_filter_matches_visible_worker_labels_in_both_modes_without_renaming() {
    let (session, mut snapshot) = worker_fixture();
    snapshot.tasks[0].brief.instructions = "\n Fix\t scheduler \nPrivate detail".into();
    for query in ["scheduler", "FIX SCHEDULER", session.name.as_str(), ""] {
        assert_worker_filter(&session, &snapshot, query, true);
    }
    assert_worker_filter(&session, &snapshot, "Private detail", false);
    assert_worker_filter(&session, &snapshot, "unrelated", false);
}

#[test]
fn sidebar_filter_matches_unicode_and_the_actual_readable_worker_fallbacks() {
    let (mut session, mut snapshot) = worker_fixture();
    assert_worker_filter(&session, &snapshot, "読みやすい worker", true);
    snapshot.tasks[0].brief.instructions = " \n\t".into();
    assert_worker_filter(&session, &snapshot, "implementation", true);
    snapshot.tasks.clear();
    session.name = "  Déboguer\t 計画  ".into();
    assert_worker_filter(&session, &snapshot, "déboguer 計画", true);
    session.name = format!("worker-{}", LogicalSessionId::new());
    assert_worker_filter(&session, &snapshot, "worker", true);
}

#[test]
fn sidebar_filter_rejects_task_text_without_current_ownership_and_revision() {
    let (session, snapshot) = worker_fixture();
    for mutation in 0..4 {
        let mut snapshot = snapshot.clone();
        match mutation {
            0 => snapshot.tasks[0].revision += 1,
            1 => snapshot.sessions[1].terminal_id = Some("old-terminal".into()),
            2 => snapshot.runs[0].leader_id = LogicalSessionId::new(),
            3 => snapshot.attempts[0].session_id = LogicalSessionId::new(),
            _ => unreachable!(),
        }
        assert_worker_filter(&session, &snapshot, "読みやすい", false);
        assert_worker_filter(&session, &snapshot, &session.name, true);
    }
}

#[test]
fn sidebar_filter_preserves_workspace_and_worktree_ancestor_matches() {
    use adroit::leader_navigation::narrowed;
    let (mut tree, snapshot) = fixture();
    tree.workspaces[0].name = "Workspace ancestor".into();
    tree.workspaces[0].worktrees[0].worktree.branch = Some("Branch ancestor".into());
    let workspace = tree.workspaces[0].clone();
    assert_eq!(
        narrowed(&tree, &snapshot, "workspace ancestor").workspaces,
        vec![workspace]
    );
    let worktree = tree.workspaces[0].worktrees[0].clone();
    assert_eq!(
        narrowed(&tree, &snapshot, "branch ancestor").workspaces[0].worktrees,
        vec![worktree]
    );
}
