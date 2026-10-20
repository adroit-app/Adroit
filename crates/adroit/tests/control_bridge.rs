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
    let call = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"acknowledge","arguments":{"request_id":RequestId::new(),"message_id":message.id}}});
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

#[tokio::test]
async fn a_native_stdio_child_uses_the_scoped_launch_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let mut store = Store::open(&root.join("state.sqlite3")).unwrap();
    let session = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Codex,
            dir.path().into(),
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
                title: "Scoped endpoint".into(),
                goal: "Reach only this run".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let credential = store.grant(session.reference()).unwrap();
    drop(store);
    let _service = Service::bind(&root).await.unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_adroit"))
        .args(["control", "mcp"])
        .env("ADROIT_CONTROL_ROOT", &root)
        .env("ADROIT_CONTROL_TOKEN", credential)
        .env("XDG_STATE_HOME", dir.path().join("unrelated-state"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    input.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\"}}\n").await.unwrap();
    let mut line = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        output.read_line(&mut line),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(serde_json::from_str::<Value>(&line).unwrap()["result"].is_object());
    let query = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"snapshot","arguments":{"request_id":RequestId::new()}}});
    input
        .write_all(
            format!("{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}\n{query}\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    line.clear();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        output.read_line(&mut line),
    )
    .await
    .unwrap()
    .unwrap();
    let response: Value = serde_json::from_str(&line).unwrap();
    assert_ne!(response["result"]["isError"], true, "{response}");
    let snapshot: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(snapshot["Ok"]["runs"][0]["id"], run.to_string());
    drop(input);
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}

#[tokio::test]
async fn native_tools_describe_complete_task_and_result_inputs_and_offer_typed_ids() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let mut store = Store::open(&root.join("state.sqlite3")).unwrap();
    let leader = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Codex,
            dir.path().into(),
        )
        .unwrap()
        .value;
    let run_id = RunId::new();
    store
        .open_run(
            RequestId::new(),
            leader.reference(),
            run_id,
            RunBrief {
                title: "Native tools".into(),
                goal: "Delegate through typed tools".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let token = store.grant(leader.reference()).unwrap();
    drop(store);
    let _service = Service::bind(&root).await.unwrap();
    let (input, mut send) = tokio::io::duplex(8192);
    let (output, receive) = tokio::io::duplex(8192);
    let task = tokio::spawn(bridge::run(
        BufReader::new(input),
        output,
        Client::session(&root, token),
        false,
    ));
    let mut receive = BufReader::new(receive);
    let mut line = String::new();
    send.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n")
        .await
        .unwrap();
    receive.read_line(&mut line).await.unwrap();
    send.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n")
        .await
        .unwrap();
    line.clear();
    receive.read_line(&mut line).await.unwrap();
    let reply: Value = serde_json::from_str(&line).unwrap();
    let tools = reply["result"]["tools"].as_array().unwrap();
    let plan = &tools
        .iter()
        .find(|tool| tool["name"] == "put_task")
        .unwrap()["inputSchema"];
    assert!(plan["properties"].get("request_id").is_some());
    assert!(plan["properties"].get("id").is_some());
    assert!(plan["properties"].get("brief").is_some());
    assert!(plan["properties"].get("plan").is_none());
    assert!(plan["properties"].get("command").is_none());
    assert!(
        plan["properties"]["expected_revision"]["description"]
            .as_str()
            .unwrap()
            .contains("new task")
    );
    let report = &tools
        .iter()
        .find(|tool| tool["name"] == "report_task")
        .unwrap()["inputSchema"];
    assert!(report["properties"].get("result").is_some());
    assert!(
        report["$defs"]["TaskResult"]["properties"]
            .get("checks")
            .is_some()
    );
    for tool in tools {
        assert!(serde_json::to_vec(&tool["inputSchema"]).unwrap().len() < 32_768);
        assert!(tool["inputSchema"]["$defs"].get("Command").is_none());
    }
    for op in [
        "put_task",
        "report_task",
        "request_change_completion",
        "propose_candidate",
        "send_message",
        "context",
        "acknowledge",
    ] {
        assert!(tools.iter().any(|tool| tool["name"] == op), "{op}");
    }
    for op in [
        "control",
        "register_leader",
        "remember_launch",
        "grant",
        "confirm_terminal_exit",
        "restart_leader",
        "claim_delivery",
        "accept_delivery",
    ] {
        assert!(!tools.iter().any(|tool| tool["name"] == op), "{op}");
    }
    assert!(tools.iter().any(|tool| tool["name"] == "new_ids"));
    let call = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"new_ids","arguments":{"requests":3,"tasks":2,"messages":1}}});
    send.write_all(format!("{call}\n").as_bytes())
        .await
        .unwrap();
    line.clear();
    receive.read_line(&mut line).await.unwrap();
    let reply: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(reply["result"]["isError"], false);
    let result: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let ids = &result["Ok"];
    assert_eq!(ids["request_ids"].as_array().unwrap().len(), 3);
    assert_eq!(ids["task_ids"].as_array().unwrap().len(), 2);
    assert_eq!(ids["message_ids"].as_array().unwrap().len(), 1);
    let mut unique = std::collections::HashSet::new();
    for values in ids.as_object().unwrap().values() {
        for id in values.as_array().unwrap() {
            let id = id.as_str().unwrap();
            assert!(id.parse::<RequestId>().is_ok());
            assert!(unique.insert(id));
        }
    }
    let create = json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"put_task","arguments":{
        "request_id":ids["request_ids"][0], "id":ids["task_ids"][0], "run_id":run_id,
        "expected_revision":null, "brief":{"instructions":"Classify blue as a color", "purpose":"classify"}, "dependencies":[]
    }}});
    send.write_all(format!("{create}\n").as_bytes())
        .await
        .unwrap();
    line.clear();
    receive.read_line(&mut line).await.unwrap();
    let created: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(created["result"]["isError"], false, "{created}");
    let body: Value =
        serde_json::from_str(created["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(body["Ok"]["value"]["revision"], 1);
    let queue = json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"queue_task","arguments":{
        "request_id":ids["request_ids"][1], "task_id":ids["task_ids"][0], "revision":1
    }}});
    send.write_all(format!("{queue}\n").as_bytes())
        .await
        .unwrap();
    line.clear();
    receive.read_line(&mut line).await.unwrap();
    let queued: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(queued["result"]["isError"], false, "{queued}");
    let body: Value =
        serde_json::from_str(queued["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(body["Ok"]["value"]["state"], "queued");
    drop(send);
    task.await.unwrap().unwrap();
}
