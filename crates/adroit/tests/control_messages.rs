use adroit::control::{envelope, store::Store};
use adroit_model::*;

fn message(body: String) -> AgentMessage {
    AgentMessage {
        id: MessageId::new(),
        run_id: RunId::new(),
        from: LogicalSessionId::new(),
        to: LogicalSessionId::new(),
        task_id: None,
        attempt_id: None,
        revision: None,
        kind: MessageKind::Note,
        body,
        reply_to: None,
        blocks_work: false,
    }
}

#[test]
fn short_native_notifications_keep_the_complete_message_without_an_extra_read() {
    let message = message("The result is ready".into());
    let encoded = envelope::encode(&message).unwrap();
    assert_eq!(
        serde_json::from_str::<AgentMessage>(&encoded).unwrap(),
        message
    );
}

#[test]
fn large_or_heavily_escaped_bodies_use_a_bounded_reference_instead_of_native_argv() {
    for body in ["x".repeat(65536), "\0".repeat(65536), "\"".repeat(65536)] {
        let message = message(body);
        let encoded = envelope::encode(&message).unwrap();
        assert!(encoded.len() <= 16 * 1024);
        let reference: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(reference["type"], "adroit_message_reference");
        assert_eq!(reference["message_id"], message.id.to_string());
        assert_eq!(reference["run_id"], message.run_id.to_string());
        assert!(reference.get("body").is_none());
    }
}

#[tokio::test]
async fn only_the_addressed_scope_can_fetch_the_complete_immutable_body() {
    use adroit::control::{client::Client, protocol::Command, service::Service};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let mut store = Store::open(&root.join("state.sqlite3")).unwrap();
    let mut owners = Vec::new();
    for _ in 0..2 {
        let owner = store
            .register_leader(
                RequestId::new(),
                LogicalSessionId::new(),
                Agent::Codex,
                dir.path().into(),
            )
            .unwrap()
            .value
            .reference();
        let run = RunId::new();
        store
            .open_run(
                RequestId::new(),
                owner,
                run,
                RunBrief {
                    title: "Request".into(),
                    goal: "Read my messages".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        owners.push((owner, run, store.grant(owner).unwrap()));
    }
    let task = store
        .put_task(
            RequestId::new(),
            owners[0].0,
            TaskPlan {
                id: TaskId::new(),
                run_id: owners[0].1,
                expected_revision: None,
                brief: TaskBrief {
                    instructions: "Send the full finding".into(),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap()
        .value;
    let version = TaskVersion {
        task_id: task.id,
        revision: 1,
    };
    store
        .queue_task(RequestId::new(), owners[0].0, version)
        .unwrap();
    let attempt = store
        .reserve_attempt(
            RequestId::new(),
            owners[0].0,
            version,
            TargetSelection::default(),
        )
        .unwrap()
        .value;
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    let mut sent = message("\0".repeat(65536));
    sent.run_id = owners[0].1;
    sent.from = worker.id;
    sent.to = owners[0].0.id;
    sent.task_id = Some(task.id);
    sent.attempt_id = Some(attempt.id);
    sent.revision = Some(1);
    store
        .send_message(RequestId::new(), worker, sent.clone())
        .unwrap();
    drop(store);
    let _service = Service::bind(&root).await.unwrap();
    let own = Client::session(&root, owners[0].2.clone());
    let foreign = Client::session(&root, owners[1].2.clone());
    let read = own
        .request(
            RequestId::new(),
            Command::ReadMessage {
                message_id: sent.id,
            },
        )
        .await
        .unwrap();
    let read: AgentMessage = serde_json::from_value(read).unwrap();
    assert_eq!(read, sent);
    let snapshot = own
        .request(RequestId::new(), Command::Snapshot)
        .await
        .unwrap();
    assert_eq!(snapshot["deliveries"][0]["state"], "stored");
    assert!(
        foreign
            .request(
                RequestId::new(),
                Command::ReadMessage {
                    message_id: sent.id
                }
            )
            .await
            .is_err()
    );
    own.request(
        RequestId::new(),
        Command::Acknowledge {
            message_id: sent.id,
        },
    )
    .await
    .unwrap();
    let read = own
        .request(
            RequestId::new(),
            Command::ReadMessage {
                message_id: sent.id,
            },
        )
        .await
        .unwrap();
    assert_eq!(read["body"], sent.body);
}
