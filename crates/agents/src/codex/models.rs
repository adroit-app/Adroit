use adroit_model::ProviderModel;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

/// The models of one `model/list` page. A list that continues on another
/// page is not the whole list, and checking against part of it would refuse
/// the models it leaves out.
fn models(result: &Value) -> Option<Vec<ProviderModel>> {
    if !result["nextCursor"].is_null() {
        return None;
    }
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    Some(
        result["data"]
            .as_array()?
            .iter()
            .filter_map(|m| {
                let id = m["id"].as_str().filter(|id| !id.is_empty())?.to_string();
                Some(ProviderModel {
                    resolved: m["model"]
                        .as_str()
                        .filter(|r| !r.is_empty() && *r != id)
                        .map(Into::into),
                    name: m["displayName"].as_str().unwrap_or(&id).to_string(),
                    description: text(&m["description"]),
                    id,
                })
            })
            .collect(),
    )
}

/// Ask the installed Codex which models it accepts without starting a thread or turn.
pub(super) async fn probe(
    start: crate::Started,
    dir: &Path,
    timeout: Duration,
) -> Option<Vec<ProviderModel>> {
    models(&super::rpc::request(start, dir, timeout, "model/list", serde_json::json!({})).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn each_listed_model_keeps_its_id_name_and_the_model_it_stands_for() {
        let got = models(&json!({"data": [
            {"id": "gpt-6-sol", "model": "gpt-6-sol", "displayName": "GPT-6-Sol", "description": "Frontier"},
            {"id": "fast", "model": "gpt-6-luna"},
            {"id": "", "model": "nameless"},
        ], "nextCursor": null}))
        .unwrap();
        assert_eq!(
            got,
            [
                ProviderModel {
                    id: "gpt-6-sol".into(),
                    resolved: None,
                    name: "GPT-6-Sol".into(),
                    description: "Frontier".into(),
                },
                ProviderModel {
                    id: "fast".into(),
                    resolved: Some("gpt-6-luna".into()),
                    name: "fast".into(),
                    description: String::new(),
                },
            ]
        );
    }

    #[test]
    fn a_list_continued_on_another_page_or_without_data_is_unknown() {
        assert_eq!(
            models(&json!({"data": [{"id": "gpt-6-sol"}], "nextCursor": "2"})),
            None
        );
        assert_eq!(models(&json!({})), None);
    }

    #[tokio::test]
    async fn the_probe_asks_for_the_model_list() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("codex");
        std::fs::write(
            &program,
            r#"#!/bin/sh
test "$1" = app-server || exit 1
read line
echo '{"id":"adroit-init","result":{}}'
read line
IFS= read -r line
printf '%s\n' "$line" > request
echo '{"id":"adroit-query","result":{"data":[{"id":"gpt-6-sol"}],"nextCursor":null}}'
exec sleep 30
"#,
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let start = (
            program.to_str().unwrap().into(),
            vec!["app-server".into()],
            vec![],
        );

        let got = probe(start, dir.path(), Duration::from_secs(5)).await;

        assert_eq!(got.map(|m| m.len()), Some(1));
        let request = std::fs::read_to_string(dir.path().join("request")).unwrap();
        let request: Value = serde_json::from_str(request.trim()).unwrap();
        assert_eq!(request["method"], "model/list");
        assert!(request["params"].is_object(), "{request}");
    }

    #[tokio::test]
    #[ignore = "needs Codex installed and logged in"]
    async fn installed_codex_answers_with_the_models_it_accepts() {
        let got = probe(
            ("codex".into(), vec!["app-server".into()], vec![]),
            &std::env::temp_dir(),
            Duration::from_secs(15),
        )
        .await
        .expect("Codex models");
        assert!(!got.is_empty(), "{got:?}");
    }
}
