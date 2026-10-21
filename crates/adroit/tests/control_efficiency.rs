use adroit::control::store::Store;
use adroit_model::*;
use serde_json::json;

fn assignment(
    store: &mut Store,
    cwd: &std::path::Path,
) -> (SessionRef, RunId, TaskVersion, AttemptRecord) {
    let leader = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Claude,
            cwd.into(),
        )
        .unwrap()
        .value
        .reference();
    let run = RunId::new();
    store
        .open_run(
            RequestId::new(),
            leader,
            run,
            RunBrief {
                title: "Efficiency".into(),
                goal: "Avoid redundant turns".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let task = TaskVersion {
        task_id: TaskId::new(),
        revision: 1,
    };
    store
        .put_task(
            RequestId::new(),
            leader,
            TaskPlan {
                id: task.task_id,
                run_id: run,
                expected_revision: None,
                brief: TaskBrief {
                    instructions: "Implement".into(),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap();
    store.queue_task(RequestId::new(), leader, task).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), leader, task, TargetSelection::default())
        .unwrap()
        .value;
    (leader, run, task, attempt)
}

#[test]
fn progress_is_durable_without_waking_a_leader_and_does_not_hide_questions() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (leader, run, task, attempt) = assignment(&mut store, dir.path());
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    for kind in [MessageKind::Note, MessageKind::Question] {
        store
            .send_message(
                RequestId::new(),
                worker,
                AgentMessage {
                    id: MessageId::new(),
                    run_id: run,
                    from: worker.id,
                    to: leader.id,
                    task_id: Some(task.task_id),
                    attempt_id: Some(attempt.id),
                    revision: Some(1),
                    kind,
                    body: "Update".into(),
                    reply_to: None,
                    blocks_work: false,
                },
            )
            .unwrap();
    }
    assert_eq!(store.mailbox(leader, 0, 10).unwrap().messages.len(), 2);
    let pending = store.next_pending_message(leader).unwrap().unwrap();
    assert_eq!(pending.kind, MessageKind::Question);
    store
        .accept_delivery(RequestId::new(), pending.id, leader)
        .unwrap();
    let page = store.mailbox(leader, 0, 10).unwrap();
    assert_eq!(page.messages[1].delivery.state, DeliveryState::Acknowledged);
    assert!(store.next_pending_message(leader).unwrap().is_none());
}

#[test]
fn controller_notes_and_leader_instructions_still_wake_their_recipients() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (leader, run, task, attempt) = assignment(&mut store, dir.path());
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    store
        .send_message(
            RequestId::new(),
            leader,
            AgentMessage {
                id: MessageId::new(),
                run_id: run,
                from: leader.id,
                to: worker.id,
                task_id: Some(task.task_id),
                attempt_id: Some(attempt.id),
                revision: Some(1),
                kind: MessageKind::Note,
                body: "Change the approach".into(),
                reply_to: None,
                blocks_work: false,
            },
        )
        .unwrap();
    assert!(store.next_pending_message(worker).unwrap().is_some());
}

#[test]
fn aliases_survive_reopening_are_recipient_scoped_and_leave_evidence_intact() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let (leader, run, task, attempt) = assignment(&mut store, dir.path());
    let full = serde_json::to_value(store.context(leader).unwrap()).unwrap();
    let compact = store.native_output(leader, "context", full).unwrap();
    let value: serde_json::Value = serde_json::from_str(&compact).unwrap();
    assert_eq!(value["run"]["id"], "r1");
    assert_eq!(value["session"]["binding"]["logical_id"], "w1");
    let args = json!({"run_id":"r1", "expected_revision":1,"brief":{"goal":"r1", "decisions":[run.to_string()]},"affected":[{"task_id":task.task_id,"revision":1}]});
    let resolved = store
        .native_arguments(leader, "update_run", args.clone())
        .unwrap();
    assert_eq!(resolved["run_id"], run.to_string());
    assert_eq!(resolved["brief"], args["brief"]);
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    assert!(
        store
            .native_arguments(worker, "cancel_run", json!({"run_id":"r1"}))
            .is_err()
    );
    drop(store);
    let mut store = Store::open(&path).unwrap();
    assert_eq!(
        store.native_arguments(leader, "update_run", args).unwrap(),
        resolved
    );
    assert_eq!(store.context(leader).unwrap().run.unwrap().id, run);
}

#[test]
fn compact_mutations_return_identity_and_revision_without_echoing_the_task_brief() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (leader, _, task, _) = assignment(&mut store, dir.path());
    let reply = store.cancel_task(RequestId::new(), leader, task).unwrap();
    let text = store
        .native_output(leader, "cancel_task", serde_json::to_value(reply).unwrap())
        .unwrap();
    assert!(text.starts_with("ok t1.1"), "{text}");
    assert!(!text.contains("Implement"));
}

#[test]
fn report_notifications_show_summary_and_references_without_double_encoded_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (leader, run, task, attempt) = assignment(&mut store, dir.path());
    let message = AgentMessage { id: MessageId::new(), run_id: run, from: attempt.session_id, to: leader.id, task_id: Some(task.task_id), attempt_id: Some(attempt.id), revision: Some(1), kind: MessageKind::Report, body: json!({"status":"failed","summary":"Checks need attention","artifact":{"kind":"report","data":{"details":"large evidence"}},"checks":[{"command":["just","ci"],"exit_code":1}],"unresolved":["Fix lint"]}).to_string(), reply_to: None, blocks_work: false };
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    store
        .send_message(RequestId::new(), worker, message.clone())
        .unwrap();
    let encoded = store.native_envelope(leader, message.id).unwrap();
    assert!(encoded.starts_with("r m1 w1 t1.1 failed\n"), "{encoded}");
    assert!(encoded.contains("Checks need attention"));
    assert!(encoded.contains("Fix lint"));
    assert!(!encoded.contains("large evidence"));
    assert!(!encoded.contains(&run.to_string()));
    let args = store
        .native_arguments(leader, "read_message", json!({"message_id":"m1"}))
        .unwrap();
    assert_eq!(args["message_id"], message.id.to_string());
    assert_eq!(store.read_message(leader, message.id).unwrap(), message);
}

#[test]
fn hash_aliases_resolve_in_structured_artifacts_but_never_in_arbitrary_json_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (leader, _, task, attempt) = assignment(&mut store, dir.path());
    let revision = "0123456789abcdef0123456789abcdef01234567";
    store
        .native_output(leader, "candidate", json!({"revision":revision}))
        .unwrap();
    let arguments = json!({"message_id":MessageId::new(),"task":task,"attempt_id":attempt.id,"result":{"status":"failed","summary":"Review","checks":[],"unresolved":[],"artifact":{"kind":"review","decision":"blocked","reviewed_revision":"h1","files":[],"data":{"revision":"h1","message_id":"m99","null":null}}}});
    let resolved = store
        .native_arguments(leader, "report_task", arguments.clone())
        .unwrap();
    assert_eq!(
        resolved["result"]["artifact"]["reviewed_revision"],
        revision
    );
    assert_eq!(
        resolved["result"]["artifact"]["data"],
        arguments["result"]["artifact"]["data"]
    );
}

#[test]
fn native_previews_are_utf8_bounded_and_do_not_put_control_bytes_in_process_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (leader, run, task, attempt) = assignment(&mut store, dir.path());
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    let message = AgentMessage {
        id: MessageId::new(),
        run_id: run,
        from: worker.id,
        to: leader.id,
        task_id: Some(task.task_id),
        attempt_id: Some(attempt.id),
        revision: Some(1),
        kind: MessageKind::Question,
        body: format!("\0{}", "日本語".repeat(6000)),
        reply_to: None,
        blocks_work: true,
    };
    store
        .send_message(RequestId::new(), worker, message.clone())
        .unwrap();
    let text = store.native_envelope(leader, message.id).unwrap();
    assert!(!text.contains('\0'));
    assert!(text.len() <= 2048);
    assert!(text.ends_with("read_message m1 for full body"));
    assert_eq!(store.read_message(leader, message.id).unwrap(), message);
}

#[test]
fn mailbox_and_snapshot_previews_reference_report_evidence_instead_of_echoing_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (leader, run, task, attempt) = assignment(&mut store, dir.path());
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    let message = AgentMessage { id: MessageId::new(),run_id:run,from:worker.id,to:leader.id,task_id:Some(task.task_id),attempt_id:Some(attempt.id),revision:Some(1),kind:MessageKind::Report,body:json!({"status":"failed","summary":"Fix CI","artifact":{"kind":"report","data":"private-large-evidence"},"checks":[{"command":["just","ci"]}],"unresolved":["lint"]}).to_string(),reply_to:None,blocks_work:false };
    store
        .send_message(RequestId::new(), worker, message.clone())
        .unwrap();
    let page = serde_json::to_value(store.mailbox(leader, 0, 10).unwrap()).unwrap();
    let preview = store.native_output(leader, "mailbox", page).unwrap();
    assert!(preview.contains("Fix CI"));
    assert!(!preview.contains("private-large-evidence"));
    assert!(!preview.contains("\\\"status\\\""));
    let full = store
        .native_output(
            leader,
            "read_message",
            serde_json::to_value(&message).unwrap(),
        )
        .unwrap();
    assert!(full.contains("private-large-evidence"));
    let report: serde_json::Value = serde_json::from_str(&message.body).unwrap();
    let text = store
        .native_output(
            leader,
            "snapshot",
            json!({"reports":[{"report":{"message_id":message.id,"result":report}}]}),
        )
        .unwrap();
    assert!(text.contains("Fix CI"));
    assert!(!text.contains("private-large-evidence"));
}

#[test]
fn alias_migration_keeps_previous_schema_ui_reads_available() {
    use adroit::control::store::ui::read_only_snapshot;
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let mut store = Store::open(&root.join("state.sqlite3")).unwrap();
    let (leader, _, _, _) = assignment(&mut store, dir.path());
    let before = store.snapshot().unwrap();
    drop(store);
    // Read-only UI fallback authenticates the instance credential, not a session grant.
    std::fs::write(root.join("human.token"), "a".repeat(72)).unwrap();
    std::fs::set_permissions(
        root.join("human.token"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let db = rusqlite::Connection::open(root.join("state.sqlite3")).unwrap();
    db.execute_batch(
        "DROP TABLE native_progress; DROP TABLE native_aliases; PRAGMA user_version = 12;",
    )
    .unwrap();
    assert_eq!(
        read_only_snapshot(&root, &"a".repeat(72))
            .unwrap()
            .sessions
            .len(),
        before.sessions.len()
    );
    drop(db);
    let mut reopened = Store::open(&root.join("state.sqlite3")).unwrap();
    assert!(reopened.context(leader).is_ok());
}

#[test]
fn full_identifiers_can_be_requested_for_cross_session_handoffs() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (leader, run, _, attempt) = assignment(&mut store, dir.path());
    let context = serde_json::to_value(store.context(leader).unwrap()).unwrap();
    store.native_output(leader, "context", context).unwrap();
    assert_eq!(
        store.resolve_native_ids(leader, vec!["r1".into()]).unwrap()["r1"],
        run.to_string()
    );
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    assert!(store.resolve_native_ids(worker, vec!["r1".into()]).is_err());
}

#[test]
fn record_alias_types_are_preserved_when_typed_namespaces_share_a_uuid() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (leader, run, _, _) = assignment(&mut store, dir.path());
    let reply = store
        .put_task(
            RequestId::new(),
            leader,
            TaskPlan {
                id: run.to_string().parse().unwrap(),
                run_id: run,
                expected_revision: None,
                brief: TaskBrief {
                    instructions: "Independent namespace".into(),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap();
    let text = store
        .native_output(leader, "put_task", serde_json::to_value(reply).unwrap())
        .unwrap();
    assert!(text.starts_with("ok t1.1"), "{text}");
    let text = store
        .native_output(
            leader,
            "report_task",
            json!({"value":{"report":{"message_id":MessageId::new()},"applied":true}}),
        )
        .unwrap();
    assert_eq!(text, "ok m1 applied=true");
}

#[test]
fn worker_exit_notifications_are_actionable_controller_events_even_when_the_kind_is_note() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (leader, run, task, attempt) = assignment(&mut store, dir.path());
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    let path = dir.path().canonicalize().unwrap();
    let area = store
        .register_work_area(
            RequestId::new(),
            leader,
            run,
            WorkArea {
                id: WorkAreaId::new(),
                backend: WorkAreaBackend::Directory,
                path: path.clone(),
                source: path.clone(),
                source_root: path,
                repository: None,
                base_revision: None,
                native_name: None,
                description: "Test area".into(),
                state: WorkAreaState::Ready,
                error: None,
            },
            true,
        )
        .unwrap()
        .value;
    store
        .bind_work_area(RequestId::new(), leader, task, attempt.id, area.id)
        .unwrap();
    store
        .begin_start(RequestId::new(), leader, task, attempt.id)
        .unwrap();
    store
        .attach_session(
            RequestId::new(),
            worker,
            area.path.clone(),
            "0123456789abcdef0123456789abcdef".into(),
            MessageId::new().to_string(),
        )
        .unwrap();
    store
        .observe_terminal_exit(
            RequestId::new(),
            worker,
            "0123456789abcdef0123456789abcdef".into(),
        )
        .unwrap();
    assert!(store.next_pending_message(leader).unwrap().is_some());
}
