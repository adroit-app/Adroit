//! The hosts as a work machine serves them to remote clients: their
//! discovery, replacement, mutations and environments.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::Duration;

mod support {
    pub mod isolated;
}

use support::isolated::{Hosts, Service, isolated, remote_info};

#[test]
fn remote_discovery_refuses_absent_hosts_without_starting_local_ones() {
    let hosts = Hosts::new();
    let output = isolated(hosts.0.path())
        .args(["remote", "info"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!hosts.0.path().join("work.sock").exists());
    assert!(!hosts.0.path().join("term.sock").exists());
}

#[test]
fn remote_mutations_survive_lost_replies_and_retire_old_ids() {
    use adroit_protocol::client::Connection;
    use adroit_protocol::{Answer, Ask, Event, MutationStamp, MutationState, Request, SPOKEN};

    async fn open(socket: &Path) -> Connection {
        let mut link = Connection::open(socket).await.unwrap();
        link.requests
            .send(&Request::Hello {
                versions: SPOKEN.to_vec(),
            })
            .await
            .unwrap();
        loop {
            if matches!(
                link.events.next().await.unwrap(),
                Some(Event::Welcome { .. })
            ) {
                return link;
            }
        }
    }
    async fn ask(link: &mut Connection, ask: Ask) -> Answer {
        link.requests
            .send(&Request::Ask {
                asked: 1,
                ask: Box::new(ask),
            })
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Some(Event::Answer { asked: 1, answer }) = link.events.next().await.unwrap()
                {
                    return answer;
                }
            }
        })
        .await
        .unwrap()
    }
    fn applied(stamp: &MutationStamp, request: &Request) -> Ask {
        Ask::Apply {
            stamp: stamp.clone(),
            request: Box::new(request.clone()),
        }
    }
    let hosts = Hosts::new();
    let _terminal = Service::start(&hosts, "daemon");
    let record = hosts
        .0
        .path()
        .join("state/adroit/instances/headless-test/remote/terminal.json");
    for _ in 0..1000 {
        if record.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let work = Service::start(&hosts, "host");
    let info = remote_info(&hosts);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let old_stamp = runtime.block_on(async {
        let socket = Path::new(info["work"]["socket"].as_str().unwrap());
        let mut link = open(socket).await;
        let stamp = MutationStamp {
            instance: info["instance"].as_str().unwrap().into(),
            work: info["work"]["generation"].as_str().unwrap().into(),
            terminal: info["terminal"]["generation"].as_str().unwrap().into(),
            epoch: link.hello.mutation_epoch.unwrap(), id: format!("{:032x}", 1),
        };
        let start = Request::Ask { asked: 1, ask: Box::new(Ask::StartTerminal(Box::new(adroit_protocol::TerminalStart {
            name: "once".into(), cwd: adroit_protocol::HostPath::local(hosts.0.path()),
            program: adroit_model::TerminalProgram::Shell, file: None, cols: 80, rows: 24,
        }))) };
        // Lose the connection immediately after writing, before reading an answer.
        link.requests.send(&Request::Ask { asked: 1, ask: Box::new(applied(&stamp, &start)) }).await.unwrap();
        drop(link);
        let mut link = open(socket).await;
        let result = loop {
            let result = ask(&mut link, applied(&stamp, &start)).await;
            if !matches!(result, Answer::Mutation { status: MutationState::Accepted }) { break result; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        assert!(matches!(result, Answer::Mutation { status: MutationState::Result { answer } } if matches!(*answer, Answer::Terminal { .. })));
        let changed = Request::Release { id: 0 };
        assert_eq!(ask(&mut link, applied(&stamp, &changed)).await, Answer::Mutation { status: MutationState::Conflict });
        let daemon = adroit_term::Daemon::new(info["terminal"]["socket"].as_str().unwrap());
        assert_eq!(daemon.sessions().await.unwrap().len(), 1);
        for i in 2..4200 {
            let epoch = match ask(&mut link, Ask::MutationEpoch).await { Answer::MutationEpoch { epoch } => epoch, other => panic!("{other:?}") };
            let current = MutationStamp { epoch, id: format!("{i:032x}"), ..stamp.clone() };
            assert!(matches!(ask(&mut link, applied(&current, &changed)).await, Answer::Mutation { status: MutationState::Result { .. } }));
        }
        assert_eq!(ask(&mut link, applied(&stamp, &start)).await, Answer::Mutation { status: MutationState::Retired });
        assert_eq!(ask(&mut link, Ask::MutationStatus { stamp: stamp.clone() }).await, Answer::Mutation { status: MutationState::Unknown });
        assert_eq!(daemon.sessions().await.unwrap().len(), 1);
        stamp
    });
    drop(work);
    let _replacement = Service::start(&hosts, "host");
    let info = remote_info(&hosts);
    runtime.block_on(async {
        let mut link = open(Path::new(info["work"]["socket"].as_str().unwrap())).await;
        assert_eq!(
            ask(&mut link, Ask::MutationStatus { stamp: old_stamp }).await,
            Answer::Mutation {
                status: MutationState::Unknown
            }
        );
    });
}

#[test]
fn remote_work_host_replacement_preserves_terminal_generation_and_invalidates_the_old_work_socket()
{
    let hosts = Hosts::new();
    let _terminal = Service::start(&hosts, "daemon");
    let terminal_record = hosts
        .0
        .path()
        .join("state/adroit/instances/headless-test/remote/terminal.json");
    for _ in 0..1000 {
        if terminal_record.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let work = Service::start(&hosts, "host");
    let first = remote_info(&hosts);
    let terminal = isolated(hosts.0.path())
        .env("ADROIT_REMOTE", "1")
        .args([
            "terminal",
            "start",
            "--program",
            "command",
            "--json",
            "--",
            "/bin/sh",
            "-c",
            "printf remote-alive; read line",
        ])
        .output()
        .unwrap();
    assert!(
        terminal.status.success(),
        "{}",
        String::from_utf8_lossy(&terminal.stderr)
    );
    let terminal: serde_json::Value = serde_json::from_slice(&terminal.stdout).unwrap();
    drop(work);
    let _replacement = Service::start(&hosts, "host");
    let second = remote_info(&hosts);
    assert_eq!(first["instance"], second["instance"]);
    assert_eq!(first["terminal"], second["terminal"]);
    assert_ne!(first["work"]["generation"], second["work"]["generation"]);
    let old = first["work"]["socket"].as_str().unwrap();
    assert!(std::os::unix::net::UnixStream::connect(old).is_err());
    let listed = isolated(hosts.0.path())
        .env("ADROIT_REMOTE", "1")
        .args(["terminal", "list", "--json"])
        .output()
        .unwrap();
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    assert!(
        String::from_utf8_lossy(&listed.stdout).contains(terminal["resource_id"].as_str().unwrap())
    );
}

#[test]
fn remote_jobs_cannot_change_another_environments_terminal_by_runtime_id() {
    use adroit_protocol::{Answer, Ask, Event, MutationStamp, MutationState, Request, SPOKEN};
    let hosts = Hosts::new();
    let _terminal = Service::start(&hosts, "daemon");
    let record = hosts
        .0
        .path()
        .join("state/adroit/instances/headless-test/remote/terminal.json");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Ok(text) = std::fs::read_to_string(&record)
                    && let Ok(record) = serde_json::from_str::<serde_json::Value>(&text)
                    && let Some(socket) = record["socket"].as_str()
                    && adroit_term::Daemon::new(socket).sessions().await.is_ok()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
    });
    let _work = Service::start(&hosts, "host");
    let info = remote_info(&hosts);
    runtime.block_on(async {
        let daemon = adroit_term::Daemon::new(info["terminal"]["socket"].as_str().unwrap());
        let id = daemon.spawn(adroit_term::Spawn {
            program: "/bin/sh".into(), args: vec!["-c".into(), "read answer".into()],
            cwd: hosts.0.path().into(),
            tags: [(adroit_host::environment::TAG, "personal"), ("adroit.kind", "codex"), ("adroit.name", "personal"), ("adroit.cwd", hosts.0.path().to_str().unwrap())].into_iter().flat_map(|(key, value)| adroit_term::tags::tagged(key, value)).collect(),
            ..Default::default()
        }).await.unwrap();
        let session = adroit_model::Session { id: format!("{id:032x}"), ..Default::default() };
        let jobs = [
            Request::Release { id },
            Request::Rename { id: session.id.clone(), name: "foreign".into(), was: "personal".into(), agent: adroit_model::Agent::Codex },
            Request::Kill { session: Box::new(session.clone()), process: adroit_model::Process::default() },
            Request::Stop { session: Box::new(session) },
            Request::EndTerminal { id },
        ];
        for (index, job) in jobs.into_iter().enumerate() {
            let mut link = adroit_protocol::client::Connection::open_in(Path::new(info["work"]["socket"].as_str().unwrap()), Some("work")).await.unwrap();
            let epoch = link.hello.mutation_epoch.unwrap();
            link.requests.send(&Request::Hello { versions: SPOKEN.to_vec() }).await.unwrap();
            let stamp = MutationStamp {
                instance: info["instance"].as_str().unwrap().into(),
                work: info["work"]["generation"].as_str().unwrap().into(),
                terminal: info["terminal"]["generation"].as_str().unwrap().into(),
                epoch, id: format!("{:032x}", index + 1),
            };
            link.requests.send(&Request::Ask { asked: 99, ask: Box::new(Ask::Apply { stamp, request: Box::new(job) }) }).await.unwrap();
            let answer = tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    if let Some(Event::Answer { asked: 99, answer }) = link.events.next().await.unwrap() { break answer; }
                }
            }).await.unwrap();
            let shown = format!("{answer:?}");
            assert!(matches!(answer, Answer::Mutation { status: MutationState::Result { answer } } if matches!(*answer, Answer::Failed { ref text } if text == "the session belongs to another working environment")), "job {index} was not refused by ownership: {shown}");
            let listed = daemon.sessions().await.unwrap();
            assert!(listed.iter().any(|s| s.id == id && !s.exited));
            assert!(!listed.iter().any(|s| s.tags.contains(&"adroit.name=foreign".to_string())));
        }
        daemon.end(id).await.unwrap();
    });
}

#[test]
fn a_work_host_started_before_its_terminal_host_waits_for_it() {
    let hosts = Hosts::new();
    let _work = Service::start(&hosts, "host");
    std::thread::sleep(Duration::from_millis(500));
    let _terminal = Service::start(&hosts, "daemon");
    let info = remote_info(&hosts);
    assert_eq!(info["work"]["instance"], info["terminal"]["instance"]);
}

#[test]
fn a_work_host_lists_its_environments_with_the_names_windows_called_them_and_the_sessions_in_each()
{
    use adroit_protocol::{Request, SPOKEN, ServedEnvironment};
    let hosts = Hosts::new();
    let _terminal = Service::start(&hosts, "daemon");
    let _work = Service::start(&hosts, "host");
    let info = remote_info(&hosts);
    let socket = PathBuf::from(info["work"]["socket"].as_str().unwrap());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let daemon = adroit_term::Daemon::new(info["terminal"]["socket"].as_str().unwrap());
        daemon
            .spawn(adroit_term::Spawn {
                program: "/bin/sh".into(),
                args: vec!["-c".into(), "read answer".into()],
                cwd: hosts.0.path().into(),
                tags: adroit_term::tags::tagged(adroit_host::environment::TAG, "work"),
                ..Default::default()
            })
            .await
            .unwrap();
        for (scope, name) in [(None, "Personal"), (Some("work"), "Work box")] {
            let mut link = adroit_protocol::client::Connection::open_in(&socket, scope)
                .await
                .unwrap();
            link.requests
                .send(&Request::Hello {
                    versions: SPOKEN.to_vec(),
                })
                .await
                .unwrap();
            link.requests
                .send(&Request::Call { name: name.into() })
                .await
                .unwrap();
            // A request answered after it means the host took the name.
            link.requests.send(&Request::Snapshot).await.unwrap();
            link.events.next().await.unwrap();
        }
        let listed = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let listed = adroit_protocol::client::environments(&socket)
                    .await
                    .unwrap();
                if listed.iter().all(|e| e.name.is_some()) {
                    break listed;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            listed,
            [
                ServedEnvironment {
                    id: None,
                    name: Some("Personal".into()),
                    running: 0,
                },
                ServedEnvironment {
                    id: Some("work".into()),
                    name: Some("Work box".into()),
                    running: 1,
                },
            ]
        );
    });
}

#[test]
fn a_work_host_forgets_an_environment_only_where_no_session_runs_and_no_window_worked() {
    use adroit_protocol::client::{environments, forget_environment};
    let hosts = Hosts::new();
    let _terminal = Service::start(&hosts, "daemon");
    let _work = Service::start(&hosts, "host");
    let info = remote_info(&hosts);
    let socket = PathBuf::from(info["work"]["socket"].as_str().unwrap());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let daemon = adroit_term::Daemon::new(info["terminal"]["socket"].as_str().unwrap());
        daemon
            .spawn(adroit_term::Spawn {
                program: "/bin/sh".into(),
                args: vec!["-c".into(), "read answer".into()],
                cwd: hosts.0.path().into(),
                tags: adroit_term::tags::tagged(adroit_host::environment::TAG, "busy"),
                ..Default::default()
            })
            .await
            .unwrap();
        let opened = adroit_protocol::client::Connection::open_in(&socket, Some("opened"))
            .await
            .unwrap();
        let folders = folder_below(hosts.0.path(), "opened");
        std::fs::create_dir_all(folders.with_file_name("idle")).unwrap();
        std::fs::create_dir_all(folders.with_file_name("busy")).unwrap();
        let ids = |listed: Vec<adroit_protocol::ServedEnvironment>| {
            listed.into_iter().filter_map(|e| e.id).collect::<Vec<_>>()
        };
        assert_eq!(
            ids(environments(&socket).await.unwrap()),
            ["busy", "idle", "opened"]
        );
        assert!(forget_environment(&socket, "busy").await.is_err());
        assert!(forget_environment(&socket, "opened").await.is_err());
        assert!(forget_environment(&socket, "nowhere").await.is_err());
        forget_environment(&socket, "idle").await.unwrap();
        assert_eq!(
            ids(environments(&socket).await.unwrap()),
            ["busy", "opened"]
        );
        drop(opened);
    });
}

/// The folder named `name` somewhere below `dir`.
fn folder_below(dir: &Path, name: &str) -> PathBuf {
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                if entry.file_name() == name {
                    return path;
                }
                pending.push(path);
            }
        }
    }
    panic!("no folder {name} below {}", dir.display())
}

#[test]
fn the_hosts_a_window_on_the_work_machine_started_are_the_ones_a_paired_client_reaches() {
    let hosts = Hosts::new();
    let started = isolated(hosts.0.path())
        .args([
            "terminal",
            "start",
            "--program",
            "command",
            "--name",
            "at-the-machine",
            "--dir",
            hosts.0.path().to_str().unwrap(),
            "--json",
            "--",
            "/bin/sh",
            "-c",
            "printf here; read answer",
        ])
        .output()
        .unwrap();
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let info = remote_info(&hosts);
    assert_eq!(
        std::fs::read_link(info["terminal"]["socket"].as_str().unwrap()).unwrap(),
        hosts.0.path().join("term.sock")
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let remote = Path::new(info["work"]["socket"].as_str().unwrap());
        let listed = adroit_protocol::client::environments(remote).await.unwrap();
        assert_eq!(listed[0].running, 1);
        let paired = adroit_protocol::client::Connection::open_unscoped(remote)
            .await
            .unwrap();
        assert!(paired.hello.endpoint.is_some());
        assert!(paired.hello.mutation_epoch.is_some());
        let local =
            adroit_protocol::client::Connection::open_unscoped(&hosts.0.path().join("work.sock"))
                .await
                .unwrap();
        assert!(local.hello.endpoint.is_none());
        assert!(local.hello.mutation_epoch.is_none());
    });
}
