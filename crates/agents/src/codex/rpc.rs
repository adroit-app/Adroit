use serde_json::{Value, json};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Ask an app-server process one question, without creating a thread or turn.
pub(super) async fn request(
    (program, args, env): crate::Started,
    dir: &Path,
    timeout: Duration,
    method: &str,
    params: Value,
) -> Option<Value> {
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .envs(env)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let mut lines = BufReader::new(child.stdout.take()?).lines();
    let asked = async {
        let initialize = json!({
            "id":"adroit-init", "method":"initialize",
            "params":{"clientInfo":{"name":"adroit","version":env!("CARGO_PKG_VERSION")}}
        });
        stdin
            .write_all(format!("{initialize}\n").as_bytes())
            .await
            .ok()?;
        stdin.flush().await.ok()?;
        response(&mut lines, "adroit-init").await?;
        let query = json!({"id":"adroit-query", "method":method,"params":params});
        stdin
            .write_all(format!("{{\"method\":\"initialized\"}}\n{query}\n").as_bytes())
            .await
            .ok()?;
        stdin.flush().await.ok()?;
        response(&mut lines, "adroit-query").await
    };
    let got = tokio::time::timeout(timeout, asked).await.ok().flatten();
    let _ = child.start_kill();
    let _ = child.wait().await;
    got
}

async fn response(
    lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    id: &str,
) -> Option<Value> {
    while let Some(line) = lines.next_line().await.ok()? {
        let Ok(reply) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if reply["id"] == id {
            return reply.get("result").cloned();
        }
    }
    None
}
