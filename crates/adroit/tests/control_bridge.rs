use adroit::control::{bridge, client::Client, protocol::Command, service::Service, store::Store};
use adroit_model::*;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::test]
async fn a_channel_write_is_accepted_and_only_an_agent_acknowledges_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let mut store = Store::open(&root.join("state.sqlite3")).unwrap();
    let leader = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Claude,
            dir.path().into(),
        )
        .unwrap()
        .value;
    let owner = leader.reference();
    let run = RunId::new();
    store
        .open_run(
            RequestId::new(),
            owner,
            run,
            RunBrief {
                title: "Review".into(),
                goal: "Review the change".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let task = TaskId::new();
    store
        .put_task(
            RequestId::new(),
            owner,
            TaskPlan {
                id: task,
                run_id: run,
                expected_revision: None,
                brief: TaskBrief {
                    instructions: "Review the change".into(),
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
    store.queue_task(RequestId::new(), owner, version).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), owner, version, TargetSelection::default())
        .unwrap()
        .value;
    let message = AgentMessage {
        id: MessageId::new(),
        run_id: run,
        from: attempt.session_id,
        to: owner.id,
        task_id: Some(task),
        attempt_id: Some(attempt.id),
        revision: Some(1),
        kind: MessageKind::Report,
        body: "Review completed".into(),
        reply_to: None,
        blocks_work: false,
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
    let token = store.grant(owner).unwrap();
    drop(store);
    let _service = Service::bind(&root).await.unwrap();
    let human = Client::human(&root).unwrap();
    let (input, mut send) = tokio::io::duplex(8192);
    let (output, receive) = tokio::io::duplex(8192);
    let task = tokio::spawn(bridge::run(
        BufReader::new(input),
        output,
        Client::session(&root, token),
        true,
    ));
    let mut receive = BufReader::new(receive);
    send.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\"}}\n").await.unwrap();
    let mut line = String::new();
    receive.read_line(&mut line).await.unwrap();
    let reply: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(
        reply["result"]["capabilities"]["experimental"]["claude/channel"],
        json!({})
    );
    send.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .await
        .unwrap();
    line.clear();
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        receive.read_line(&mut line),
    )
    .await
    .unwrap()
    .unwrap();
    let event: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(event["method"], "notifications/claude/channel");
    assert_eq!(
        event["params"]["meta"]["message_id"],
        message.id.to_string()
    );
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let snapshot = human
                .request(RequestId::new(), Command::Snapshot)
                .await
                .unwrap();
            if snapshot["deliveries"][0]["state"] == "accepted" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let call = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"control","arguments":{"request_id":RequestId::new(),"command":{"op":"acknowledge","message_id":message.id}}}});
    send.write_all(format!("{call}\n").as_bytes())
        .await
        .unwrap();
    line.clear();
    receive.read_line(&mut line).await.unwrap();
    let reply: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(reply["result"]["isError"], false);
    let snapshot = human
        .request(RequestId::new(), Command::Snapshot)
        .await
        .unwrap();
    assert_eq!(snapshot["deliveries"][0]["state"], "acknowledged");
    drop(send);
    task.await.unwrap().unwrap();
}
