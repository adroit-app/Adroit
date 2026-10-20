use std::path::PathBuf;
use std::process::Stdio;

pub fn serve_process(socket: PathBuf) -> tokio::task::JoinHandle<()> {
    // Native socket binding changes the process-wide umask temporarily.
    let module = module_path!().split_once("::").unwrap().1;
    let fixture = format!("{module}::fixture");
    tokio::spawn(async move {
        let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
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
