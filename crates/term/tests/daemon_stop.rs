//! A daemon serving in a process of its own is found by its socket and
//! stopped.

use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use adroit_term::Daemon;

mod support;
use support::{sh, socket_dir};

const SOCKET_ENV: &str = "ADROIT_TEST_STOP_SOCKET";

/// A daemon run by this test binary in a process of its own.
struct Served {
    pid: u32,
    /// Waited on all along, so the process is reaped as soon as it ends.
    exited: tokio::task::JoinHandle<ExitStatus>,
}

async fn run(fixture: &str, socket: &Path) -> Served {
    let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", fixture, "--ignored", "--nocapture"])
        .env(SOCKET_ENV, socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    let exited = tokio::spawn(async move { child.wait().await.unwrap() });
    let daemon = Daemon::new(socket);
    for _ in 0..200 {
        if daemon.pid().is_some() {
            return Served { pid, exited };
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{fixture} never listened");
}

async fn until_gone(pid: u32) -> bool {
    for _ in 0..250 {
        if !adroit_sys::process::alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test]
async fn the_process_serving_a_socket_is_found_from_it() {
    let dir = socket_dir();
    let socket = dir.path().join("d.sock");
    let served = run("serving_fixture", &socket).await;
    assert_eq!(Daemon::new(&socket).pid(), Some(served.pid));
}

#[test]
fn no_process_is_found_where_nothing_listens() {
    let dir = socket_dir();
    assert_eq!(Daemon::new(dir.path().join("d.sock")).pid(), None);
}

#[tokio::test]
#[cfg_attr(windows, ignore = "sh tells a process id of its own, not Windows'")]
async fn stopping_ends_the_daemon_and_the_programs_of_its_sessions() {
    let dir = socket_dir();
    let socket = dir.path().join("d.sock");
    let served = run("serving_fixture", &socket).await;
    let daemon = Daemon::new(&socket);
    let told = dir.path().join("program.pid");
    let mut spawn = sh(r#"echo $$ > "$0"; exec sleep 30"#);
    spawn.args.push(told.to_string_lossy().into_owned());
    daemon.spawn(spawn).await.unwrap();
    let mut program = None;
    for _ in 0..200 {
        if let Some(pid) = std::fs::read_to_string(&told)
            .ok()
            .and_then(|t| t.trim().parse::<u32>().ok())
        {
            program = Some(pid);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let program = program.expect("the session's program never ran");

    assert!(daemon.stop().await.unwrap());

    tokio::time::timeout(Duration::from_secs(5), served.exited)
        .await
        .expect("the daemon still runs")
        .unwrap();
    assert!(
        until_gone(program).await,
        "the session's program still runs"
    );
    assert_eq!(daemon.pid(), None);
}

#[tokio::test]
async fn a_daemon_serving_on_does_not_drain() {
    let dir = socket_dir();
    let socket = dir.path().join("d.sock");
    let _served = run("serving_fixture", &socket).await;

    assert!(!Daemon::new(&socket).draining().await.unwrap());
}

#[tokio::test]
#[cfg_attr(windows, ignore = "sh traps signals of its own, not Windows'")]
async fn a_daemon_told_to_stop_drains_while_it_takes_its_sessions_down() {
    let dir = socket_dir();
    let socket = dir.path().join("d.sock");
    let _served = run("serving_fixture", &socket).await;
    let daemon = Daemon::new(&socket);
    let told = dir.path().join("program.pid");
    let mut spawn = sh(r#"trap '' HUP TERM; echo $$ > "$0"; while :; do sleep 1; done"#);
    spawn.args.push(told.to_string_lossy().into_owned());
    daemon.spawn(spawn).await.unwrap();
    for _ in 0..200 {
        if told.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let stopping = tokio::spawn({
        let daemon = Daemon::new(&socket);
        async move { daemon.stop().await }
    });

    let mut seen = false;
    for _ in 0..200 {
        match daemon.draining().await {
            Ok(true) => {
                seen = true;
                break;
            }
            Ok(false) => tokio::time::sleep(Duration::from_millis(5)).await,
            Err(_) => break,
        }
    }
    stopping.await.unwrap().unwrap();
    assert!(seen, "the daemon was never seen draining");
}

#[tokio::test]
async fn a_daemon_that_does_not_answer_is_still_stopped() {
    let dir = socket_dir();
    let socket = dir.path().join("d.sock");
    let served = run("deaf_fixture", &socket).await;

    assert!(Daemon::new(&socket).stop().await.unwrap());

    tokio::time::timeout(Duration::from_secs(5), served.exited)
        .await
        .expect("the deaf daemon still runs")
        .unwrap();
    assert!(!adroit_sys::process::alive(served.pid));
}

#[tokio::test]
async fn stopping_where_nothing_listens_stops_nothing() {
    let dir = socket_dir();
    let stopped = Daemon::new(dir.path().join("d.sock")).stop().await.unwrap();
    assert!(!stopped);
}

#[tokio::test]
#[ignore = "invoked in a subprocess as the daemon to stop"]
async fn serving_fixture() {
    let socket = std::env::var_os(SOCKET_ENV).unwrap();
    Daemon::serve(Path::new(&socket), Duration::from_secs(60), |_| {})
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "invoked in a subprocess as a daemon too old to answer"]
async fn deaf_fixture() {
    let socket = std::env::var_os(SOCKET_ENV).unwrap();
    let listener = adroit_sys::local::Listener::bind(socket).unwrap();
    let mut held = Vec::new();
    while let Ok(stream) = listener.accept().await {
        held.push(stream);
    }
}
