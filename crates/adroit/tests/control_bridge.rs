use adroit::control::{bridge, client::Client, protocol::Command, service::Service, store::Store};
use adroit_model::*;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

async fn exchange(
    send: &mut tokio::io::DuplexStream,
    receive: &mut BufReader<tokio::io::DuplexStream>,
    request: Value,
) -> Value {
    send.write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    let mut line = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        receive.read_line(&mut line),
    )
    .await
    .unwrap()
    .unwrap();
    serde_json::from_str(&line).unwrap()
}

fn legacy_proxy(
    root: &std::path::Path,
    upstream: &std::path::Path,
) -> (
    tokio::task::JoinHandle<()>,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
) {
    use adroit::control::protocol::{Response, VERSION};
    use adroit::control::store::{ControlError, ErrorCode};
    std::fs::create_dir_all(root).unwrap();
    let listener = tokio::net::UnixListener::bind(root.join("control.sock")).unwrap();
    let upstream = upstream.join("control.sock");
    let acknowledgements = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = acknowledgements.clone();
    let task = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let upstream = upstream.clone();
            let observed = observed.clone();
            tokio::spawn(async move {
                let mut stream = BufReader::new(stream);
                let mut line = String::new();
                stream.read_line(&mut line).await.unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                let op = request["command"]["op"].as_str().unwrap();
                let response = if op.starts_with("native_") || op == "resolve_ids" {
                    serde_json::to_string(&Response {
                        version: VERSION,
                        id: None,
                        result: Err(ControlError::new(
                            ErrorCode::InvalidRequest,
                            "invalid control request",
                        )),
                    })
                    .unwrap()
                        + "\n"
                } else {
                    let mut upstream = tokio::net::UnixStream::connect(upstream).await.unwrap();
                    upstream.write_all(line.as_bytes()).await.unwrap();
                    let mut response = String::new();
                    BufReader::new(upstream)
                        .read_line(&mut response)
                        .await
                        .unwrap();
                    response
                };
                if op == "acknowledge" {
                    observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
                stream
                    .get_mut()
                    .write_all(response.as_bytes())
                    .await
                    .unwrap();
            });
        }
    });
    (task, acknowledgements)
}

#[tokio::test]
async fn native_tools_keep_working_with_a_daemon_that_predates_native_projection() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let cwd = dir.path().to_owned();
    let (run, token) = service
        .actor()
        .call(move |store| {
            let leader = store
                .register_leader(
                    RequestId::new(),
                    LogicalSessionId::new(),
                    Agent::Claude,
                    cwd,
                )?
                .value
                .reference();
            let run = RunId::new();
            store.open_run(
                RequestId::new(),
                leader,
                run,
                RunBrief {
                    title: "Legacy".into(),
                    goal: "Keep sessions alive".into(),
                    ..Default::default()
                },
            )?;
            Ok((run, store.grant(leader)?))
        })
        .await
        .unwrap();
    let proxy_root = dir.path().join("legacy");
    let (proxy, _) = legacy_proxy(&proxy_root, &root);
    let (input, mut send) = tokio::io::duplex(8192);
    let (output, receive) = tokio::io::duplex(8192);
    let task = tokio::spawn(bridge::run(
        BufReader::new(input),
        output,
        Client::session(&proxy_root, token),
        false,
    ));
    let mut receive = BufReader::new(receive);
    exchange(
        &mut send,
        &mut receive,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    )
    .await;
    let reply = exchange(&mut send,&mut receive,json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"context","arguments":{}}})).await;
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    let context: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(context["run"]["id"], run.to_string());
    let mut arguments = json!({"id":TaskId::new(),"run_id":run,"expected_revision":null,"brief":{"instructions":"Investigate"},"dependencies":[]});
    let call = |arguments: Value| json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"put_task","arguments":arguments}});
    let reply = exchange(&mut send, &mut receive, call(arguments.clone())).await;
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    let text = reply["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.starts_with(&format!("ok {}.1", arguments["id"].as_str().unwrap())),
        "{text}"
    );
    let retry = text.split("request_id=").nth(1).unwrap();
    arguments["request_id"] = json!(retry);
    let task_id = arguments["id"].clone();
    let reply = exchange(&mut send, &mut receive, call(arguments)).await;
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    let reply = exchange(&mut send,&mut receive,json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"queue_task","arguments":{"task_id":task_id,"revision":1}}})).await;
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    for (id, succeeds) in [(run.to_string(), true), ("r1".to_owned(), false)] {
        let reply = exchange(&mut send,&mut receive,json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"resolve_ids","arguments":{"ids":[id]}}})).await;
        assert_eq!(reply["result"]["isError"], !succeeds, "{reply}");
    }
    assert_eq!(
        service
            .actor()
            .call(|store| store.snapshot())
            .await
            .unwrap()
            .tasks
            .len(),
        1
    );
    drop(send);
    task.await.unwrap().unwrap();
    proxy.abort();
}

#[tokio::test]
async fn codex_mutations_require_the_observed_parent_thread_but_bootstrap_reads_remain_available() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let cwd = dir.path().to_owned();
    let (owner, run, token) = service
        .actor()
        .call(move |store| {
            let owner = store
                .register_leader(
                    RequestId::new(),
                    LogicalSessionId::new(),
                    Agent::Codex,
                    cwd.clone(),
                )?
                .value
                .reference();
            let run = RunId::new();
            store.open_run(
                RequestId::new(),
                owner,
                run,
                RunBrief {
                    title: "Parent request".into(),
                    goal: "Keep child results internal".into(),
                    ..Default::default()
                },
            )?;
            store.attach_session(
                RequestId::new(),
                owner,
                cwd,
                "0123456789abcdef0123456789abcdef".into(),
                String::new(),
            )?;
            Ok((owner, run, store.grant(owner)?))
        })
        .await
        .unwrap();
    let (input, mut send) = tokio::io::duplex(8192);
    let (output, receive) = tokio::io::duplex(8192);
    let task = tokio::spawn(bridge::run(
        BufReader::new(input),
        output,
        Client::session(&root, token),
        false,
    ));
    let mut receive = BufReader::new(receive);
    exchange(
        &mut send,
        &mut receive,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    )
    .await;
    for (name, arguments) in [
        ("new_ids", json!({"requests":1})),
        ("context", json!({"request_id":RequestId::new()})),
    ] {
        let reply = exchange(&mut send, &mut receive, json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":arguments}})).await;
        assert_eq!(reply["result"]["isError"], false, "{reply}");
    }
    let parent = MessageId::new().to_string();
    let mutation = |thread: Option<&str>| {
        let mut request = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"put_task","arguments":{
            "request_id":RequestId::new(),"id":TaskId::new(),"run_id":run,"expected_revision":null,"brief":{"instructions":"Review the request"},"dependencies":[]
        }}});
        if let Some(thread) = thread {
            request["params"]["_meta"] = json!({"threadId":thread});
        }
        request
    };
    let reply = exchange(&mut send, &mut receive, mutation(Some(&parent))).await;
    assert_eq!(
        reply["result"]["isError"], true,
        "unobserved thread must not mutate: {reply}"
    );
    assert!(
        reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("retry")
    );
    let observed = parent.clone();
    service
        .actor()
        .call(move |store| {
            store
                .observe_conversation(
                    RequestId::new(),
                    owner,
                    "0123456789abcdef0123456789abcdef".into(),
                    observed,
                )
                .map(|_| ())
        })
        .await
        .unwrap();
    for thread in [None, Some(MessageId::new().to_string())] {
        let reply = exchange(&mut send, &mut receive, mutation(thread.as_deref())).await;
        assert_eq!(
            reply["result"]["isError"], true,
            "child or unknown origin must not mutate: {reply}"
        );
    }
    assert!(
        service
            .actor()
            .call(|store| store.snapshot())
            .await
            .unwrap()
            .tasks
            .is_empty()
    );
    let reply = exchange(&mut send, &mut receive, mutation(Some(&parent))).await;
    assert_eq!(
        reply["result"]["isError"], false,
        "observed parent must retain control: {reply}"
    );
    assert_eq!(
        service
            .actor()
            .call(|store| store.snapshot())
            .await
            .unwrap()
            .tasks
            .len(),
        1
    );
    drop(send);
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn a_failed_notification_stream_terminates_the_mcp_connection() {
    let dir = tempfile::tempdir().unwrap();
    let (input, mut send) = tokio::io::duplex(8192);
    let (output, receive) = tokio::io::duplex(8192);
    let task = tokio::spawn(bridge::run(
        BufReader::new(input),
        output,
        Client::session(&dir.path().join("absent-control"), "test-session".into()),
        true,
    ));
    let mut receive = BufReader::new(receive);
    send.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n")
        .await
        .unwrap();
    let mut line = String::new();
    receive.read_line(&mut line).await.unwrap();
    send.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .await
        .unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), task)
        .await
        .expect("a failed notification task must close the connection")
        .unwrap();
    assert!(result.is_err());
    line.clear();
    assert_eq!(receive.read_line(&mut line).await.unwrap(), 0);
}

#[tokio::test]
async fn a_failed_notification_stream_closes_a_connection_waiting_for_a_partial_frame() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let cwd = dir.path().to_owned();
    let token = service
        .actor()
        .call(move |store| {
            let identity = store
                .register_leader(
                    RequestId::new(),
                    LogicalSessionId::new(),
                    Agent::Claude,
                    cwd,
                )?
                .value
                .reference();
            store.grant(identity)
        })
        .await
        .unwrap();
    let (input, mut send) = tokio::io::duplex(1);
    let (output, receive) = tokio::io::duplex(8192);
    let task = tokio::spawn(bridge::run(
        BufReader::new(input),
        output,
        Client::session(&root, token),
        true,
    ));
    let mut receive = BufReader::new(receive);
    exchange(
        &mut send,
        &mut receive,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    )
    .await;
    send.write_all(
        b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":",
    )
    .await
    .unwrap();
    drop(service);
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), task)
        .await
        .expect("channel failure must interrupt an unfinished input frame")
        .unwrap();
    assert!(result.is_err());
    let mut line = String::new();
    assert_eq!(receive.read_line(&mut line).await.unwrap(), 0);
}

#[tokio::test]
async fn a_channel_write_records_receipt_without_an_acknowledgement_round_trip() {
    channel_receipt(false).await;
}

#[tokio::test]
async fn legacy_channels_record_receipt_without_an_acknowledgement_model_turn() {
    channel_receipt(true).await;
}

async fn channel_receipt(legacy: bool) {
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
    let proxy_root = dir.path().join("legacy");
    let proxy = legacy.then(|| legacy_proxy(&proxy_root, &root));
    let endpoint = if legacy { &proxy_root } else { &root };
    let (input, mut send) = tokio::io::duplex(8192);
    let (output, receive) = tokio::io::duplex(8192);
    let task = tokio::spawn(bridge::run(
        BufReader::new(input),
        output,
        Client::session(endpoint, token),
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
            if snapshot["deliveries"][0]["state"] == "acknowledged"
                && proxy
                    .as_ref()
                    .is_none_or(|(_, count)| count.load(std::sync::atomic::Ordering::SeqCst) > 0)
            {
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
    if let Some((proxy, _)) = proxy {
        proxy.abort();
    }
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
    assert_eq!(snapshot["runs"][0]["id"], "r1");
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
async fn native_snapshot_compacts_evidence_before_crossing_the_control_socket() {
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
        .value
        .reference();
    let run = RunId::new();
    store
        .open_run(
            RequestId::new(),
            leader,
            run,
            RunBrief {
                title: "Large evidence".into(),
                goal: "Read summaries".into(),
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
                    instructions: "Review".into(),
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
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    for _ in 0..20 {
        store.send_message(RequestId::new(),worker,AgentMessage {
            id:MessageId::new(),run_id:run,from:worker.id,to:leader.id,task_id:Some(task.task_id),attempt_id:Some(attempt.id),revision:Some(1),kind:MessageKind::Report,
            body:json!({"status":"failed","summary":"Review complete","artifact":{"data":"x".repeat(60_000)},"unresolved":[]}).to_string(),reply_to:None,blocks_work:false
        }).unwrap();
    }
    let token = store.grant(leader).unwrap();
    drop(store);
    let _service = Service::bind(&root).await.unwrap();
    let client = Client::session(&root, token);
    assert!(
        client
            .request(RequestId::new(), Command::Snapshot)
            .await
            .is_err()
    );
    let (input, mut send) = tokio::io::duplex(8192);
    let (output, receive) = tokio::io::duplex(8192);
    let task = tokio::spawn(bridge::run(BufReader::new(input), output, client, false));
    let mut receive = BufReader::new(receive);
    exchange(
        &mut send,
        &mut receive,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    )
    .await;
    let reply = exchange(&mut send,&mut receive,json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"snapshot","arguments":{}}})).await;
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    let text = reply["result"]["content"][0]["text"].as_str().unwrap();
    let page: Value = serde_json::from_str(text).unwrap();
    assert_eq!(page["messages"].as_array().unwrap().len(), 20);
    assert!(text.len() < 32_768, "{}", text.len());
    drop(send);
    task.await.unwrap().unwrap();
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
    let conversation = MessageId::new().to_string();
    store
        .attach_session(
            RequestId::new(),
            leader.reference(),
            dir.path().into(),
            "0123456789abcdef0123456789abcdef".into(),
            conversation.clone(),
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
    for kind in ["report", "review"] {
        let artifact = report["$defs"]["TaskArtifact"]["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .find(|variant| variant["properties"]["kind"]["const"] == kind)
            .unwrap();
        let alternatives = artifact["properties"]["data"]["anyOf"].as_array().unwrap();
        assert!(
            alternatives
                .iter()
                .any(|schema| schema["type"] == "object" && schema["additionalProperties"] == true)
        );
        assert!(alternatives.iter().any(|schema| schema["type"] == "array"));
        assert!(alternatives.iter().any(|schema| schema["type"] == "null"));
    }
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
    let create = json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"put_task","_meta":{"threadId":conversation},"arguments":{
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
    let text = created["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("ok t1.1"), "{text}");
    let queue = json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"queue_task","_meta":{"threadId":conversation},"arguments":{
        "task_id":"t1", "revision":1
    }}});
    send.write_all(format!("{queue}\n").as_bytes())
        .await
        .unwrap();
    line.clear();
    receive.read_line(&mut line).await.unwrap();
    let queued: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(queued["result"]["isError"], false, "{queued}");
    let text = queued["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.starts_with("ok t1.1 state=queued request_id="),
        "{text}"
    );
    assert!(
        text.split("request_id=")
            .nth(1)
            .unwrap()
            .parse::<RequestId>()
            .is_ok()
    );
    let failure = exchange(&mut send, &mut receive, json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"queue_task","_meta":{"threadId":conversation},"arguments":{"task_id":"t1","revision":2}}})).await;
    assert_eq!(failure["result"]["isError"], true);
    let text = failure["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("revision_conflict:"), "{text}");
    assert!(
        text.split("request_id=")
            .nth(1)
            .unwrap()
            .parse::<RequestId>()
            .is_ok()
    );
    drop(send);
    task.await.unwrap().unwrap();
}
