//! The local hosts a command reaches and starts: which instance's, their
//! stopping and restarting, the terminals and sessions that outlive them,
//! and the environments they keep apart.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::Duration;

mod support {
    pub mod isolated;
}

use support::isolated::{Hosts, Service, answering_terminal, isolated, listening};

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
/// A link to the work host at `root`, in the environment `id`, once it sent
/// its snapshot, whose scratch folder is `expected_scratch` where given.
fn environment_link(
    root: &Path,
    id: &str,
    expected_scratch: Option<&str>,
) -> std::io::BufReader<std::os::unix::net::UnixStream> {
    use adroit_protocol::{Event, Request, SPOKEN, decode, encode};
    use std::io::{BufRead, Write};
    use std::os::unix::net::UnixStream;
    let mut stream = UnixStream::connect(root.join("work.sock")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut hello = String::new();
    let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
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

#[test]
fn a_window_in_an_environment_of_this_machine_restarts_the_sessions_daemon() {
    use adroit_protocol::{Answer, Ask, Event, Request, decode, encode};
    use std::io::{BufRead, Write};
    let hosts = Hosts::new();
    let root = hosts.0.path();
    let started = isolated(root)
        .env("ADROIT_ENVIRONMENT", "personal")
        .args([
            "terminal",
            "start",
            "--program",
            "command",
            "--name",
            "held",
            "--dir",
            root.to_str().unwrap(),
            "--json",
            "--",
            "/bin/sh",
            "-c",
            "read answer",
        ])
        .output()
        .unwrap();
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let daemon_pid = || adroit_term::Daemon::new(root.join("term.sock")).pid();
    let held = daemon_pid();
    assert!(held.is_some());
    let mut link = environment_link(root, "personal", None);
    link.get_mut()
        .write_all(
            encode(&Request::Ask {
                asked: 7,
                ask: Box::new(Ask::RestartTerminalHost),
            })
            .as_bytes(),
        )
        .unwrap();
    link.get_mut()
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let answer = loop {
        let mut line = String::new();
        link.read_line(&mut line).unwrap();
        assert!(!line.is_empty());
        if let Event::Answer { asked: 7, answer } = decode::<Event>(&line).unwrap() {
            break answer;
        }
    };
    assert!(matches!(answer, Answer::TerminalHost { .. }), "{answer:?}");
    assert_ne!(daemon_pid(), held);
    let listed = isolated(root)
        .env("ADROIT_ENVIRONMENT", "personal")
        .args(["terminal", "list", "--json"])
        .output()
        .unwrap();
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 0, "{listed}");
}

#[test]
fn profile_agents_and_work_settings_are_scoped_and_stale_saves_are_refused() {
    use adroit_protocol::{Answer, Ask, Event, FileText, Request, decode, encode};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

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
    let mut personal = environment_link(hosts.0.path(), "personal", None);
    let mut work = environment_link(hosts.0.path(), "work", None);
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
    let _personal = environment_link(hosts.0.path(), "personal", Some("/personal-work"));
    let _work = environment_link(hosts.0.path(), "work", Some("/work-folder"));
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
            // A run's line is written in pieces; the last one may not be
            // whole yet.
            .split_inclusive('\n')
            .filter_map(|line| line.strip_suffix('\n'))
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
