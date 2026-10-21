#[path = "support/daemon.rs"]
mod native_daemon;
use adroit::control::{
    client::{AppClient, Client},
    protocol::Command,
    service::Service,
};
use adroit_model::{Agent, LogicalSessionId, RequestId, RunBrief, RunId};
use serde_json::json;

#[test]
fn the_session_cli_cannot_create_a_leader_after_agent_markers_are_removed() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let workspace = dir.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(workspace.join(".git")).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_adroit"))
        .args(["session", "start", "--dir"])
        .arg(&workspace)
        .args(["--", "Launch without approval"])
        .env_remove("ADROIT_CONTROL_TOKEN")
        .env_remove("ADROIT_CONTROL_ROOT")
        .env("XDG_STATE_HOME", dir.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Adroit window"));
    assert!(!dir.path().join("adroit/control/state.sqlite3").exists());
}

#[tokio::test]
async fn a_readable_human_token_cannot_compose_an_independent_leader_launch() {
    use adroit::control::runtime::RuntimeLaunch;
    use adroit_model::{AgentPermissions, SessionRef};
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let root = dir.path().join("control");
    let mut service = Service::bind(&root).await.unwrap();
    let stolen = Client::session(
        &root,
        std::fs::read_to_string(root.join("human.token")).unwrap(),
    );
    let id = LogicalSessionId::new();
    let first = SessionRef { id, generation: 1 };
    let run = RunId::new();
    let registration = stolen
        .request(
            RequestId::new(),
            Command::RegisterLeader {
                id,
                agent: Agent::Codex,
                cwd: dir.path().into(),
                permissions: AgentPermissions::Default,
            },
        )
        .await;
    if registration.is_ok() {
        let token = stolen
            .request(RequestId::new(), Command::Grant { identity: first })
            .await
            .unwrap();
        let agent = Client::session(&root, token.as_str().unwrap().into());
        agent
            .request(
                RequestId::new(),
                Command::OpenRun {
                    id: run,
                    brief: RunBrief {
                        title: "Injected".into(),
                        goal: "Launch without a window action".into(),
                        ..Default::default()
                    },
                },
            )
            .await
            .unwrap();
        stolen
            .request(
                RequestId::new(),
                Command::RememberLaunch {
                    identity: first,
                    runtime: Box::new(RuntimeLaunch::new(Default::default())),
                },
            )
            .await
            .unwrap();
        stolen
            .request(
                RequestId::new(),
                Command::FailStart {
                    identity: first,
                    reason: "force restart".into(),
                },
            )
            .await
            .unwrap();
        stolen
            .request(RequestId::new(), Command::RestartLeader { identity: first })
            .await
            .unwrap();
    }
    assert!(
        registration.is_err(),
        "human.token must not register a Leader"
    );
    for command in [
        Command::Grant { identity: first },
        Command::RememberLaunch {
            identity: first,
            runtime: Box::new(RuntimeLaunch::new(Default::default())),
        },
        Command::FailStart {
            identity: first,
            reason: "force restart".into(),
        },
        Command::RestartLeader { identity: first },
        Command::OpenRun {
            id: run,
            brief: RunBrief {
                title: "Injected".into(),
                goal: "Launch".into(),
                ..Default::default()
            },
        },
        Command::NativeCall {
            name: "register_leader".into(),
            arguments: json!({
                "id": LogicalSessionId::new(), "agent": "codex", "cwd": dir.path(), "permissions": "default"
            }),
        },
        Command::NativeEnvelope {
            message_id: adroit_model::MessageId::new(),
        },
    ] {
        let error = stolen
            .clone()
            .as_actor(first)
            .request(RequestId::new(), command)
            .await
            .unwrap_err();
        assert!(error.message.contains("read-only"), "{error}");
    }
    let snapshot = service
        .actor()
        .call(|store| store.snapshot())
        .await
        .unwrap();
    assert!(snapshot.sessions.is_empty());
    assert!(snapshot.runs.is_empty());
    assert!(
        service
            .actor()
            .call(|store| store.pending_independent_leaders())
            .await
            .unwrap()
            .is_empty()
    );
    let socket = dir.path().join("native.sock");
    let native = native_daemon::serve_process(socket.clone());
    let daemon = adroit_term::Daemon::new(socket);
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while daemon.sessions().await.is_err() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    service
        .supervise(
            daemon.clone(),
            "/adroit".into(),
            adroit::settings::Settings::default,
        )
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(
        daemon.sessions().await.unwrap().is_empty(),
        "the rejected command sequence must not launch a native session"
    );
    native.abort();
}

#[tokio::test]
async fn only_a_human_can_authorize_and_only_the_bound_leader_can_create() {
    use adroit_model::{
        AgentPermissions, ManagedSession, SessionRef, TargetSelection, TaskBrief, TaskId, TaskPlan,
        TaskVersion,
    };
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let human = Client::human(&root).unwrap();
    let app = AppClient::new(&root);
    let mut leaders = Vec::new();
    for _ in 0..2 {
        let record: ManagedSession = serde_json::from_value(
            app.request(
                RequestId::new(),
                Command::RegisterLeader {
                    id: LogicalSessionId::new(),
                    agent: Agent::Claude,
                    cwd: dir.path().into(),
                    permissions: AgentPermissions::Default,
                },
            )
            .await
            .unwrap()["value"]
                .clone(),
        )
        .unwrap();
        let identity = record.reference();
        let token: String = serde_json::from_value(
            app.request(RequestId::new(), Command::Grant { identity })
                .await
                .unwrap(),
        )
        .unwrap();
        let client = Client::session(&root, token);
        app.clone()
            .as_actor(identity)
            .request(
                RequestId::new(),
                Command::OpenRun {
                    id: RunId::new(),
                    brief: RunBrief {
                        title: "Existing".into(),
                        goal: "Existing work".into(),
                        ..Default::default()
                    },
                },
            )
            .await
            .unwrap();
        leaders.push((identity, client));
    }
    let brief = RunBrief {
        title: "Independent".into(),
        goal: "Human requested work".into(),
        ..Default::default()
    };
    assert!(
        human
            .request(
                RequestId::new(),
                Command::AuthorizeIndependentRun {
                    leader: leaders[0].0,
                    workspace: dir.path().into(),
                    brief: brief.clone(),
                    agent: Agent::Claude,
                    model: None,
                    permissions: AgentPermissions::Default,
                }
            )
            .await
            .is_err(),
        "reading the human token must not mint an independent-run grant"
    );
    assert!(
        leaders[0]
            .1
            .request(
                RequestId::new(),
                Command::AuthorizeIndependentRun {
                    leader: leaders[0].0,
                    workspace: dir.path().into(),
                    brief: brief.clone(),
                    agent: Agent::Claude,
                    model: None,
                    permissions: AgentPermissions::Default
                }
            )
            .await
            .is_err()
    );
    assert!(
        leaders[0]
            .1
            .request(
                RequestId::new(),
                Command::RegisterLeader {
                    id: LogicalSessionId::new(),
                    agent: Agent::Claude,
                    cwd: dir.path().into(),
                    permissions: AgentPermissions::Default
                }
            )
            .await
            .is_err()
    );
    let grant = adroit::control::ui_authorization::authorize(
        &root,
        RequestId::new(),
        adroit::control::store::IndependentAuthorizationPlan {
            leader: leaders[0].0,
            workspace: dir.path().into(),
            brief,
            agent: Agent::Claude,
            model: None,
            permissions: AgentPermissions::Default,
        },
    )
    .unwrap();
    let id = grant.id;
    let workspace_plan = |workspace| adroit::control::store::IndependentAuthorizationPlan {
        leader: leaders[0].0,
        workspace,
        brief: RunBrief {
            title: "Path check".into(),
            goal: "Check the selected workspace".into(),
            ..Default::default()
        },
        agent: Agent::Claude,
        model: None,
        permissions: AgentPermissions::Default,
    };
    let missing = dir.path().join("missing");
    assert!(
        adroit::control::ui_authorization::authorize(
            &root,
            RequestId::new(),
            workspace_plan(missing)
        )
        .is_err()
    );
    let file = dir.path().join("file");
    std::fs::write(&file, "contents").unwrap();
    assert!(
        adroit::control::ui_authorization::authorize(&root, RequestId::new(), workspace_plan(file))
            .is_err()
    );
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(dir.path(), &link).unwrap();
    assert!(
        adroit::control::ui_authorization::authorize(&root, RequestId::new(), workspace_plan(link))
            .is_err()
    );
    let replaceable = dir.path().join("replaceable");
    std::fs::create_dir(&replaceable).unwrap();
    let replacement = adroit::control::ui_authorization::authorize(
        &root,
        RequestId::new(),
        workspace_plan(replaceable.clone()),
    )
    .unwrap();
    std::fs::rename(&replaceable, dir.path().join("old-workspace")).unwrap();
    std::fs::create_dir(&replaceable).unwrap();
    assert!(
        leaders[0]
            .1
            .request(
                RequestId::new(),
                Command::CreateIndependentRun {
                    authorization_id: replacement.id
                }
            )
            .await
            .is_err()
    );
    assert!(
        leaders[1]
            .1
            .request(
                RequestId::new(),
                Command::CreateIndependentRun {
                    authorization_id: id
                }
            )
            .await
            .is_err()
    );
    let first = leaders[0].0;
    let worker = service
        .actor()
        .call(move |store| {
            let run = store
                .snapshot()
                .unwrap()
                .runs
                .into_iter()
                .find(|run| run.leader_id == first.id)
                .unwrap();
            let plan = TaskPlan {
                id: TaskId::new(),
                run_id: run.id,
                expected_revision: None,
                brief: TaskBrief {
                    instructions: "Worker".into(),
                    ..Default::default()
                },
                dependencies: vec![],
            };
            let task = store.put_task(RequestId::new(), first, plan).unwrap().value;
            let version = TaskVersion {
                task_id: task.id,
                revision: task.revision,
            };
            store.queue_task(RequestId::new(), first, version).unwrap();
            let attempt = store
                .reserve_attempt(RequestId::new(), first, version, TargetSelection::default())
                .unwrap()
                .value;
            let identity = SessionRef {
                id: attempt.session_id,
                generation: 1,
            };
            Ok((identity, store.grant(identity).unwrap()))
        })
        .await
        .unwrap();
    let worker_client = Client::session(&root, worker.1);
    assert!(
        worker_client
            .request(
                RequestId::new(),
                Command::CreateIndependentRun {
                    authorization_id: id
                }
            )
            .await
            .is_err()
    );
    let created = leaders[0]
        .1
        .request(
            RequestId::new(),
            Command::CreateIndependentRun {
                authorization_id: id,
            },
        )
        .await
        .unwrap();
    assert_ne!(created["value"]["leader_id"], json!(leaders[0].0.id));
}

#[tokio::test]
async fn native_compatibility_requires_an_explicit_unknown_command_response() {
    use adroit::control::{
        protocol::{Request, Response, VERSION},
        store::{ControlError, ErrorCode},
    };
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    for (version, id_kind, code, message, expected) in [
        (
            VERSION,
            "none",
            ErrorCode::InvalidRequest,
            "invalid control request",
            "unsupported native projection",
        ),
        (
            VERSION,
            "wrong",
            ErrorCode::InvalidRequest,
            "invalid control request",
            "the response does not match the request",
        ),
        (
            VERSION + 1,
            "none",
            ErrorCode::InvalidRequest,
            "invalid control request",
            "the response does not match the request",
        ),
        (
            VERSION,
            "none",
            ErrorCode::OwnershipMismatch,
            "invalid control request",
            "the response does not match the request",
        ),
        (
            VERSION,
            "none",
            ErrorCode::InvalidRequest,
            "credential rejected",
            "the response does not match the request",
        ),
        (
            VERSION,
            "same",
            ErrorCode::OwnershipMismatch,
            "credential rejected",
            "credential rejected",
        ),
        (
            VERSION,
            "same",
            ErrorCode::InvalidRequest,
            "invalid control request",
            "invalid control request",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let listener = tokio::net::UnixListener::bind(dir.path().join("control.sock")).unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(stream);
            let mut line = String::new();
            stream.read_line(&mut line).await.unwrap();
            let request: Request = serde_json::from_str(&line).unwrap();
            let response = Response {
                version,
                id: match id_kind {
                    "same" => Some(request.id),
                    "wrong" => Some(RequestId::new()),
                    _ => None,
                },
                result: Err(ControlError::new(code, message)),
            };
            stream
                .get_mut()
                .write_all((serde_json::to_string(&response).unwrap() + "\n").as_bytes())
                .await
                .unwrap();
        });
        let error = Client::session(dir.path(), "credential".into())
            .request(
                RequestId::new(),
                Command::NativeArguments {
                    name: "context".into(),
                    arguments: json!({}),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.message, expected);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn requests_survive_reconnection_and_daemon_restart_without_a_window() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let client = Client::human(&root).unwrap();
    let app = AppClient::new(&root);
    let id = LogicalSessionId::new();
    let request = RequestId::new();
    let command = Command::RegisterLeader {
        permissions: adroit_model::AgentPermissions::Default,
        id,
        agent: Agent::Codex,
        cwd: dir.path().into(),
    };
    let first = app.request(request, command.clone()).await.unwrap();
    drop(service);
    let _service = Service::bind(&root).await.unwrap();
    let replay = AppClient::new(&root)
        .request(request, command)
        .await
        .unwrap();
    assert_eq!(first, replay);
    let events = client
        .request(
            RequestId::new(),
            Command::Events {
                after: 0,
                limit: 100,
            },
        )
        .await
        .unwrap();
    assert_eq!(events["events"].as_array().unwrap().len(), 1);
    let events = client
        .request(
            RequestId::new(),
            Command::Events {
                after: 1,
                limit: 100,
            },
        )
        .await
        .unwrap();
    assert_eq!(events, json!({"events": [], "next_cursor": 1}));
}

#[tokio::test]
async fn credentials_determine_the_actor_and_cannot_grant_another_identity() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let _service = Service::bind(&root).await.unwrap();
    let app = AppClient::new(&root);
    let mut actors = Vec::new();
    let mut first_identity = None;
    for _ in 0..2 {
        let id = LogicalSessionId::new();
        let reply = app
            .request(
                RequestId::new(),
                Command::RegisterLeader {
                    permissions: adroit_model::AgentPermissions::Default,
                    id,
                    agent: Agent::Claude,
                    cwd: dir.path().into(),
                },
            )
            .await
            .unwrap();
        let identity = serde_json::from_value(reply["value"].clone())
            .map(|s: adroit_model::ManagedSession| s.reference())
            .unwrap();
        first_identity.get_or_insert(identity);
        let grant = app
            .request(RequestId::new(), Command::Grant { identity })
            .await
            .unwrap();
        actors.push(Client::session(&root, grant.as_str().unwrap().into()));
    }
    let run = RunId::new();
    app.as_actor(first_identity.unwrap())
        .request(
            RequestId::new(),
            Command::OpenRun {
                id: run,
                brief: RunBrief {
                    title: "Review".into(),
                    goal: "Review the change".into(),
                    ..Default::default()
                },
            },
        )
        .await
        .unwrap();
    assert!(
        actors[1]
            .request(
                RequestId::new(),
                Command::OpenRun {
                    id: RunId::new(),
                    brief: RunBrief {
                        title: "Another".into(),
                        goal: "Unapproved run".into(),
                        ..Default::default()
                    },
                }
            )
            .await
            .is_err()
    );
    assert!(
        actors[1]
            .request(
                RequestId::new(),
                Command::Grant {
                    identity: adroit_model::SessionRef {
                        id: LogicalSessionId::new(),
                        generation: 1
                    }
                }
            )
            .await
            .is_err()
    );
    assert!(
        actors[1]
            .request(RequestId::new(), Command::Snapshot)
            .await
            .is_err()
    );
    let snapshot = actors[0]
        .request(RequestId::new(), Command::Snapshot)
        .await
        .unwrap();
    assert_eq!(snapshot["runs"].as_array().unwrap().len(), 1);
    assert!(
        Client::session(&root, "incorrect".into())
            .request(RequestId::new(), Command::Snapshot)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn slow_and_oversized_clients_do_not_hold_up_other_requests() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let _service = Service::bind(&root).await.unwrap();
    let _idle = tokio::net::UnixStream::connect(root.join("control.sock"))
        .await
        .unwrap();
    let mut large = tokio::net::UnixStream::connect(root.join("control.sock"))
        .await
        .unwrap();
    large.write_all(&vec![b'x'; 1_048_577]).await.unwrap();
    let mut line = String::new();
    tokio::io::BufReader::new(large)
        .read_line(&mut line)
        .await
        .unwrap();
    assert!(line.contains("invalid_request"));
    let snapshot = Client::human(&root)
        .unwrap()
        .request(RequestId::new(), Command::Snapshot)
        .await
        .unwrap();
    assert_eq!(snapshot["event_cursor"], 0);
    assert!(Service::bind(&root).await.is_err());
}

#[tokio::test]
async fn a_session_grant_survives_restart_and_cannot_select_another_actor() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let app = AppClient::new(&root);
    let reply = app
        .request(
            RequestId::new(),
            Command::RegisterLeader {
                permissions: adroit_model::AgentPermissions::Default,
                id: LogicalSessionId::new(),
                agent: Agent::Codex,
                cwd: dir.path().into(),
            },
        )
        .await
        .unwrap();
    let session: adroit_model::ManagedSession =
        serde_json::from_value(reply["value"].clone()).unwrap();
    let grant = app
        .request(
            RequestId::new(),
            Command::Grant {
                identity: session.reference(),
            },
        )
        .await
        .unwrap();
    let agent = Client::session(&root, grant.as_str().unwrap().into());
    drop(service);
    let _service = Service::bind(&root).await.unwrap();
    AppClient::new(&root)
        .as_actor(session.reference())
        .request(
            RequestId::new(),
            Command::OpenRun {
                id: RunId::new(),
                brief: RunBrief {
                    title: "Check".into(),
                    goal: "Check the code".into(),
                    ..Default::default()
                },
            },
        )
        .await
        .unwrap();
    agent
        .request(RequestId::new(), Command::Snapshot)
        .await
        .unwrap();
    assert!(
        agent
            .as_actor(session.reference())
            .request(RequestId::new(), Command::Snapshot)
            .await
            .is_err()
    );
}

#[test]
fn upgrading_an_existing_store_keeps_records_and_adds_session_credentials() {
    use adroit::control::store::Store;
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
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE independent_authorizations; DROP TABLE native_progress; DROP TABLE native_aliases; DROP TABLE completion_requests; DROP TABLE candidate_checks; DROP TABLE candidates; DROP TABLE coordination_replies; DROP TABLE coordination_notices; DROP TABLE run_completions; DROP TABLE session_launches; DROP TABLE task_reports; DROP TABLE run_work_areas; DROP TABLE work_areas; DROP TABLE session_credentials; PRAGMA user_version=1;")
        .unwrap();
    drop(connection);
    let mut store = Store::open(&path).unwrap();
    assert_eq!(store.snapshot().unwrap().sessions, vec![session.clone()]);
    let token = store.grant(session.reference()).unwrap();
    assert_eq!(store.authenticate(&token).unwrap(), session.reference());
}

#[tokio::test]
async fn the_installed_control_command_refuses_a_missing_managed_credential() {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    let dir = tempfile::Builder::new()
        .prefix("adroit-control-")
        .tempdir_in("/tmp")
        .unwrap();
    let root = dir.path().join("adroit/control");
    let mut daemon = tokio::process::Command::new(env!("CARGO_BIN_EXE_adroit"))
        .arg("daemon")
        .env("XDG_STATE_HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path().join("config"))
        .env("ADROIT_INSTANCE", "default")
        .env("ADROIT_DAEMON", dir.path().join("felis/daemon.sock"))
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !root.join("control.sock").exists() {
            assert!(daemon.try_wait().unwrap().is_none());
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let request = RequestId::new();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_adroit"))
        .args(["control", "--request-id", &request.to_string()])
        .env("XDG_STATE_HOME", dir.path())
        .env("ADROIT_INSTANCE", "default")
        .env_remove("ADROIT_CONTROL_TOKEN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    command
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"op":"snapshot"}"#)
        .await
        .unwrap();
    let output = command.wait_with_output().await.unwrap();
    assert!(!output.status.success());
    let reply: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reply["request_id"], request.to_string());
    assert!(
        reply["result"]["Err"]["message"]
            .as_str()
            .unwrap()
            .contains("managed session credential")
    );
    daemon.kill().await.unwrap();
    daemon.wait().await.unwrap();
}
