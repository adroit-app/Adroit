//! Asking Pi something through its RPC mode, in a process that keeps no
//! session and runs no turn.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use adroit_model::ProviderModel;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::AgentContext;

/// What starts Pi's RPC mode as `context` runs sessions: in a login of
/// its own, with the skills its sessions load and none of the user's
/// others, so what it lists is what a session there is offered.
pub(super) fn args(context: &AgentContext) -> Vec<String> {
    let mut args: Vec<String> = ["--mode", "rpc", "--no-session"].map(String::from).to_vec();
    args.extend(
        super::options(context)
            .into_iter()
            .skip_while(|arg| arg != "--no-skills"),
    );
    args
}

/// Pi's answer to the command `kind`, started by `start` in `dir`; none for
/// a failure, a refusal or no answer within `timeout`.
pub(super) async fn request(
    (program, args, env): crate::Started,
    dir: &Path,
    timeout: Duration,
    kind: &str,
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
    let mut stdout = BufReader::new(child.stdout.take()?);
    let asked = async {
        let command = json!({"id": "adroit", "type": kind});
        stdin
            .write_all(format!("{command}\n").as_bytes())
            .await
            .ok()?;
        stdin.flush().await.ok()?;
        // Records are split on LF alone: a JSON string may hold U+2028.
        let mut line = Vec::new();
        loop {
            line.clear();
            if stdout.read_until(b'\n', &mut line).await.ok()? == 0 {
                return None;
            }
            let Ok(reply) = serde_json::from_slice::<Value>(&line) else {
                continue;
            };
            if reply["type"] == "response" && reply["id"] == "adroit" {
                return (reply["success"] == true).then(|| reply["data"].clone());
            }
        }
    };
    let got = tokio::time::timeout(timeout, asked).await.ok().flatten();
    let _ = child.start_kill();
    let _ = child.wait().await;
    got
}

/// The models `data` lists, each named as Pi's `--model` takes it.
fn listed(data: &Value) -> Option<Vec<ProviderModel>> {
    Some(
        data["models"]
            .as_array()?
            .iter()
            .filter_map(|m| {
                let provider = m["provider"].as_str().filter(|p| !p.is_empty())?;
                let id = m["id"].as_str().filter(|id| !id.is_empty())?;
                Some(ProviderModel {
                    id: format!("{provider}/{id}"),
                    resolved: None,
                    name: m["name"].as_str().unwrap_or(id).to_string(),
                    description: String::new(),
                })
            })
            .collect(),
    )
}

/// The models Pi, started by `start` in `dir`, can run: those of the
/// providers its login has credentials for.
pub(super) async fn models(
    start: crate::Started,
    dir: &Path,
    timeout: Duration,
) -> Option<Vec<ProviderModel>> {
    listed(&request(start, dir, timeout, "get_available_models").await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{program, started};
    use adroit_model::Agent;

    #[test]
    fn each_model_is_named_by_its_provider_and_id() {
        let got = listed(&json!({"models": [
            {"id": "slow", "name": "Slow fake", "provider": "fake"},
            {"id": "qwen3:8b", "provider": "ollama"},
            {"id": "", "provider": "fake"},
            {"id": "lost"},
        ]}))
        .unwrap();
        assert_eq!(
            got.iter()
                .map(|m| (m.id.as_str(), m.name.as_str()))
                .collect::<Vec<_>>(),
            [("fake/slow", "Slow fake"), ("ollama/qwen3:8b", "qwen3:8b")]
        );
        assert_eq!(listed(&json!({})), None);
    }

    #[tokio::test]
    async fn the_probe_asks_rpc_mode_for_the_available_models_and_ignores_events() {
        let (dir, pi) = program(
            "pi",
            r#"printf '%s ' "$@" > args
IFS= read -r line
printf '%s' "$line" > request
echo '{"type":"agent_start"}'
echo 'not json'
echo '{"type":"response","id":"other","success":true,"data":{}}'
echo '{"type":"response","id":"adroit","command":"get_available_models","success":true,"data":{"models":[{"id":"slow","provider":"fake"}]}}'
exec sleep 30"#,
        );
        let got = models(
            started(
                &pi,
                &args(&AgentContext::builtin(Agent::Pi))
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            ),
            dir.path(),
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(got.map(|m| m.len()), Some(1));
        let request: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join("request")).unwrap())
                .unwrap();
        assert_eq!(request["type"], "get_available_models");
        let asked = std::fs::read_to_string(dir.path().join("args")).unwrap();
        assert_eq!(asked.trim(), "--mode rpc --no-session");
    }

    #[tokio::test]
    async fn a_refusal_or_silence_lists_nothing() {
        let (dir, pi) = program(
            "pi",
            r#"read line
echo '{"type":"response","id":"adroit","success":false,"error":"no"}'
exec sleep 30"#,
        );
        assert_eq!(
            models(started(&pi, &[]), dir.path(), Duration::from_secs(5)).await,
            None
        );
        let (dir, pi) = program("pi", "exec sleep 30");
        assert_eq!(
            models(started(&pi, &[]), dir.path(), Duration::from_millis(200)).await,
            None
        );
    }

    #[test]
    fn in_a_login_of_its_own_the_probe_loads_the_skills_a_session_there_does() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("skills")).unwrap();
        let context = AgentContext {
            home: Some(dir.path().into()),
            ..AgentContext::builtin(Agent::Pi)
        };
        let skills = dir.path().join("skills").display().to_string();
        assert_eq!(
            args(&context),
            [
                "--mode",
                "rpc",
                "--no-session",
                "--no-skills",
                "--skill",
                &skills
            ]
        );
    }
}
