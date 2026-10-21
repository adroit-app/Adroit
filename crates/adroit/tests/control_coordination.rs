use std::path::Path;

use adroit::control::store::Store;
use adroit_model::*;

fn leader(store: &mut Store, cwd: &Path) -> (SessionRef, RunId, WorkAreaId) {
    let path = cwd.canonicalize().unwrap();
    let session = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Codex,
            path.clone(),
        )
        .unwrap()
        .value;
    let run = RunId::new();
    store
        .open_run(
            RequestId::new(),
            session.reference(),
            run,
            RunBrief {
                title: "Change".into(),
                goal: "Deliver the requested change".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let area = store
        .register_work_area(
            RequestId::new(),
            session.reference(),
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
                description: "Default".into(),
                state: WorkAreaState::Ready,
                error: None,
            },
            true,
        )
        .unwrap()
        .value;
    (session.reference(), run, area.id)
}

fn intent(run: RunId, area: WorkAreaId, scope: &str) -> CoordinationIntent {
    CoordinationIntent {
        id: CoordinationId::new(),
        run_id: run,
        work_area_id: area,
        operation: CoordinationOperation::Edit,
        purpose: "I am changing the default working area".into(),
        scope: vec![scope.into()],
        base: None,
        candidate: None,
    }
}

#[test]
fn related_leaders_receive_intents_and_results_without_an_acknowledgement_barrier() {
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let (first, first_run, area) = leader(&mut store, dir.path());
    let (second, second_run, second_area) = leader(&mut store, dir.path());
    let (unrelated, _, _) = leader(&mut store, other.path());
    assert_eq!(area, second_area);
    let first_intent = intent(first_run, area, "src/a.rs");
    let request = RequestId::new();
    let notice = store
        .announce_change(request, first, first_intent.clone())
        .unwrap();
    assert_eq!(
        store.announce_change(request, first, first_intent).unwrap(),
        notice
    );
    assert_eq!(store.mailbox(second, 0, 100).unwrap().messages.len(), 1);
    assert!(
        store
            .mailbox(unrelated, 0, 100)
            .unwrap()
            .messages
            .is_empty()
    );
    // Both Leaders can proceed without collecting votes or acquiring a directory lock.
    let other_notice = store
        .announce_change(
            RequestId::new(),
            second,
            intent(second_run, area, "src/b.rs"),
        )
        .unwrap()
        .value;
    let outcome = CoordinationOutcome {
        summary: "Updated src/a.rs".into(),
        scope: vec!["src/a.rs".into()],
        revision: None,
        fingerprint: Some("a-result".into()),
        unresolved: vec![],
    };
    store
        .finish_change(
            RequestId::new(),
            first,
            notice.value.intent.id,
            outcome.clone(),
        )
        .unwrap();
    assert_eq!(store.mailbox(second, 0, 100).unwrap().messages.len(), 2);
    assert!(
        store
            .mailbox(unrelated, 0, 100)
            .unwrap()
            .messages
            .is_empty()
    );
    assert_eq!(
        store
            .snapshot()
            .unwrap()
            .runs
            .iter()
            .filter(|run| run.state == RunState::Active)
            .count(),
        3
    );
    drop(store);
    let mut store = Store::open(&path).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.coordination.len(), 2);
    assert_eq!(
        snapshot
            .coordination
            .iter()
            .find(|kept| kept.intent.id == notice.value.intent.id)
            .unwrap()
            .outcome,
        Some(outcome)
    );
    assert_eq!(
        snapshot
            .coordination
            .iter()
            .find(|notice| notice.intent.id == other_notice.intent.id)
            .unwrap()
            .outcome,
        None
    );
}

#[test]
fn discussion_is_scoped_to_related_leaders_and_does_not_grant_finish_authority() {
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (first, run, area) = leader(&mut store, dir.path());
    let (second, second_run, _) = leader(&mut store, dir.path());
    let (outsider, outsider_run, _) = leader(&mut store, other.path());
    let notice = store
        .announce_change(RequestId::new(), first, intent(run, area, "src/shared.rs"))
        .unwrap()
        .value;
    let reply = CoordinationReply {
        id: MessageId::new(),
        notice_id: notice.intent.id,
        from: second,
        run_id: second_run,
        task_id: None,
        body: "My task touches another file, so proceed in parallel".into(),
    };
    store
        .discuss_change(RequestId::new(), second, reply.clone())
        .unwrap();
    assert_eq!(
        store.snapshot().unwrap().coordination_replies,
        vec![reply.clone()]
    );
    assert!(
        store
            .discuss_change(
                RequestId::new(),
                outsider,
                CoordinationReply {
                    id: MessageId::new(),
                    from: outsider,
                    run_id: outsider_run,
                    ..reply
                }
            )
            .is_err()
    );
    let outcome = CoordinationOutcome {
        summary: "Wrong owner".into(),
        scope: vec![],
        revision: None,
        fingerprint: None,
        unresolved: vec![],
    };
    assert!(
        store
            .finish_change(RequestId::new(), second, notice.intent.id, outcome)
            .is_err()
    );
    assert!(store.snapshot().unwrap().coordination[0].outcome.is_none());
}

#[tokio::test]
async fn scoped_clients_can_read_and_acknowledge_related_notices_without_reading_peer_runs() {
    use adroit::control::{client::Client, protocol::Command, service::Service};
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let path = dir.path().to_path_buf();
    let outside = other.path().to_path_buf();
    let (first, run, area, second, second_run, outsider, second_token, outside_token) = service
        .actor()
        .call(move |store| {
            let (first, run, area) = leader(store, &path);
            let (second, second_run, _) = leader(store, &path);
            let (outsider, _, _) = leader(store, &outside);
            Ok((
                first,
                run,
                area,
                second,
                second_run,
                outsider,
                store.grant(second)?,
                store.grant(outsider)?,
            ))
        })
        .await
        .unwrap();
    let source = adroit::control::client::AppClient::new(&root).as_actor(first);
    source
        .request(
            RequestId::new(),
            Command::AnnounceChange {
                intent: intent(run, area, "src/a.rs"),
            },
        )
        .await
        .unwrap();
    let recipient = Client::session(&root, second_token);
    let snapshot: ControlSnapshot = serde_json::from_value(
        recipient
            .request(RequestId::new(), Command::Snapshot)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot.runs.len(), 1);
    assert_eq!(snapshot.runs[0].id, second_run);
    assert_eq!(snapshot.coordination.len(), 1);
    assert_eq!(snapshot.messages.len(), 1);
    assert_eq!(snapshot.messages[0].from, first.id);
    assert_eq!(snapshot.messages[0].to, second.id);
    recipient
        .request(
            RequestId::new(),
            Command::Acknowledge {
                message_id: snapshot.messages[0].id,
            },
        )
        .await
        .unwrap();
    let outside = Client::session(&root, outside_token);
    let unrelated: ControlSnapshot = serde_json::from_value(
        outside
            .request(RequestId::new(), Command::Snapshot)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(unrelated.sessions[0].reference(), outsider);
    assert!(unrelated.coordination.is_empty());
    assert!(unrelated.coordination_replies.is_empty());
    assert!(unrelated.messages.is_empty());
}

#[test]
fn a_new_leader_receives_existing_change_context_when_its_shared_area_is_registered() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (first, run, area) = leader(&mut store, dir.path());
    let notice = store
        .announce_change(RequestId::new(), first, intent(run, area, "src/a.rs"))
        .unwrap()
        .value;
    let (new_leader, new_run, _) = leader(&mut store, dir.path());
    let snapshot = store.snapshot().unwrap();
    assert!(
        snapshot.coordination[0]
            .participants
            .iter()
            .any(|participant| participant.run_id == new_run)
    );
    let mailbox = store.mailbox(new_leader, 0, 100).unwrap();
    assert_eq!(mailbox.messages.len(), 1);
    assert!(
        mailbox.messages[0]
            .message
            .body
            .contains(&notice.intent.id.to_string())
    );
    assert_eq!(
        snapshot
            .runs
            .iter()
            .find(|run| run.id == new_run)
            .unwrap()
            .state,
        RunState::Active
    );
}
