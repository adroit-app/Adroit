use adroit::control::{runtime::RuntimeLaunch, store::Store};
use adroit::settings::Settings;
use adroit_model::*;

#[test]
fn a_sessions_native_route_survives_restart_without_entering_snapshots_or_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let session = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Codex,
            dir.path().into(),
        )
        .unwrap()
        .value;
    let mut settings = Settings::default();
    settings.codex.command = vec!["/private/provider-account-a/codex".into()];
    settings.codex.env.insert(
        "CODEX_HOME".into(),
        "/private/provider-account-a/state".into(),
    );
    settings.codex.env.insert(
        "TEST_PROVIDER_SECRET".into(),
        "private-test-route-token".into(),
    );
    let runtime = RuntimeLaunch::new(settings);
    store
        .remember_launch(RequestId::new(), session.reference(), runtime.clone())
        .unwrap();
    let serialized = serde_json::to_string(&store.snapshot().unwrap()).unwrap();
    assert!(!serialized.contains("private-test-route-token"));
    let events = serde_json::to_string(&store.events_after(0, 100).unwrap()).unwrap();
    assert!(!events.contains("private-test-route-token"));
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.runtime(session.reference()).unwrap(), Some(runtime));
    assert!(
        store
            .runtime(SessionRef {
                id: session.reference().id,
                generation: 2
            })
            .is_err()
    );
}

#[test]
fn a_failed_native_delivery_retains_the_message_id_and_does_not_retry_it_automatically() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let leader = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Codex,
            dir.path().into(),
        )
        .unwrap()
        .value;
    let identity = leader.reference();
    let run = RunId::new();
    store
        .open_run(
            RequestId::new(),
            identity,
            run,
            RunBrief {
                title: "Review".into(),
                goal: "Review the inputs".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let task = TaskId::new();
    store
        .put_task(
            RequestId::new(),
            identity,
            TaskPlan {
                id: task,
                run_id: run,
                expected_revision: None,
                brief: TaskBrief {
                    instructions: "Review".into(),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap();
    let version = TaskVersion {
        task_id: task,
        revision: 1,
    };
    store
        .queue_task(RequestId::new(), identity, version)
        .unwrap();
    let attempt = store
        .reserve_attempt(
            RequestId::new(),
            identity,
            version,
            TargetSelection::default(),
        )
        .unwrap()
        .value;
    let message = AgentMessage {
        id: MessageId::new(),
        run_id: run,
        from: attempt.session_id,
        to: identity.id,
        task_id: Some(task),
        attempt_id: Some(attempt.id),
        revision: Some(1),
        kind: MessageKind::Question,
        body: "Review needs clarification".into(),
        reply_to: None,
        blocks_work: true,
    };
    store
        .send_message(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            message.clone(),
        )
        .unwrap();
    store
        .claim_delivery(RequestId::new(), identity, message.id)
        .unwrap();
    store
        .delivery_uncertain(
            RequestId::new(),
            identity,
            message.id,
            "Queue result was not observable".into(),
        )
        .unwrap();
    assert!(
        store
            .claim_delivery(RequestId::new(), identity, message.id)
            .unwrap()
            .value
            .is_none()
    );
    let page = store.mailbox(identity, 0, 10).unwrap();
    assert_eq!(page.messages[0].message.id, message.id);
    assert_eq!(page.messages[0].delivery.state, DeliveryState::Uncertain);
    assert_eq!(page.messages[0].delivery.attempts, 1);
}

#[tokio::test]
async fn native_notifications_use_the_recipients_saved_route_and_are_not_repeated_after_acceptance()
{
    use adroit::control::{actor::Actor, dispatcher::Dispatcher};
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("native-queue");
    let receipt = dir.path().join("receipt");
    std::fs::write(
        &program,
        "#!/bin/sh\nprintf '%s\\n' \"$CODEX_HOME\" \"$@\" > \"$ADROIT_TEST_DELIVERY\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let actor = Actor::open(dir.path().join("control/state.sqlite3"))
        .await
        .unwrap();
    let cwd = dir.path().to_path_buf();
    let output = receipt.clone();
    let (_identity, message_id) = actor
        .call(move |store| {
            let session = store
                .register_leader(
                    RequestId::new(),
                    LogicalSessionId::new(),
                    Agent::Codex,
                    cwd.clone(),
                )?
                .value;
            let identity = session.reference();
            let mut settings = Settings::default();
            settings.codex.command = vec![program.to_string_lossy().into_owned()];
            settings
                .codex
                .env
                .insert("CODEX_HOME".into(), "/private/account-a".into());
            settings.codex.env.insert(
                "ADROIT_TEST_DELIVERY".into(),
                output.to_string_lossy().into_owned(),
            );
            store.remember_launch(RequestId::new(), identity, RuntimeLaunch::new(settings))?;
            store.attach_session(
                RequestId::new(),
                identity,
                cwd,
                "000000000000000000000000000000ab".into(),
                MessageId::new().to_string(),
            )?;
            let run = RunId::new();
            store.open_run(
                RequestId::new(),
                identity,
                run,
                RunBrief {
                    title: "Review".into(),
                    goal: "Review".into(),
                    ..Default::default()
                },
            )?;
            let task = TaskId::new();
            store.put_task(
                RequestId::new(),
                identity,
                TaskPlan {
                    id: task,
                    run_id: run,
                    expected_revision: None,
                    brief: TaskBrief {
                        instructions: "Review".into(),
                        ..Default::default()
                    },
                    dependencies: vec![],
                },
            )?;
            let wanted = TaskVersion {
                task_id: task,
                revision: 1,
            };
            store.queue_task(RequestId::new(), identity, wanted)?;
            let attempt = store
                .reserve_attempt(
                    RequestId::new(),
                    identity,
                    wanted,
                    TargetSelection::default(),
                )?
                .value;
            for _ in 0..101 {
                let id = MessageId::new();
                store.send_message(
                    RequestId::new(),
                    SessionRef {
                        id: attempt.session_id,
                        generation: 1,
                    },
                    AgentMessage {
                        id,
                        run_id: run,
                        from: attempt.session_id,
                        to: identity.id,
                        task_id: Some(task),
                        attempt_id: Some(attempt.id),
                        revision: Some(1),
                        kind: MessageKind::Note,
                        body: "Already accepted history".into(),
                        reply_to: None,
                        blocks_work: false,
                    },
                )?;
                store.accept_delivery(RequestId::new(), id, identity.id)?;
            }
            let id = MessageId::new();
            store.send_message(
                RequestId::new(),
                SessionRef {
                    id: attempt.session_id,
                    generation: 1,
                },
                AgentMessage {
                    id,
                    run_id: run,
                    from: attempt.session_id,
                    to: identity.id,
                    task_id: Some(task),
                    attempt_id: Some(attempt.id),
                    revision: Some(1),
                    kind: MessageKind::Question,
                    body: "Which inputs should I use?".into(),
                    reply_to: None,
                    blocks_work: true,
                },
            )?;
            Ok((identity, id))
        })
        .await
        .unwrap();
    let mut dispatcher = Dispatcher::new(actor.clone());
    assert_eq!(dispatcher.tick().await.unwrap(), 1);
    for _ in 0..200 {
        let snapshot = actor.call(|store| store.snapshot()).await.unwrap();
        if snapshot.deliveries.iter().any(|delivery| {
            delivery.message_id == message_id && delivery.state == DeliveryState::Accepted
        }) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(dispatcher.tick().await.unwrap(), 0);
    let snapshot = actor.call(|store| store.snapshot()).await.unwrap();
    let delivery = snapshot
        .deliveries
        .iter()
        .find(|delivery| delivery.message_id == message_id)
        .unwrap();
    assert_eq!(delivery.state, DeliveryState::Accepted);
    assert_eq!(delivery.attempts, 1);
    let sent = std::fs::read_to_string(receipt).unwrap();
    assert_eq!(sent.lines().next(), Some("/private/account-a"));
    assert!(sent.contains(&message_id.to_string()));
    assert!(sent.contains("Which inputs should I use?"));
}

#[test]
fn the_first_native_conversation_is_learned_but_a_switch_requires_reconciliation() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let session = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Codex,
            dir.path().into(),
        )
        .unwrap()
        .value;
    let terminal = "000000000000000000000000000000ab".to_string();
    store
        .attach_session(
            RequestId::new(),
            session.reference(),
            dir.path().into(),
            terminal.clone(),
            String::new(),
        )
        .unwrap();
    let first = MessageId::new().to_string();
    let learned = store
        .observe_conversation(
            RequestId::new(),
            session.reference(),
            terminal.clone(),
            first.clone(),
        )
        .unwrap()
        .value;
    assert_eq!(learned.conversation_id, Some(first.clone()));
    assert_eq!(learned.state, ManagedSessionState::Running);
    let second = MessageId::new().to_string();
    let switched = store
        .observe_conversation(RequestId::new(), session.reference(), terminal, second)
        .unwrap()
        .value;
    assert_eq!(switched.conversation_id, Some(first));
    assert_eq!(switched.state, ManagedSessionState::NeedsReconciliation);
    assert_eq!(switched.binding, session.binding);
}

#[test]
fn a_private_launch_selection_cannot_change_the_sessions_agent_or_permissions() {
    for wrong_agent in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
        let session = store
            .register_leader(
                RequestId::new(),
                LogicalSessionId::new(),
                Agent::Codex,
                dir.path().into(),
            )
            .unwrap()
            .value;
        let target = TargetSelection {
            agent: if wrong_agent {
                Agent::Claude
            } else {
                Agent::Codex
            },
            permissions: if wrong_agent {
                AgentPermissions::Default
            } else {
                AgentPermissions::ReadOnly
            },
            ..Default::default()
        };
        assert!(
            store
                .remember_launch(
                    RequestId::new(),
                    session.reference(),
                    RuntimeLaunch::new(Settings::default()).with_target(target)
                )
                .is_err()
        );
        assert!(store.runtime(session.reference()).unwrap().is_none());
    }
}
