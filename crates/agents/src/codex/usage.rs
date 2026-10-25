use crate::usage::{Limit, Severity, Window};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

fn limits(result: &Value) -> Option<Vec<Limit>> {
    let buckets: Vec<(Option<&str>, &Value)> = match result["rateLimitsByLimitId"].as_object() {
        Some(map) if !map.is_empty() => map
            .iter()
            .map(|(id, bucket)| (Some(bucket["limitName"].as_str().unwrap_or(id)), bucket))
            .collect(),
        _ => vec![(None, result.get("rateLimits").filter(|v| v.is_object())?)],
    };
    let mut found = Vec::new();
    for (name, bucket) in buckets {
        for key in ["primary", "secondary"] {
            let v = &bucket[key];
            if v.is_null() {
                continue;
            }
            let Some(percent) = v["usedPercent"]
                .as_f64()
                .filter(|p| p.is_finite() && *p >= 0.0)
            else {
                continue;
            };
            let window = match (v["windowDurationMins"].as_u64(), name) {
                (Some(10080), None) => Window::Week,
                (Some(10080), Some(name)) => Window::WeekOf(name.into()),
                (duration, name) => {
                    let span = match duration {
                        Some(min) if min > 0 && min % 60 == 0 => format!("{} h", min / 60),
                        Some(min) => format!("{min} min"),
                        None => key.into(),
                    };
                    Window::Other(match name {
                        Some(name) => format!("{span} · {name}"),
                        None => span,
                    })
                }
            };
            found.push(Limit {
                window,
                percent,
                resets_ms: v["resetsAt"].as_u64().and_then(|s| s.checked_mul(1000)),
                severity: if percent >= 90.0 {
                    Severity::Critical
                } else if percent >= 80.0 {
                    Severity::Warning
                } else {
                    Severity::Normal
                },
            });
        }
    }
    Some(found)
}

/// Ask the installed Codex for account limits without starting a thread or turn.
pub(super) async fn probe(
    start: crate::Started,
    dir: &Path,
    timeout: Duration,
) -> Option<Vec<Limit>> {
    limits(&super::rpc::request(start, dir, timeout, "account/rateLimits/read", Value::Null).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{program, started};
    use serde_json::json;

    #[test]
    fn codex_windows_keep_their_duration_usage_and_reset_time() {
        let got = limits(&json!({"rateLimits": {
            "primary": {"usedPercent": 23, "windowDurationMins": 300, "resetsAt": 1792500000_u64},
            "secondary": {"usedPercent": 92, "windowDurationMins": 10080, "resetsAt": 1792600000_u64}
        }})).unwrap();
        assert_eq!(
            got,
            [
                Limit {
                    window: Window::Other("5 h".into()),
                    percent: 23.0,
                    resets_ms: Some(1_792_500_000_000),
                    severity: Severity::Normal
                },
                Limit {
                    window: Window::Week,
                    percent: 92.0,
                    resets_ms: Some(1_792_600_000_000),
                    severity: Severity::Critical
                },
            ]
        );
    }

    #[test]
    fn multiple_buckets_are_named_and_the_legacy_copy_is_not_duplicated() {
        let bucket = |name: &str| {
            json!({"limitName": name,
            "primary": {"usedPercent": 85, "windowDurationMins": 60, "resetsAt": null}})
        };
        let got = limits(&json!({"rateLimits": bucket("Codex"),
            "rateLimitsByLimitId": {"codex": bucket("Codex"), "other": bucket("Other model")}
        }))
        .unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].window, Window::Other("1 h · Codex".into()));
        assert_eq!(got[1].window, Window::Other("1 h · Other model".into()));
        assert_eq!(got[0].severity, Severity::Warning);
        assert_eq!(got[0].resets_ms, None);
    }

    #[test]
    fn missing_limits_are_unknown_and_empty_limits_are_empty() {
        assert_eq!(limits(&json!({})), None);
        assert_eq!(
            limits(&json!({"rateLimits": {"primary": null, "secondary": null}})),
            Some(vec![])
        );
    }

    /// Codex running `body`, a shell script, started as an app server.
    fn fake(body: &str) -> (tempfile::TempDir, crate::Started) {
        let (dir, codex) = program("codex", body);
        (dir, started(&codex, &["app-server"]))
    }

    #[tokio::test]
    async fn the_probe_initializes_then_reads_limits_without_starting_a_turn() {
        let (dir, start) = fake(
            r#"
test "$1" = app-server || exit 1
IFS= read -r line
printf '%s\n' "$line" > requests
echo '{"id":"adroit-init","result":{}}'
IFS= read -r line
printf '%s\n' "$line" >> requests
IFS= read -r line
printf '%s\n' "$line" >> requests
echo '{"method":"account/updated","params":{}}'
echo '{"id":"unrelated","result":{}}'
echo '{"id":"adroit-query","result":{"rateLimits":{"primary":{"usedPercent":25,"windowDurationMins":300}}}}'
exec sleep 30
"#,
        );
        let got = probe(start, dir.path(), Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].percent, 25.0);
        let requests = std::fs::read_to_string(dir.path().join("requests")).unwrap();
        let methods: Vec<_> = requests
            .lines()
            .map(|line| {
                serde_json::from_str::<Value>(line).unwrap()["method"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(
            methods,
            ["initialize", "initialized", "account/rateLimits/read"]
        );
    }

    #[tokio::test]
    async fn an_error_or_closed_output_leaves_limits_unknown() {
        for body in [
            "exit 0",
            "read line\necho '{\"id\":\"adroit-init\",\"error\":{\"code\":-1}}'\nexec sleep 30",
            "read line\necho '{\"id\":\"adroit-init\",\"result\":{}}'\nread line\nread line\necho '{\"id\":\"adroit-query\",\"error\":{\"code\":-1}}'\nexec sleep 30",
        ] {
            let (dir, start) = fake(body);
            assert_eq!(probe(start, dir.path(), Duration::from_secs(5)).await, None);
        }
    }

    #[tokio::test]
    async fn a_probe_that_stops_answering_times_out() {
        let (dir, start) = fake("exec sleep 30");
        assert_eq!(
            probe(start, dir.path(), Duration::from_millis(50)).await,
            None
        );
    }

    #[tokio::test]
    #[ignore = "needs Codex installed and logged in to a plan with limits"]
    async fn installed_codex_answers_with_account_limits() {
        let got = probe(
            started(Path::new("codex"), &["app-server"]),
            &std::env::temp_dir(),
            crate::usage::PROBE_TIMEOUT,
        )
        .await
        .expect("Codex account limits");
        assert!(!got.is_empty(), "{got:?}");
    }
}
