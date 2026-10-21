use std::path::PathBuf;
use std::process::Stdio;

pub fn serve_process(socket: PathBuf) -> tokio::task::JoinHandle<()> {
    // Native socket binding changes the process-wide umask temporarily.
    let module = module_path!().split_once("::").unwrap().1;
    let fixture = format!("{module}::fixture");
    tokio::spawn(async move {
        let mut child = fixture_command()
            .args(["--exact", &fixture, "--ignored", "--nocapture"])
            .env("ADROIT_TEST_NATIVE_SOCKET", socket)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let status = child.wait().await.unwrap();
        assert!(status.success(), "native daemon fixture failed: {status}");
    })
}

#[tokio::test]
#[ignore = "invoked in a subprocess to isolate native socket permissions"]
async fn fixture() {
    let socket = std::env::var_os("ADROIT_TEST_NATIVE_SOCKET").unwrap();
    adroit_term::Daemon::serve(
        std::path::Path::new(&socket),
        std::time::Duration::from_secs(60),
        |_| {},
    )
    .await
    .unwrap();
}

#[test]
fn fixture_processes_explicitly_drop_the_callers_assignment_environment() {
    let command = fixture_command();
    let removed: std::collections::HashSet<_> = command
        .as_std()
        .get_envs()
        .filter(|(_, value)| value.is_none())
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect();
    for name in [
        "ADROIT_CONTROL_TOKEN",
        "ADROIT_CONTROL_ROOT",
        "ADROIT_TASK_ID",
        "ADROIT_ATTEMPT_ID",
        "CODEX_THREAD_ID",
    ] {
        assert!(removed.contains(name), "the fixture must remove {name}");
    }
}

fn fixture_command() -> tokio::process::Command {
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    for name in [
        "ADROIT_CONTROL_TOKEN",
        "ADROIT_CONTROL_ROOT",
        "ADROIT_SESSION_ID",
        "ADROIT_SESSION_GENERATION",
        "ADROIT_LEADER_ID",
        "ADROIT_RUN_ID",
        "ADROIT_TASK_ID",
        "ADROIT_ATTEMPT_ID",
        "CODEX_THREAD_ID",
    ] {
        command.env_remove(name);
    }
    let names = std::env::vars_os().map(|(name, _)| name.to_string_lossy().into_owned());
    for name in names.filter(|name| name.starts_with("ADROIT_")) {
        command.env_remove(name);
    }
    command
}
