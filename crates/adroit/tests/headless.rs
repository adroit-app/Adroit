//! The executable installed on a work machine, built without the window.
//! A work machine runs Unix: its hosts run as the user's services there.
#![cfg(unix)]

use std::path::{Path, PathBuf};
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

#[test]
fn an_isolated_process_reaches_main_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = isolated(dir.path());
    let started = std::time::Instant::now();
    let output = command.arg("--version").output().unwrap();
    assert!(output.status.success());
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
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
    // Cargo links the executable anew, and macOS scans a file it has not
    // run before at its first launch, for seconds that grow with the file
    // and with every other new executable queued on the machine. No
    // deadline bounds that, so the first launch is waited for here, before
    // a test times anything.
    static SCANNED: std::sync::Once = std::sync::Once::new();
    SCANNED.call_once(|| {
        Command::new(env!("CARGO_BIN_EXE_adroit"))
            .arg("--version")
            .output()
            .unwrap();
    });
    let mut command = Command::new(env!("CARGO_BIN_EXE_adroit"));
    for (name, _) in std::env::vars_os() {
        if ["ADROIT_", "FELIS_"]
            .iter()
            .any(|p| name.to_string_lossy().starts_with(p))
            || name == "CODEX_THREAD_ID"
        {
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
            .tempdir_in(adroit_sys::local::temp_dir())
            .unwrap();
        adroit_sys::fs::set_mode(dir.path(), 0o700).unwrap();
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

/// The terminal host `Service::terminal` started, once it answers.
async fn answering_terminal(hosts: &Hosts) -> adroit_term::Daemon {
    let daemon = adroit_term::Daemon::new(hosts.0.path().join("term.sock"));
    tokio::time::timeout(Duration::from_secs(5), async {
        while daemon.sessions().await.is_err() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the terminal host did not answer");
    daemon
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

/// Wait until something listens at `socket`.
fn listening(socket: &Path) {
    for _ in 0..500 {
        if std::os::unix::net::UnixStream::connect(socket).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("nothing listens at {}", socket.display());
}

#[test]
fn an_instance_named_inside_a_session_of_another_reaches_its_own_hosts_not_the_session_s() {
    let hosts = Hosts::new();
    let root = hosts.0.path();
    let mut terminal = Service::terminal(&hosts);
    let socket = root.join("term.sock");
    listening(&socket);
    for command in ["daemon", "host"] {
        // The routing `isolated` names stands for that of a session of the
        // instance `headless-owner`, which the command was typed in.
        let output = isolated(root)
            .env("ADROIT_WORK_INSTANCE", "headless-owner")
            .env("ADROIT_INSTANCE", "headless-probe")
            .env("FELIS_SOCKET", &socket)
            .args([command, "stop"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).starts_with("no "),
            "{command}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    assert!(terminal.0.try_wait().unwrap().is_none());
}

#[test]
fn stopping_or_restarting_the_hosts_a_command_s_own_session_runs_in_is_refused_unless_forced() {
    let hosts = Hosts::new();
    let root = hosts.0.path();
    // Started apart from the test, as a window starts them, so that a stop
    // can wait until they are gone.
    hosts.run(&["terminal", "list", "--json"]);
    let socket = root.join("term.sock");
    listening(&socket);
    let inside = || {
        let mut command = isolated(root);
        command.env("FELIS_SOCKET", &socket);
        command
    };
    for args in [
        ["daemon", "stop"],
        ["daemon", "restart"],
        ["host", "stop"],
        ["host", "restart"],
    ] {
        let output = inside().args(args).output().unwrap();
        assert!(!output.status.success(), "{args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("--force"),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(std::os::unix::net::UnixStream::connect(&socket).is_ok());
    assert!(std::os::unix::net::UnixStream::connect(root.join("work.sock")).is_ok());
    let forced = inside().args(["host", "stop", "--force"]).output().unwrap();
    assert!(
        forced.status.success(),
        "{}",
        String::from_utf8_lossy(&forced.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&forced.stdout).trim(),
        "stopped the work host"
    );
    let forced = inside()
        .args(["daemon", "stop", "--force"])
        .output()
        .unwrap();
    assert!(
        forced.status.success(),
        "{}",
        String::from_utf8_lossy(&forced.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&forced.stdout).trim(),
        "stopped the daemon"
    );
}

#[test]
fn a_work_host_a_command_starts_for_the_hosts_of_another_instance_serves_that_instance() {
    struct Stopped<'a>(&'a Path);
    impl Drop for Stopped<'_> {
        fn drop(&mut self) {
            for command in ["daemon", "host"] {
                let _ = isolated(self.0)
                    .env_remove("ADROIT_HOST")
                    .env_remove("ADROIT_DAEMON")
                    .env("ADROIT_INSTANCE", "headless-work")
                    .args([command, "stop", "--force"])
                    .output();
            }
        }
    }
    let hosts = Hosts::new();
    let root = hosts.0.path();
    let state = root.join("state/adroit/instances/headless-test");
    std::fs::create_dir_all(&state).unwrap();
    adroit_connect::environments::Binding {
        environment: "own".into(),
        name: "Own".into(),
        target: adroit_connect::Target::Local,
        scope: None,
        client: "headless-test".into(),
        work_instance: Some("headless-work".into()),
    }
    .save(&state)
    .unwrap();
    let _stopped = Stopped(root);
    let output = isolated(root)
        .env_remove("ADROIT_HOST")
        .env_remove("ADROIT_DAEMON")
        .args(["terminal", "list", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        root.join("state/adroit/instances/headless-work/host/host.sock")
            .exists()
    );
    assert!(
        !root
            .join("state/adroit/instances/headless-test/host/host.sock")
            .exists()
    );
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
        let daemon = answering_terminal(&hosts).await;
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
        let daemon = answering_terminal(&hosts).await;
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
            assert!(matches!(answer, Answer::Mutation { status: MutationState::Result { answer } } if matches!(*answer, Answer::Failed { ref text } if text == "the session belongs to another working environment")), "job {index} was not refused by ownership");
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
fn environments_share_services_but_keep_terminals_settings_and_recovery_apart() {
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
    /// What the gateway wrote to its standard error.
    log: std::path::PathBuf,
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
    // An announcement names the machine's addresses on the interfaces that
    // multicast, which loopback does not everywhere, so an announced
    // gateway listens on them all.
    let listen = if advertise {
        format!("0.0.0.0:{port}")
    } else {
        address.clone()
    };
    let log = hosts.0.path().join("gateway.log");
    let _gateway = Service(
        isolated(hosts.0.path())
            .args(["remote", "gateway", "--listen", &listen])
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
        log,
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
        ..
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
fn the_cli_pairs_a_client_by_the_code_it_shows_and_saves_an_environment() {
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
    // Asked before the operator allows pairing, the client waits for it.
    line_with(&client_lines, "not accepting pairing requests");

    let mut operator = isolated(hosts.0.path())
        .args(["remote", "pair"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let operator_lines = lines_of(operator.stdout.take().unwrap());
    let shown = line_with(&client_lines, "Type this code");
    let code = shown.split_whitespace().last().unwrap().to_string();
    let request = line_with(&operator_lines, "Request from");
    assert!(request.contains("\"laptop\""), "{request}");
    let mut typing = operator.stdin.take().unwrap();
    writeln!(typing, "{code}").unwrap();
    line_with(&operator_lines, "Paired");
    assert!(operator.wait().unwrap().success());
    line_with(&client_lines, "added the environment");
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

/// Whether the gateway listening at the loopback `address` answers there but
/// at none of the other addresses it is announced at.
async fn answers_only_on_loopback(
    client: &adroit_net::Identity,
    server: adroit_net::Fingerprint,
    address: &str,
) -> bool {
    let answers = |address: String| async move {
        let remote = adroit_net::Remote::new(client.clone(), server, address);
        matches!(
            tokio::time::timeout(Duration::from_secs(3), remote.info()).await,
            Ok(Ok(_))
        )
    };
    if !answers(address.to_string()).await {
        return false;
    }
    let heard = adroit_net::browse(Duration::from_secs(3))
        .await
        .unwrap_or_default();
    let elsewhere: Vec<_> = heard
        .into_iter()
        .filter(|found| found.server == server)
        .flat_map(|found| found.addresses)
        .filter(|announced| !announced.ip().is_loopback())
        .collect();
    if elsewhere.is_empty() {
        return false;
    }
    for announced in elsewhere {
        if answers(announced.to_string()).await {
            return false;
        }
    }
    true
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
    let discover = || {
        let heard = isolated(hosts.0.path())
            .args(["connect", "discover"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&heard.stdout).into_owned()
    };
    let reached = runtime.block_on(async {
        let remote = adroit_connect::Remote::with_key(&target, &key).unwrap();
        let tunnel = match remote.connect(None).await {
            Ok(tunnel) => tunnel,
            Err(error) => {
                let said = std::fs::read_to_string(&gateway.log).unwrap_or_default();
                // As on a macOS CI runner, which keeps a program from the
                // local network with no one to allow it.
                if said.contains("cannot announce itself on the local network") {
                    eprintln!("not tested where the gateway may not announce: {said}");
                    return false;
                }
                // As on a Mac whose local network privacy keeps the gateway
                // a freshly built executable runs from answering anywhere
                // but the loopback, while its announcements still go out.
                let server = gateway.server.parse().unwrap();
                if answers_only_on_loopback(&client, server, &gateway.address).await {
                    eprintln!("not tested where the gateway may answer only on the loopback");
                    return false;
                }
                panic!(
                    "{error}\nthe local network heard: {}\nthe gateway said: {said}",
                    discover()
                )
            }
        };
        assert_eq!(
            tunnel.info().instance,
            gateway.info["instance"].as_str().unwrap()
        );
        // Reached at whichever of the machine's addresses was announced.
        let reached: std::net::SocketAddr = tunnel.reached().parse().unwrap();
        let listening: std::net::SocketAddr = gateway.address.parse().unwrap();
        assert_eq!(reached.port(), listening.port(), "{reached}");
        true
    });
    if !reached {
        return;
    }
    let heard = discover();
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

/// A program standing in for Claude Code that adds the arguments of each
/// run, as one line, to `runs`, and then waits as an agent does.
fn recording_agent(dir: &Path, runs: &Path) -> PathBuf {
    let path = dir.join("agent.sh");
    std::fs::write(
        &path,
        format!(
            "printf '%s ' \"$@\" | tr '\\n' ' ' >> '{}'\necho >> '{}'\nexec sleep 600\n",
            runs.display(),
            runs.display()
        ),
    )
    .unwrap();
    path
}

fn runs_of(runs: &Path, at_least: usize) -> Vec<String> {
    for _ in 0..300 {
        let lines: Vec<String> = std::fs::read_to_string(runs)
            .unwrap_or_default()
            .lines()
            // Adroit also runs the agent to ask what it offers; a session
            // is started with Adroit's plugin.
            .filter(|line| line.contains("--plugin-dir"))
            .map(String::from)
            .collect();
        if lines.len() >= at_least {
            return lines;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the agent ran fewer than {at_least} times");
}

#[test]
#[cfg_attr(windows, ignore = "fakes the agent with a shell script")]
fn a_session_the_terminal_host_took_with_it_comes_back_resuming_its_conversation() {
    let hosts = Hosts::new();
    let root = hosts.0.path();
    let runs = root.join("runs");
    let agent = recording_agent(root, &runs);
    let config = root.join("config/adroit");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("settings.toml"),
        format!(
            "restore_sessions = \"always\"\nprofile = \"fake\"\n\n[[profiles]]\nid = \"fake\"\nagent = \"claude\"\n\n[profiles.launch]\ncommand = [\"/bin/sh\", \"{}\"]\nvia_shell = false\n",
            agent.display()
        ),
    )
    .unwrap();
    let started = isolated(root)
        .args([
            "session",
            "start",
            "--dir",
            root.to_str().unwrap(),
            "--name",
            "fix",
            "--model",
            "opus",
            "--",
            "go",
        ])
        .output()
        .unwrap();
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let first = runs_of(&runs, 1);
    let conversation = first[0]
        .split_whitespace()
        .skip_while(|word| *word != "--session-id")
        .nth(1)
        .unwrap_or_else(|| panic!("no conversation in {first:?}"))
        .to_string();

    let restarted = isolated(root).args(["daemon", "restart"]).output().unwrap();
    assert!(
        restarted.status.success(),
        "{}",
        String::from_utf8_lossy(&restarted.stderr)
    );

    let again = runs_of(&runs, 2);
    assert!(
        again[1].contains(&format!("--resume {conversation}")),
        "{again:?}"
    );
    assert!(again[1].contains("--model opus"), "{again:?}");
    // Brought back once: the session standing for it now is not taken for
    // another one interrupted.
    std::thread::sleep(Duration::from_secs(2));
    let after = runs_of(&runs, 2);
    assert_eq!(after.len(), 2, "{after:?}");
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
