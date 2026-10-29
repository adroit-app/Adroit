//! The executable installed on a work machine, built without the window.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[test]
fn a_headless_build_does_not_link_the_window_or_its_desktop_runtime() {
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--offline",
            "--manifest-path",
            concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
            "--no-default-features",
            "--edges",
            "normal",
            "--prefix",
            "none",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8(output.stdout).unwrap();
    assert!(!tree.contains("adroit-window"), "{tree}");
    assert!(!tree.contains("dioxus"), "{tree}");
    assert!(!tree.contains("wry "), "{tree}");
}

#[test]
fn the_headless_executable_reports_its_version_without_starting_a_host() {
    let dir = tempfile::tempdir().unwrap();
    let output = isolated(dir.path()).arg("--version").output().unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("adroit ")
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[cfg(not(feature = "desktop"))]
#[test]
fn a_headless_build_refuses_window_and_editor_requests_without_starting_hosts() {
    for args in [
        vec![],
        vec!["--new-window"],
        vec!["editor", "/tmp/prompt.md"],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let output = isolated(dir.path()).args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("no window")
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}

/// A process whose hosts and data belong to this test alone.
fn isolated(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_adroit"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("ADROIT_") || name == "CODEX_THREAD_ID" {
            command.env_remove(name);
        }
    }
    command
        .env("XDG_STATE_HOME", dir.join("state"))
        .env("XDG_DATA_HOME", dir.join("data"))
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("ADROIT_HOST", dir.join("work.sock"))
        .env("ADROIT_DAEMON", dir.join("term.sock"))
        .env("ADROIT_INSTANCE", "headless-test");
    command
}

struct Hosts(tempfile::TempDir);

impl Hosts {
    fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("adroit-h-")
            .tempdir_in("/tmp")
            .unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        Self(dir)
    }

    fn run(&self, args: &[&str]) -> serde_json::Value {
        let output = isolated(self.0.path()).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

impl Drop for Hosts {
    fn drop(&mut self) {
        // The terminal host outlives the work host, including a failed test.
        for command in ["daemon", "host"] {
            let _ = isolated(self.0.path())
                .env("ADROIT_WORK_INSTANCE", "headless-test")
                .args([command, "stop"])
                .output();
        }
    }
}

struct Service(std::process::Child);

impl Service {
    fn terminal(hosts: &Hosts) -> Self {
        use std::process::Stdio;
        Self(
            isolated(hosts.0.path())
                .arg("daemon")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }
    fn start(hosts: &Hosts, role: &str) -> Self {
        use std::process::Stdio;
        Self(
            isolated(hosts.0.path())
                .args(["remote", role])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(std::fs::File::create(hosts.0.path().join(format!("{role}.log"))).unwrap())
                .spawn()
                .unwrap(),
        )
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn remote_info(hosts: &Hosts) -> serde_json::Value {
    for _ in 0..1000 {
        let output = isolated(hosts.0.path())
            .args(["remote", "info"])
            .output()
            .unwrap();
        if output.status.success() {
            return serde_json::from_slice(&output.stdout).unwrap();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!(
        "remote hosts did not publish live discovery: terminal={} work={}",
        std::fs::read_to_string(hosts.0.path().join("daemon.log")).unwrap_or_default(),
        std::fs::read_to_string(hosts.0.path().join("host.log")).unwrap_or_default()
    );
}

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
fn a_paired_target_or_invalid_execution_file_never_runs_native_client_commands_locally() {
    let paired = format!(
        "backend = 'native'\naddress = 'work-machine.local:47130'\nserver = '{}'\n",
        "ab".repeat(32)
    );
    for config in [
        paired.as_str(),
        "backend = 'native'\naddress = 'work-machine.local:47130'\n",
        "backend = 'unknown'\n",
    ] {
        let hosts = Hosts::new();
        let state = hosts.0.path().join("state/adroit/instances/headless-test");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(state.join("execution.toml"), config).unwrap();
        for args in [
            vec!["terminal", "list", "--json"],
            vec!["session", "list"],
            vec!["daemon", "stop"],
            vec!["host", "stop"],
            vec!["editor", "/tmp/prompt.md"],
        ] {
            let output = isolated(hosts.0.path()).args(&args).output().unwrap();
            assert!(
                !output.status.success(),
                "{args:?} unexpectedly used the local machine"
            );
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("execution"),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert!(!hosts.0.path().join("work.sock").exists());
        assert!(!hosts.0.path().join("term.sock").exists());
        assert!(
            isolated(hosts.0.path())
                .arg("--version")
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}

#[test]
fn a_terminal_owned_startup_lease_expires_without_a_work_host_and_a_late_release_does_not_type_enter()
 {
    let hosts = Hosts::new();
    let _terminal = Service::terminal(&hosts);
    let marker = hosts.0.path().join("started");
    let reply = hosts.0.path().join("reply");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let daemon = adroit_term::Daemon::new(hosts.0.path().join("term.sock"));
        tokio::time::timeout(Duration::from_secs(5), async {
            while daemon.sessions().await.is_err() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let spawn = adroit_term::Spawn {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                "printf once >> \"$1\"; IFS= read -r line; printf '%s' \"$line\" > \"$2\"; sleep 5"
                    .into(),
                "sh".into(),
                marker.to_string_lossy().into_owned(),
                reply.to_string_lossy().into_owned(),
            ],
            cwd: hosts.0.path().into(),
            cols: 80,
            rows: 24,
            ..Default::default()
        };
        let id = daemon
            .spawn_held_for(
                spawn,
                Path::new(env!("CARGO_BIN_EXE_adroit")),
                Duration::from_millis(150),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while std::fs::read(&marker).ok().as_deref() != Some(b"once") {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(std::fs::read(&marker).unwrap(), b"once");
        daemon.release(id).await.unwrap();
        assert!(
            !reply.exists(),
            "late release reached the application as input"
        );
        daemon.press(id, b"actual-input\r").await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while std::fs::read(&reply).ok().as_deref() != Some(b"actual-input") {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(std::fs::read(&reply).unwrap(), b"actual-input");
        assert!(!hosts.0.path().join("work.sock").exists());
        daemon.end(id).await.unwrap();
    });
}

#[test]
fn an_explicit_startup_release_runs_the_program_once_before_the_lease_expires() {
    let hosts = Hosts::new();
    let _terminal = Service::terminal(&hosts);
    let marker = hosts.0.path().join("started");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let daemon = adroit_term::Daemon::new(hosts.0.path().join("term.sock"));
        tokio::time::timeout(Duration::from_secs(5), async {
            while daemon.sessions().await.is_err() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let spawn = adroit_term::Spawn {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                "printf once >> \"$1\"; sleep 5".into(),
                "sh".into(),
                marker.to_string_lossy().into_owned(),
            ],
            cwd: hosts.0.path().into(),
            cols: 80,
            rows: 24,
            ..Default::default()
        };
        let id = daemon
            .spawn_held_for(
                spawn,
                Path::new(env!("CARGO_BIN_EXE_adroit")),
                Duration::from_secs(10),
            )
            .await
            .unwrap();
        assert!(!marker.exists());
        tokio::time::timeout(Duration::from_secs(2), daemon.release(id))
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while std::fs::read(&marker).ok().as_deref() != Some(b"once") {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        daemon.release(id).await.unwrap();
        assert_eq!(std::fs::read(&marker).unwrap(), b"once");
        daemon.end(id).await.unwrap();
    });
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
            assert!(matches!(answer, Answer::Mutation { status: MutationState::Result { answer } } if matches!(*answer, Answer::Failed { ref text } if text == "the session belongs to another working profile")), "job {index} was not refused by ownership");
            let listed = daemon.sessions().await.unwrap();
            assert!(listed.iter().any(|s| s.id == id && !s.exited));
            assert!(!listed.iter().any(|s| s.tags.contains(&"adroit.name=foreign".to_string())));
        }
        daemon.end(id).await.unwrap();
    });
}

#[test]
fn the_headless_cli_keeps_a_terminal_through_replacing_its_work_host() {
    kept_through_restart(false);
}

#[test]
fn existing_terminals_remain_usable_when_a_replacement_work_host_cannot_prepare_terminfo() {
    kept_through_restart(true);
}

fn kept_through_restart(break_terminfo: bool) {
    let hosts = Hosts::new();
    let terminal = hosts.run(&[
        "terminal",
        "start",
        "--name",
        "kept",
        "--dir",
        hosts.0.path().to_str().unwrap(),
        "--program",
        "command",
        "--json",
        "--",
        "/bin/sh",
        "-c",
        "printf 'before-restart\\n'; while IFS= read -r line; do printf 'reply:%s\\n' \"$line\"; done",
    ]);
    let id = terminal["resource_id"].as_str().unwrap();
    let stopped = isolated(hosts.0.path())
        .args(["host", "stop"])
        .output()
        .unwrap();
    assert!(stopped.status.success());
    if break_terminfo {
        let terminfo = hosts.0.path().join("state/adroit/terminfo");
        std::fs::rename(&terminfo, terminfo.with_extension("kept")).unwrap();
        std::fs::write(&terminfo, "not a directory").unwrap();
    }
    let input = hosts.run(&[
        "terminal",
        "input",
        "--terminal",
        id,
        "--text",
        "after-restart",
        "--keys",
        "Enter",
        "--json",
    ]);
    assert_eq!(input["sent"], true);
    let mut screen = String::new();
    for _ in 0..100 {
        screen = hosts
            .run(&["terminal", "screen", "--terminal", id, "--json"])
            .to_string();
        if screen.contains("before-restart") && screen.contains("reply:after-restart") {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(screen.contains("before-restart"), "{screen}");
    assert!(screen.contains("reply:after-restart"), "{screen}");
    let listed = hosts.run(&["terminal", "list", "--json"]);
    assert_eq!(listed.as_array().unwrap().len(), 1);
    if break_terminfo {
        let marker = hosts.0.path().join("must-not-start");
        let output = isolated(hosts.0.path())
            .args([
                "terminal",
                "start",
                "--program",
                "command",
                "--json",
                "--",
                "/usr/bin/touch",
                marker.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("cannot install the terminal's description")
        );
        assert!(!marker.exists());
    }
    hosts.run(&["terminal", "stop", "--terminal", id, "--json"]);
}

#[test]
fn environment_profiles_share_services_but_keep_terminals_settings_and_recovery_apart() {
    let hosts = Hosts::new();
    let root = hosts.0.path();
    let run = |scope: Option<&str>, args: &[&str]| {
        let mut command = isolated(root);
        if let Some(scope) = scope {
            command.env("ADROIT_ENVIRONMENT", scope);
        }
        let output = command.args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let dir = root.to_str().unwrap();
    let a = run(
        Some("personal"),
        &[
            "terminal",
            "start",
            "--program",
            "command",
            "--name",
            "personal-work",
            "--dir",
            dir,
            "--json",
            "--",
            "/bin/sh",
            "-c",
            "printf personal; read answer",
        ],
    );
    let b = run(
        Some("work"),
        &[
            "terminal",
            "start",
            "--program",
            "command",
            "--name",
            "work-work",
            "--dir",
            dir,
            "--json",
            "--",
            "/bin/sh",
            "-c",
            "printf work; read answer",
        ],
    );
    assert_ne!(a["resource_id"], b["resource_id"]);
    let execution = root.join("state/adroit/instances/headless-test/execution.toml");
    adroit_connect::Target::Native {
        address: "work.local:47130".into(),
        server: "ab".repeat(32),
    }
    .save(&execution)
    .unwrap();
    assert_eq!(
        run(Some("personal"), &["terminal", "list", "--json"])
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        run(Some("work"), &["terminal", "list", "--json"])
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        !isolated(root)
            .args(["terminal", "list", "--json"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let foreign = isolated(root)
        .env("ADROIT_ENVIRONMENT", "work")
        .args([
            "terminal",
            "stop",
            "--terminal",
            a["resource_id"].as_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(!foreign.status.success());
    assert!(
        isolated(root)
            .env("ADROIT_WORK_INSTANCE", "headless-test")
            .args(["terminal", "list", "--json"])
            .output()
            .unwrap()
            .status
            .success()
    );
    std::fs::remove_file(execution).unwrap();
    let restarted = isolated(root).args(["host", "restart"]).output().unwrap();
    assert!(restarted.status.success());
    let personal = run(Some("personal"), &["terminal", "list", "--json"]);
    assert_eq!(personal[0]["resource_id"], a["resource_id"]);
    assert_eq!(
        run(Some("work"), &["terminal", "list", "--json"])[0]["resource_id"],
        b["resource_id"]
    );
}

#[test]
fn profile_agents_and_work_settings_are_scoped_and_stale_saves_are_refused() {
    use adroit_protocol::{Answer, Ask, Event, FileText, Request, SPOKEN, decode, encode};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    fn connect(root: &Path, id: &str, expected_scratch: Option<&str>) -> BufReader<UnixStream> {
        let mut stream = UnixStream::connect(root.join("work.sock")).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut hello = String::new();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        reader.read_line(&mut hello).unwrap();
        writeln!(
            stream,
            "{}",
            serde_json::json!({"op":"environment", "id":id})
        )
        .unwrap();
        let mut selected = String::new();
        reader.read_line(&mut selected).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&selected).unwrap()["environment"],
            id
        );
        stream
            .write_all(
                encode(&Request::Hello {
                    versions: SPOKEN.to_vec(),
                })
                .as_bytes(),
            )
            .unwrap();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if let Event::Snapshot(snapshot) = decode::<Event>(&line).unwrap() {
                if let Some(expected) = expected_scratch {
                    assert_eq!(
                        snapshot.config.configured.unwrap().scratch,
                        Path::new(expected)
                    );
                }
                return reader;
            }
        }
    }
    fn save(reader: &mut BufReader<UnixStream>, text: String, expected: FileText) -> Answer {
        reader
            .get_mut()
            .write_all(
                encode(&Request::Ask {
                    asked: 55,
                    ask: Box::new(Ask::SaveSettingsChecked { text, expected }),
                })
                .as_bytes(),
            )
            .unwrap();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(!line.is_empty());
            if let Event::Answer { asked: 55, answer } = decode::<Event>(&line).unwrap() {
                return answer;
            }
        }
    }
    let hosts = Hosts::new();
    hosts.run(&["terminal", "list", "--json"]);
    let mut personal = connect(hosts.0.path(), "personal", None);
    let mut work = connect(hosts.0.path(), "work", None);
    let text = "scratch_dir = '/personal-work'\n[[profiles]]\nid = 'personal-agent'\nagent = 'codex'\n[profiles.launch]\ncommand = ['/personal-cli']\n".to_string();
    assert!(matches!(
        save(&mut personal, text.clone(), FileText::Missing),
        Answer::Done
    ));
    assert!(matches!(
        save(&mut personal, text.clone(), FileText::Missing),
        Answer::Failed { .. }
    ));
    let other = text
        .replace("/personal-cli", "/work-cli")
        .replace("/personal-work", "/work-folder");
    assert!(matches!(
        save(&mut work, other, FileText::Missing),
        Answer::Done
    ));
    let base = hosts
        .0
        .path()
        .join("state/adroit/instances/headless-test/environments");
    let personal_file =
        std::fs::read_to_string(base.join("personal/config/settings.toml")).unwrap();
    let work_file = std::fs::read_to_string(base.join("work/config/settings.toml")).unwrap();
    assert!(personal_file.contains("/personal-cli"));
    assert!(!personal_file.contains("/work-cli"));
    assert!(work_file.contains("/work-cli"));
    assert!(!work_file.contains("/personal-cli"));
    assert!(!hosts.0.path().join("config/adroit/settings.toml").exists());
    drop(personal);
    drop(work);
    assert!(
        isolated(hosts.0.path())
            .args(["host", "restart"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let _personal = connect(hosts.0.path(), "personal", Some("/personal-work"));
    let _work = connect(hosts.0.path(), "work", Some("/work-folder"));
}

struct Gatewayed {
    _services: Vec<Service>,
    info: serde_json::Value,
    address: String,
    server: String,
}

/// Both remote hosts and a gateway in front of them, on a free local port.
fn gatewayed(hosts: &Hosts, advertise: bool) -> Gatewayed {
    let _terminal = Service::start(hosts, "daemon");
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
    let _work = Service::start(hosts, "host");
    let info = remote_info(hosts);
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let address = format!("127.0.0.1:{port}");
    let log = hosts.0.path().join("gateway.log");
    let _gateway = Service(
        isolated(hosts.0.path())
            .args(["remote", "gateway", "--listen", &address])
            .args((!advertise).then_some("--no-advertise"))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::fs::File::create(&log).unwrap())
            .spawn()
            .unwrap(),
    );
    let server = (0..250)
        .find_map(|_| {
            let text = std::fs::read_to_string(&log).unwrap_or_default();
            let found = text
                .split_whitespace()
                .skip_while(|word| *word != "gateway")
                .nth(1)
                .filter(|_| text.contains("listening on"))
                .map(str::to_string);
            if found.is_none() {
                std::thread::sleep(Duration::from_millis(20));
            }
            found
        })
        .unwrap_or_else(|| {
            panic!(
                "the gateway did not start: {}",
                std::fs::read_to_string(&log).unwrap_or_default()
            )
        });
    Gatewayed {
        _services: vec![_terminal, _work, _gateway],
        info,
        address,
        server,
    }
}

#[test]
fn a_paired_client_reaches_the_hosts_through_the_gateway_and_leaving_keeps_their_sessions() {
    let hosts = Hosts::new();
    let Gatewayed {
        _services,
        info,
        address,
        server,
    } = gatewayed(&hosts, false);
    let paired_key = hosts.0.path().join("client/paired.key");
    let client = adroit_net::Identity::load_or_create(&paired_key).unwrap();
    adroit_net::Enroll::enroll(
        &adroit_net::PeerFile::open(
            hosts
                .0
                .path()
                .join("state/adroit/instances/headless-test/remote/peers.json"),
        )
        .unwrap(),
        client.fingerprint(),
        "test client",
    )
    .unwrap();
    let target = adroit_connect::Target::Native {
        address: address.clone(),
        server: server.clone(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let remote = adroit_connect::Remote::with_key(&target, &paired_key).unwrap();
        let tunnel = remote.connect(None).await.unwrap();
        assert_eq!(tunnel.info().instance, info["instance"].as_str().unwrap());
        assert_eq!(
            tunnel.info().terminal.generation,
            info["terminal"]["generation"].as_str().unwrap()
        );
        let connection = adroit_protocol::client::Connection::open_unscoped(&remote.work_socket())
            .await
            .unwrap();
        assert_eq!(
            connection.hello.endpoint.as_ref(),
            Some(&tunnel.info().work)
        );
        let id = adroit_term::Daemon::new(info["terminal"]["socket"].as_str().unwrap())
            .spawn(adroit_term::Spawn {
                program: "/bin/sh".into(),
                args: vec!["-c".into(), "printf alive; read line".into()],
                cwd: hosts.0.path().into(),
                cols: 40,
                rows: 6,
                ..Default::default()
            })
            .await
            .unwrap();
        let listed = |sessions: Vec<adroit_term::Session>| sessions.iter().any(|s| s.id == id);
        assert!(listed(remote.terminal().sessions().await.unwrap()));
        drop(connection);
        let seen = tunnel.info().clone();
        drop(tunnel);
        let again = remote.connect(Some(&seen)).await.unwrap();
        assert_eq!(again.info(), &seen);
        assert!(listed(remote.terminal().sessions().await.unwrap()));

        let stranger =
            adroit_connect::Remote::with_key(&target, &hosts.0.path().join("client/stranger.key"))
                .unwrap();
        let refused = stranger.connect(None).await.err().unwrap();
        assert!(refused.contains("refused this client's key"), "{refused}");
    });
}

/// Lines a child prints, read on a thread of their own.
fn lines_of(output: impl std::io::Read + Send + 'static) -> std::sync::mpsc::Receiver<String> {
    let (send, lines) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(output).lines() {
            let Ok(line) = line else { break };
            if send.send(line).is_err() {
                break;
            }
        }
    });
    lines
}

fn line_with(lines: &std::sync::mpsc::Receiver<String>, needle: &str) -> String {
    loop {
        let line = lines
            .recv_timeout(Duration::from_secs(20))
            .unwrap_or_else(|_| panic!("no line with {needle:?}"));
        if line.contains(needle) {
            return line;
        }
    }
}

#[test]
fn the_cli_pairs_a_client_by_the_code_it_shows_and_saves_a_profile() {
    use std::io::Write;
    use std::process::Stdio;
    let hosts = Hosts::new();
    let gateway = gatewayed(&hosts, false);
    let mut client = isolated(hosts.0.path())
        .args(["connect", "pair", &gateway.address, "--name", "laptop"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let client_lines = lines_of(client.stdout.take().unwrap());
    let shown = line_with(&client_lines, "type this code");
    let code = shown.split_whitespace().last().unwrap().to_string();

    let mut operator = isolated(hosts.0.path())
        .args(["remote", "pair"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let operator_lines = lines_of(operator.stdout.take().unwrap());
    let request = line_with(&operator_lines, "Request from");
    assert!(request.contains("\"laptop\""), "{request}");
    let mut typing = operator.stdin.take().unwrap();
    writeln!(typing, "{code}").unwrap();
    line_with(&operator_lines, "Paired");
    assert!(operator.wait().unwrap().success());
    line_with(&client_lines, "added the profile");
    assert!(client.wait().unwrap().success());

    let catalog =
        std::fs::read_to_string(hosts.0.path().join("config/adroit/environments.toml")).unwrap();
    assert!(catalog.contains("backend = \"native\""), "{catalog}");
    assert!(catalog.contains(&gateway.server), "{catalog}");
    let devices = isolated(hosts.0.path())
        .args(["remote", "devices"])
        .output()
        .unwrap();
    let devices = String::from_utf8_lossy(&devices.stdout);
    assert!(devices.contains("laptop"), "{devices}");
}

#[test]
fn a_paired_work_machine_is_found_on_the_local_network_when_its_address_changed() {
    let hosts = Hosts::new();
    let gateway = gatewayed(&hosts, true);
    let key = hosts.0.path().join("client/paired.key");
    let client = adroit_net::Identity::load_or_create(&key).unwrap();
    adroit_net::Enroll::enroll(
        &adroit_net::PeerFile::open(
            hosts
                .0
                .path()
                .join("state/adroit/instances/headless-test/remote/peers.json"),
        )
        .unwrap(),
        client.fingerprint(),
        "test client",
    )
    .unwrap();
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let target = adroit_connect::Target::Native {
        address: closed.to_string(),
        server: gateway.server.clone(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let remote = adroit_connect::Remote::with_key(&target, &key).unwrap();
        let tunnel = remote.connect(None).await.unwrap();
        assert_eq!(
            tunnel.info().instance,
            gateway.info["instance"].as_str().unwrap()
        );
        assert_eq!(tunnel.reached(), gateway.address);
    });
    let heard = isolated(hosts.0.path())
        .args(["connect", "discover"])
        .output()
        .unwrap();
    let heard = String::from_utf8_lossy(&heard.stdout);
    assert!(heard.contains(&gateway.server[..16]), "{heard}");
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
