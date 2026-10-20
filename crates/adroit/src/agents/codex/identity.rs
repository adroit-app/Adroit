//! A shortened native title is corroborated by the provider, never used as an ID.
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};

use super::{Agent, Settings, rpc, transcript};

pub(super) fn prefix(title: &str) -> Option<String> {
    let mut matches = title.split_whitespace().filter_map(|part| {
        let prefix = part.strip_suffix("...")?;
        (prefix.len() == 29 && transcript::thread_id(&format!("{prefix}0000000")))
            .then(|| prefix.to_owned())
    });
    let found = matches.next()?;
    matches.next().is_none().then_some(found)
}

pub(super) async fn resolve(settings: Settings, cwd: PathBuf, title: String) -> Option<String> {
    let prefix = prefix(&title)?;
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut cursor = Value::Null;
        let mut matched = BTreeSet::new();
        for _ in 0..8 {
            let start = crate::agents::launch_with_settings(
                &settings,
                Agent::Codex,
                vec!["app-server".into()],
                Vec::new(),
            );
            let page = rpc::request(
                start,
                &cwd,
                Duration::from_secs(3),
                "thread/list",
                json!({
                    "cursor":cursor, "limit":100, "sortKey":"updated_at", "cwd":cwd,
                    "useStateDbOnly":true
                }),
            )
            .await?;
            for thread in page["data"].as_array()? {
                if let Some(id) = thread["id"]
                    .as_str()
                    .filter(|id| transcript::thread_id(id) && id.starts_with(&prefix))
                {
                    matched.insert(id.to_owned());
                }
            }
            if matched.len() > 1 {
                return None;
            }
            match page.get("nextCursor")? {
                Value::Null => return matched.into_iter().next(),
                Value::String(next) => cursor = json!(next),
                _ => return None,
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
}
