//! What the tests that run the executable share: a process whose hosts
//! and data belong to the test alone, and waits on those hosts.
// Each test binary uses part of it.
#![allow(dead_code)]

use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// A process whose hosts and data belong to this test alone.
pub fn isolated(dir: &Path) -> Command {
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

pub struct Hosts(pub tempfile::TempDir);

impl Hosts {
    pub fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("adroit-h-")
            .tempdir_in(adroit_sys::local::temp_dir())
            .unwrap();
        adroit_sys::fs::set_mode(dir.path(), 0o700).unwrap();
        Self(dir)
    }

    pub fn run(&self, args: &[&str]) -> serde_json::Value {
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

pub struct Service(pub std::process::Child);

impl Service {
    pub fn terminal(hosts: &Hosts) -> Self {
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
    pub fn start(hosts: &Hosts, role: &str) -> Self {
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
pub async fn answering_terminal(hosts: &Hosts) -> adroit_term::Daemon {
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

pub fn remote_info(hosts: &Hosts) -> serde_json::Value {
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

/// Wait until something listens at `socket`.
pub fn listening(socket: &Path) {
    for _ in 0..500 {
        if std::os::unix::net::UnixStream::connect(socket).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("nothing listens at {}", socket.display());
}
