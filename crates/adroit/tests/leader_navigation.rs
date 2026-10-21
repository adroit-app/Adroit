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
fn workspaces_hide_managed_sessions_until_requested_without_changing_the_native_tree() {
    use adroit::leader_navigation::workspace_tree;
    let (mut tree, _) = fixture();
    let mut standalone = tree.sessions().next().unwrap().1.clone();
    standalone.id = "standalone".into();
    standalone.binding = None;
    tree.workspaces[0].worktrees[0]
        .sessions
        .push(standalone.clone());
    let visible = workspace_tree(&tree, false);
    assert_eq!(visible.sessions().count(), 1);
    assert_eq!(visible.find("standalone").unwrap().2, &standalone);
    assert_eq!(visible.workspaces.len(), tree.workspaces.len());
    assert_eq!(workspace_tree(&tree, true), tree);
    assert_eq!(tree.sessions().count(), 5);
}

#[test]
fn repository_groups_keep_cross_repository_workers_with_their_leader_and_can_fold() {
    use adroit::leader_navigation::repository_items;
    let (tree, snapshot) = fixture();
    let rows = project(&tree, &snapshot);
    let items = repository_items(&tree, &rows, &Default::default());
    assert_eq!(items[0], Item::Workspace("/repo".into()));
    assert_eq!(
        items[1..],
        rows.iter().map(|row| row.item.clone()).collect::<Vec<_>>()
    );
    assert_eq!(
        items
            .iter()
            .filter(|item| matches!(item, Item::Workspace(_)))
            .count(),
        1
    );
    let collapsed = ["ws:/repo".to_owned()].into_iter().collect();
    assert_eq!(
        repository_items(&tree, &rows, &collapsed),
        vec![Item::Workspace("/repo".into())]
    );
}

#[test]
fn distinct_repositories_keep_independent_request_groups_and_completed_workers() {
    use adroit::leader_navigation::{Completions, group_completed, repository_items};
    let (mut tree, snapshot) = completed_fixture();
    let first = tree
        .find(snapshot.sessions[0].terminal_id.as_ref().unwrap())
        .unwrap()
        .0
        .root
        .clone();
    let mut other = tree.workspaces[0].clone();
    other.root = "/other".into();
    other.worktrees[0].sessions[0].id = "standalone-other".into();
    other.worktrees[0].sessions[0].binding = None;
    other.worktrees[0].sessions.truncate(1);
    tree.workspaces.push(other);
    let rows = group_completed(
        &project(&tree, &snapshot),
        &tree,
        &snapshot,
        &Completions::default(),
        &Default::default(),
        &Default::default(),
    );
    let items = repository_items(&tree, &rows, &Default::default());
    assert_eq!(
        items
            .iter()
            .filter(|item| matches!(item, Item::Workspace(_)))
            .count(),
        2
    );
    assert_eq!(items[0], Item::Workspace(first));
    assert!(items.iter().any(|item| matches!(item, Item::Completed(_))));
    let other = items
        .iter()
        .position(|item| *item == Item::Workspace("/other".into()))
        .unwrap();
    assert_eq!(items[other + 1], Item::Session("standalone-other".into()));
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

fn completed_fixture() -> (Tree, ControlSnapshot) {
    let (session, mut snapshot) = worker_fixture();
    snapshot.tasks[0].state = TaskState::Succeeded;
    snapshot.attempts[0].state = AttemptState::Succeeded;
    snapshot.reports.push(TaskReportRecord {
        run_id: snapshot.tasks[0].run_id,
        applied: true,
        report: TaskReport {
            message_id: MessageId::new(),
            task: TaskVersion {
                task_id: snapshot.tasks[0].id,
                revision: 1,
            },
            attempt_id: snapshot.attempts[0].id,
            result: TaskResult {
                status: ReportStatus::Succeeded,
                summary: "done".into(),
                artifact: None,
                checks: vec![],
                unresolved: vec![],
            },
        },
    });
    // Use the fixture's matching authoritative leader with the completed Worker.
    let leader = Session {
        id: snapshot.sessions[0].terminal_id.clone().unwrap(),
        binding: Some(snapshot.sessions[0].binding.clone()),
        agent: snapshot.sessions[0].agent,
        cwd: "/repo".into(),
        ..Default::default()
    };
    let tree = build(&[leader, session], &[], &HashMap::new(), &[], None);
    (tree, snapshot)
}

#[test]
fn completed_workers_collapse_and_expand_without_losing_native_open_paths() {
    use adroit::leader_navigation::Completions;
    use adroit::leader_navigation::group_completed;
    let (tree, snapshot) = completed_fixture();
    let before = (tree.clone(), snapshot.clone());
    let rows = project(&tree, &snapshot);
    let run = snapshot.runs[0].id;
    let grouped = group_completed(
        &rows,
        &tree,
        &snapshot,
        &Completions::default(),
        &Default::default(),
        &Default::default(),
    );
    assert_eq!(
        grouped.iter().map(|r| r.item.clone()).collect::<Vec<_>>(),
        vec![rows[0].item.clone(), Item::Completed(run)]
    );
    let expanded = group_completed(
        &rows,
        &tree,
        &snapshot,
        &Completions::default(),
        &[run].into(),
        &Default::default(),
    );
    assert_eq!(expanded.last().unwrap().item, rows[1].item);
    assert_eq!(
        adroit::nav::step(
            &expanded.iter().map(|r| r.item.clone()).collect::<Vec<_>>(),
            Some(&Item::Completed(run)),
            1
        ),
        Some(rows[1].item.clone())
    );
    let Item::Session(id) = &rows[1].item else {
        panic!()
    };
    assert!(adroit::nav::find(&tree, id).is_some());
    assert!(adroit::leader_navigation::terminal_binding(&tree, &snapshot, &rows[1].item).is_some());
    assert_eq!((tree, snapshot), before);
}

#[test]
fn completed_grouping_requires_current_final_success_and_no_attention() {
    use adroit::leader_navigation::Completions;
    use adroit::leader_navigation::group_completed;
    for mutation in 0..17 {
        let (mut tree, mut snapshot) = completed_fixture();
        let id = tree
            .sessions()
            .find(|(_, s)| s.binding.as_ref().unwrap().role() == SessionRole::Worker)
            .unwrap()
            .1
            .id
            .clone();
        let worker = tree
            .workspaces
            .iter_mut()
            .flat_map(|w| &mut w.worktrees)
            .flat_map(|w| &mut w.sessions)
            .find(|s| s.id == id)
            .unwrap();
        let mut reveal = std::collections::HashSet::new();
        match mutation {
            0 => snapshot.tasks[0].revision += 1,
            1 => snapshot.tasks[0].current_attempt = None,
            2 => snapshot.tasks[0].state = TaskState::Verifying,
            3 => snapshot.tasks[0].brief.candidate_id = Some(CandidateId::new()),
            4 => snapshot.reports.clear(),
            5 => snapshot.reports[0].applied = false,
            6 => snapshot.reports[0].report.task.revision += 1,
            7 => snapshot.attempts[0].state = AttemptState::Uncertain,
            8 => snapshot.attempts.clear(),
            9 => worker.state = State::Working,
            10 => worker.state = State::Waiting(Wait::Permission),
            11 => worker.state = State::Waiting(Wait::Input),
            12 => worker.state = State::Done,
            13 => worker.last_input_ms = Some(1),
            14 => {
                reveal.insert(id.clone());
            }
            15 => snapshot.sessions[1].terminal_id = Some("replacement".into()),
            16 => snapshot.tasks.clear(),
            _ => unreachable!(),
        }
        let rows = project(&tree, &snapshot);
        let grouped = group_completed(
            &rows,
            &tree,
            &snapshot,
            &Completions::default(),
            &Default::default(),
            &reveal,
        );
        assert!(
            grouped.iter().any(|r| r.item == Item::Session(id.clone())),
            "mutation {mutation}"
        );
        assert!(
            mutation == 14 || !grouped.iter().any(|r| matches!(r.item, Item::Completed(_))),
            "mutation {mutation}"
        );
    }
}

#[test]
fn completion_attention_requires_acknowledgement_and_answers_at_both_ends() {
    use adroit::leader_navigation::{Completions, group_completed};
    for state in [
        None,
        Some(DeliveryState::Stored),
        Some(DeliveryState::Dispatching),
        Some(DeliveryState::Accepted),
        Some(DeliveryState::Uncertain),
        Some(DeliveryState::Acknowledged),
    ] {
        for inbound in [false, true] {
            for question in [false, true] {
                let (tree, mut snapshot) = completed_fixture();
                let worker = snapshot.sessions[1].binding.logical_id();
                let leader = snapshot.runs[0].leader_id;
                let message = AgentMessage {
                    id: MessageId::new(),
                    run_id: snapshot.runs[0].id,
                    from: if inbound { leader } else { worker },
                    to: if inbound { worker } else { leader },
                    task_id: None,
                    attempt_id: None,
                    revision: None,
                    kind: if question {
                        MessageKind::Question
                    } else {
                        MessageKind::Note
                    },
                    body: "attention".into(),
                    reply_to: None,
                    blocks_work: question,
                };
                if let Some(state) = state {
                    snapshot.deliveries.push(DeliveryRecord {
                        message_id: message.id,
                        recipient: message.to,
                        state,
                        attempts: 1,
                        last_error: None,
                    });
                }
                snapshot.messages.push(message.clone());
                let grouped = |snapshot: &ControlSnapshot| {
                    group_completed(
                        &project(&tree, snapshot),
                        &tree,
                        snapshot,
                        &Completions::default(),
                        &Default::default(),
                        &Default::default(),
                    )
                };
                assert_eq!(
                    grouped(&snapshot)
                        .iter()
                        .any(|row| matches!(row.item, Item::Completed(_))),
                    state == Some(DeliveryState::Acknowledged) && !question
                );
                if question && state == Some(DeliveryState::Acknowledged) {
                    let answer = AgentMessage {
                        id: MessageId::new(),
                        from: message.to,
                        to: message.from,
                        kind: MessageKind::Answer,
                        reply_to: Some(message.id),
                        blocks_work: false,
                        ..message
                    };
                    snapshot.deliveries.push(DeliveryRecord {
                        message_id: answer.id,
                        recipient: answer.to,
                        state: DeliveryState::Acknowledged,
                        attempts: 1,
                        last_error: None,
                    });
                    snapshot.messages.push(answer);
                    assert!(
                        grouped(&snapshot)
                            .iter()
                            .any(|row| matches!(row.item, Item::Completed(_)))
                    );
                }
            }
        }
    }
}

#[test]
fn eleven_successful_workers_with_running_native_sessions_leave_one_completed_group() {
    use adroit::leader_navigation::{Completions, group_completed, worker_presentation};
    let (mut tree, mut snapshot) = completed_fixture();
    let worker = tree
        .sessions()
        .find(|(_, s)| s.binding.as_ref().unwrap().role() == SessionRole::Worker)
        .unwrap()
        .1
        .clone();
    for index in 1..11 {
        let mut native = worker.clone();
        let task_id = TaskId::new();
        let attempt_id = AttemptId::new();
        let logical = LogicalSessionId::new();
        native.id = format!("completed-{index}");
        native.binding = Some(
            SessionBinding::worker(
                logical,
                snapshot.runs[0].leader_id,
                snapshot.runs[0].id,
                task_id,
                attempt_id,
            )
            .unwrap(),
        );
        let mut managed = snapshot.sessions[1].clone();
        managed.binding = native.binding.clone().unwrap();
        managed.terminal_id = Some(native.id.clone());
        snapshot.sessions.push(managed);
        let mut task = snapshot.tasks[0].clone();
        task.id = task_id;
        task.current_attempt = Some(attempt_id);
        snapshot.tasks.push(task);
        let mut attempt = snapshot.attempts[0].clone();
        attempt.id = attempt_id;
        attempt.task_id = task_id;
        attempt.session_id = logical;
        snapshot.attempts.push(attempt);
        let mut report = snapshot.reports[0].clone();
        report.report.message_id = MessageId::new();
        report.report.task.task_id = task_id;
        report.report.attempt_id = attempt_id;
        snapshot.reports.push(report);
        tree.workspaces[0].worktrees[0].sessions.push(native);
    }
    let before = (tree.clone(), snapshot.clone());
    let rows = project(&tree, &snapshot);
    assert_eq!(rows.len(), 12);
    assert!(
        snapshot
            .sessions
            .iter()
            .all(|s| s.state == ManagedSessionState::Running)
    );
    let collapsed = group_completed(
        &rows,
        &tree,
        &snapshot,
        &Completions::default(),
        &Default::default(),
        &Default::default(),
    );
    assert_eq!(collapsed.len(), 2);
    assert_eq!(collapsed[1].role, RowRole::Completed(11));
    let expanded = group_completed(
        &rows,
        &tree,
        &snapshot,
        &Completions::default(),
        &[snapshot.runs[0].id].into(),
        &Default::default(),
    );
    assert_eq!(expanded.len(), 13);
    for (index, row) in expanded[2..].iter().enumerate() {
        let Item::Session(id) = &row.item else {
            panic!()
        };
        let native = tree.find(id).unwrap().2;
        assert_eq!(
            worker_presentation(native, &snapshot).unwrap().metadata,
            format!("Worker {}", index + 1)
        );
    }
    let item = &expanded[1].item;
    assert_eq!(
        adroit::menu::entries(&adroit::menu::Target::Row(item.clone()), None),
        vec![(adroit::keymap::Action::CursorOpen, "Fold or unfold")]
    );
    assert_eq!((tree, snapshot), before);
}

#[test]
fn viewed_completions_can_fold_and_selected_or_active_rows_reveal_without_reordering() {
    use adroit::leader_navigation::{Completions, group_completed};
    let (mut tree, snapshot) = completed_fixture();
    let worker = tree
        .workspaces
        .iter_mut()
        .flat_map(|w| &mut w.worktrees)
        .flat_map(|w| &mut w.sessions)
        .find(|s| s.binding.as_ref().unwrap().role() == SessionRole::Worker)
        .unwrap();
    worker.state = State::Done;
    let id = worker.id.clone();
    let marks = Completions::default().updated(&tree, Some(&id));
    let rows = project(&tree, &snapshot);
    let folded = group_completed(
        &rows,
        &tree,
        &snapshot,
        &marks,
        &Default::default(),
        &Default::default(),
    );
    assert_eq!(folded.len(), 2);
    let revealed = group_completed(
        &rows,
        &tree,
        &snapshot,
        &marks,
        &Default::default(),
        &[id.clone()].into(),
    );
    assert_eq!(revealed.len(), 3);
    assert_eq!(revealed[1].item, folded[1].item);
    assert_eq!(revealed[2].item, Item::Session(id));
    let items: Vec<_> = revealed.iter().map(|row| row.item.clone()).collect();
    assert_eq!(
        adroit::nav::step(&items, Some(&items[2]), -1),
        Some(items[1].clone())
    );
    assert_eq!(
        adroit::nav::step(&items, Some(&items[1]), 1),
        Some(items[2].clone())
    );
    assert!(
        adroit::nav::next_session(
            &tree,
            Some(&adroit::tabs::Tab::Session(
                tree.sessions().next().unwrap().1.id.clone()
            )),
            None,
            true
        )
        .is_some()
    );
}

#[test]
fn ordinary_startup_prompt_and_acknowledged_final_report_do_not_prevent_grouping() {
    use adroit::leader_navigation::{Completions, group_completed};
    for agent in Agent::ALL {
        let (mut tree, mut snapshot) = completed_fixture();
        let worker = tree
            .workspaces
            .iter_mut()
            .flat_map(|ws| &mut ws.worktrees)
            .flat_map(|wt| &mut wt.sessions)
            .find(|s| s.binding.as_ref().unwrap().role() == SessionRole::Worker)
            .unwrap();
        worker.agent = agent;
        snapshot.sessions[1].agent = agent;
        let prompt = "Complete ordinary work\n\nTask revision: 1\nScope: \nInputs: ";
        let launch = adroit::execution::resolve(
            &adroit::settings::Settings::default(),
            adroit::execution::StartRequest {
                agent,
                name: worker.name.clone(),
                cwd: worker.cwd.clone(),
                binding: worker.binding.clone(),
                prompt: prompt.into(),
                ..Default::default()
            },
            std::path::Path::new("/tmp/adroit-regression"),
        )
        .unwrap();
        assert!(launch.spawn.args.iter().any(|arg| arg == prompt));
        assert!(adroit_term::tags::value(&launch.spawn.tags, "adroit.last-input").is_none());
        worker.last_input_ms = adroit_term::tags::value(&launch.spawn.tags, "adroit.last-input")
            .map(|x| x.parse().unwrap());
        let worker_id = worker.binding.as_ref().unwrap().logical_id();
        let native_id = worker.id.clone();
        let report = &snapshot.reports[0];
        let message = AgentMessage {
            id: report.report.message_id,
            run_id: report.run_id,
            from: worker_id,
            to: snapshot.runs[0].leader_id,
            task_id: Some(report.report.task.task_id),
            attempt_id: Some(report.report.attempt_id),
            revision: Some(report.report.task.revision),
            kind: MessageKind::Report,
            body: "ordinary final applied report".into(),
            reply_to: None,
            blocks_work: false,
        };
        snapshot.messages.push(message.clone());
        snapshot.deliveries.push(DeliveryRecord {
            message_id: message.id,
            recipient: message.to,
            state: DeliveryState::Accepted,
            attempts: 1,
            last_error: None,
        });
        let rows = project(&tree, &snapshot);
        let grouped = group_completed(
            &rows,
            &tree,
            &snapshot,
            &Completions::default(),
            &Default::default(),
            &Default::default(),
        );
        assert!(
            !grouped
                .iter()
                .any(|row| matches!(row.item, Item::Completed(_)))
        );
        assert!(
            grouped
                .iter()
                .any(|row| row.item == Item::Session(native_id.clone()))
        );
        snapshot.deliveries[0].state = DeliveryState::Acknowledged;
        let before = (tree.clone(), snapshot.clone());
        let grouped = group_completed(
            &rows,
            &tree,
            &snapshot,
            &Completions::default(),
            &Default::default(),
            &Default::default(),
        );
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[1].role, RowRole::Completed(1));
        assert_eq!(snapshot.sessions[1].state, ManagedSessionState::Running);
        assert_eq!((tree, snapshot), before);
    }
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

#[test]
fn label_search_reveals_a_collapsed_completed_worker_without_changing_its_identity() {
    use adroit::leader_navigation::{Completions, group_completed, narrowed, worker_presentation};
    use adroit::nav;
    use std::collections::HashSet;

    let (tree, mut snapshot) = completed_fixture();
    snapshot.tasks[0].brief.instructions = "Fix scheduler\nPrivate detail".into();
    let worker = tree
        .sessions()
        .find(|(_, session)| session.binding.as_ref().unwrap().role() == SessionRole::Worker)
        .unwrap()
        .1;
    let before = (tree.clone(), snapshot.clone());
    let rows = project(&tree, &snapshot);
    let folded = group_completed(
        &rows,
        &tree,
        &snapshot,
        &Completions::default(),
        &HashSet::new(),
        &HashSet::new(),
    );
    assert!(
        !folded
            .iter()
            .any(|row| row.item == Item::Session(worker.id.clone()))
    );
    assert_eq!(folded.last().unwrap().role, RowRole::Completed(1));

    for query in ["SCHEDULER", worker.name.as_str()] {
        let shown = narrowed(&tree, &snapshot, query);
        let workspace_items = nav::items(&shown, &inbox(&shown), &HashSet::new());
        // A query exposes the full projection before matching native identities.
        let leader_items: Vec<_> = rows
            .iter()
            .filter(|row| match &row.item {
                Item::Session(id) => shown.find(id).is_some(),
                _ => false,
            })
            .map(|row| row.item.clone())
            .collect();
        for items in [workspace_items, leader_items] {
            assert!(items.contains(&Item::Session(worker.id.clone())), "{query}");
        }
        assert_eq!(shown.find(&worker.id).unwrap().2, worker);
        assert_eq!(
            worker_presentation(worker, &snapshot).unwrap().metadata,
            "Worker 1"
        );
    }
    assert!(
        narrowed(&tree, &snapshot, "Private detail")
            .find(&worker.id)
            .is_none()
    );
    assert_eq!(
        group_completed(
            &rows,
            &tree,
            &snapshot,
            &Completions::default(),
            &HashSet::new(),
            &HashSet::new()
        ),
        folded,
    );
    assert_eq!((tree, snapshot), before);
}
