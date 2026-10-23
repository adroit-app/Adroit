//! Provider-specific transport acceptance, recorded as receipt rather than processing.
use std::{fmt, path::PathBuf, pin::Pin, process::Stdio, time::Duration};

use tokio::io::AsyncReadExt;

use crate::AgentSettings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryFailure {
    Unsupported,
    Rejected(String),
    Uncertain(String),
}
impl fmt::Display for DeliveryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => {
                f.write_str("this provider has no command-based delivery transport")
            }
            Self::Rejected(message) | Self::Uncertain(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for DeliveryFailure {}

pub type DeliveryFuture =
    Pin<Box<dyn std::future::Future<Output = Result<(), DeliveryFailure>> + Send>>;

pub(super) async fn codex_queue(
    settings: AgentSettings,
    cwd: PathBuf,
    conversation: String,
    envelope: String,
) -> Result<(), DeliveryFailure> {
    use DeliveryFailure::{Rejected, Uncertain};
    if conversation.parse::<adroit_model::MessageId>().is_err() {
        return Err(Rejected(
            "Codex delivery requires a conversation UUID, not a session name".into(),
        ));
    }
    let (program, args, env) = super::launch_with_settings(
        &settings,
        adroit_model::Agent::Codex,
        vec![
            "queue".into(),
            "--thread".into(),
            conversation,
            "--message".into(),
            envelope,
        ],
        vec![],
    );
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .envs(env)
        .env_remove("CODEX_THREAD_ID")
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| Rejected(format!("cannot start Codex queue: {error}")))?;
    let mut stderr = child.stderr.take().expect("piped stderr");
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let diagnostic = async {
            let mut kept = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let count = stderr.read(&mut buffer).await?;
                if count == 0 {
                    break;
                }
                let take = count.min((16 * 1024_usize).saturating_sub(kept.len()));
                kept.extend_from_slice(&buffer[..take]);
            }
            Ok::<_, std::io::Error>(String::from_utf8_lossy(&kept).trim().to_string())
        };
        let (status, diagnostic) = tokio::join!(child.wait(), diagnostic);
        let status =
            status.map_err(|error| Uncertain(format!("cannot observe Codex queue: {error}")))?;
        let diagnostic = diagnostic
            .map_err(|error| Uncertain(format!("cannot read Codex queue diagnostics: {error}")))?;
        if status.success() {
            Ok(())
        } else {
            Err(Uncertain(format!(
                "Codex queue ended with {status}: {diagnostic}"
            )))
        }
    })
    .await;
    match result {
        Ok(result) => result,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(Uncertain(
                "Codex queue timed out; the message may have been queued".into(),
            ))
        }
    }
}
