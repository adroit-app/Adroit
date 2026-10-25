//! How much of the account's usage limits is used, asked of Claude Code
//! itself: the limits its `/usage` command shows, as the release installed
//! reads them.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use super::control;
use crate::Started;

pub use crate::usage::*;

/// What one line Claude Code printed says about the usage asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// The limits; none for an account without any.
    Limits(Vec<Limit>),
    /// Claude Code answered, but not with usage.
    Refused,
    /// Not the answer: another message, or no JSON at all.
    Other,
}

/// The one line written to Claude Code: a `get_usage` control request, never
/// a user message, so no turn runs and no tokens are spent. It leaves out the
/// account's habits, which Claude Code would read from every transcript.
const REQUEST: &str = "{\"request_id\":\"adroit\",\"type\":\"control_request\",\"request\":{\"subtype\":\"get_usage\",\"skip_behaviors\":true}}\n";

/// Read one line of Claude Code's output.
pub fn reply(line: &str) -> Reply {
    let answer = match control::answer(line) {
        control::Answer::Success(v) => v,
        control::Answer::Refused => return Reply::Refused,
        control::Answer::Other => return Reply::Other,
    };
    let Some(list) = answer["rate_limits"]["limits"].as_array() else {
        return Reply::Limits(Vec::new());
    };
    Reply::Limits(list.iter().filter_map(limit).collect())
}

fn limit(v: &Value) -> Option<Limit> {
    let kind = v["kind"].as_str()?;
    let window = match kind {
        "session" => Window::Session,
        "weekly_all" => Window::Week,
        "weekly_scoped" => match scope_name(&v["scope"]) {
            Some(name) => Window::WeekOf(name.to_string()),
            None => Window::Other(kind.to_string()),
        },
        other => Window::Other(other.to_string()),
    };
    let severity = match v["severity"].as_str() {
        Some("normal") | None => Severity::Normal,
        Some("warning") => Severity::Warning,
        Some(_) => Severity::Critical,
    };
    Some(Limit {
        window,
        percent: v["percent"].as_f64()?,
        resets_ms: v["resets_at"].as_str().and_then(epoch_ms),
        severity,
    })
}

fn scope_name(scope: &Value) -> Option<&str> {
    ["model", "surface"]
        .iter()
        .find_map(|k| scope[k]["display_name"].as_str())
}

/// `text`, an RFC 3339 time with its offset, in milliseconds since the Unix
/// epoch.
fn epoch_ms(text: &str) -> Option<u64> {
    let (date, rest) = text.split_once(['T', 't', ' '])?;
    let mut ymd = date.splitn(3, '-').map(str::parse::<i64>);
    let (y, m, d) = (ymd.next()?.ok()?, ymd.next()?.ok()?, ymd.next()?.ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let (clock, offset_s) = if let Some(c) = rest.strip_suffix(['Z', 'z']) {
        (c, 0)
    } else {
        let at = rest.rfind(['+', '-'])?;
        let (c, off) = rest.split_at(at);
        let (oh, om) = off[1..].split_once(':')?;
        let secs = oh.parse::<i64>().ok()? * 3600 + om.parse::<i64>().ok()? * 60;
        (c, if off.starts_with('-') { -secs } else { secs })
    };
    let (whole, frac) = clock.split_once('.').unwrap_or((clock, ""));
    let mut hms = whole.splitn(3, ':').map(str::parse::<i64>);
    let (h, mi, s) = (hms.next()?.ok()?, hms.next()?.ok()?, hms.next()?.ok()?);
    let ms = match frac {
        "" => 0,
        f if f.bytes().all(|b| b.is_ascii_digit()) => format!("{f:0<3}")[..3].parse().ok()?,
        _ => return None,
    };
    let days = crate::time::days_from_civil(y, m, d);
    let secs = days * 86_400 + h * 3600 + mi * 60 + s - offset_s;
    u64::try_from(secs * 1000 + ms).ok()
}

/// Ask Claude Code, started by `start` in `dir`, how much of the account's limits
/// is used, giving up after `timeout`. `None` for every failure: the request
/// is unversioned, and a release that changes it leaves the limits unknown
/// rather than guessed.
pub async fn probe(
    start: impl FnOnce(Vec<String>) -> Started,
    dir: &Path,
    timeout: Duration,
) -> Option<Vec<Limit>> {
    control::ask(start, dir, REQUEST, timeout, |line| match reply(line) {
        Reply::Limits(list) => Some(Some(list)),
        Reply::Refused => Some(None),
        Reply::Other => None,
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{control_error, control_success, launching, program};

    /// Cut down from what Claude Code 2.1.303 answers for a Max account.
    fn answer() -> String {
        control_success(
            r#"{"subscription_type":"max","rate_limits_available":true,"rate_limits":{"five_hour":{"utilization":3,"resets_at":"2026-10-19T19:19:59.661431+00:00"},"limits":[{"kind":"session","group":"session","percent":3,"severity":"normal","resets_at":"2026-10-19T19:19:59.661431+00:00","scope":null,"is_active":false},{"kind":"weekly_all","group":"weekly","percent":86,"severity":"warning","resets_at":"2026-10-22T13:59:59.661454+00:00","scope":null,"is_active":true},{"kind":"weekly_scoped","group":"weekly","percent":0,"severity":"normal","resets_at":"2026-10-22T14:00:00+00:00","scope":{"model":{"id":null,"display_name":"Fable"},"surface":null},"is_active":false}]}}"#,
        )
    }

    fn answer_with(limits: &str) -> String {
        control_success(&format!(r#"{{"rate_limits":{{"limits":{limits}}}}}"#))
    }

    #[test]
    fn the_answer_lists_the_limits_usage_shows_in_its_order() {
        let Reply::Limits(got) = reply(&answer()) else {
            panic!("no limits");
        };
        assert_eq!(
            got,
            [
                Limit {
                    window: Window::Session,
                    percent: 3.0,
                    resets_ms: Some(1_792_437_599_661),
                    severity: Severity::Normal,
                },
                Limit {
                    window: Window::Week,
                    percent: 86.0,
                    resets_ms: Some(1_792_677_599_661),
                    severity: Severity::Warning,
                },
                Limit {
                    window: Window::WeekOf("Fable".into()),
                    percent: 0.0,
                    resets_ms: Some(1_792_677_600_000),
                    severity: Severity::Normal,
                },
            ]
        );
    }

    #[test]
    fn a_limit_scoped_to_a_surface_is_named_after_it() {
        let line = answer_with(
            r#"[{"kind":"weekly_scoped","percent":5,"severity":"normal","scope":{"model":null,"surface":{"display_name":"Cowork"}}}]"#,
        );
        let Reply::Limits(got) = reply(&line) else {
            panic!("no limits");
        };
        assert_eq!(got[0].window, Window::WeekOf("Cowork".into()));
    }

    #[test]
    fn a_kind_adroit_does_not_know_keeps_claude_codes_name_for_it() {
        let line = answer_with(r#"[{"kind":"monthly_all","percent":12.5,"severity":"normal"}]"#);
        let Reply::Limits(got) = reply(&line) else {
            panic!("no limits");
        };
        assert_eq!(got[0].window, Window::Other("monthly_all".into()));
        assert_eq!(got[0].percent, 12.5);
        assert_eq!(got[0].resets_ms, None);
    }

    #[test]
    fn a_severity_past_warning_is_critical() {
        let line = answer_with(r#"[{"kind":"session","percent":100,"severity":"exceeded"}]"#);
        let Reply::Limits(got) = reply(&line) else {
            panic!("no limits");
        };
        assert_eq!(got[0].severity, Severity::Critical);
    }

    #[test]
    fn a_limit_without_a_kind_or_a_percent_is_passed_over() {
        let line = answer_with(
            r#"[{"percent":3,"severity":"normal"},{"kind":"session","severity":"normal"},{"kind":"weekly_all","percent":1,"severity":"normal"}]"#,
        );
        let Reply::Limits(got) = reply(&line) else {
            panic!("no limits");
        };
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].window, Window::Week);
    }

    #[test]
    fn an_account_without_limits_has_none() {
        let line = control_success(
            r#"{"subscription_type":null,"rate_limits_available":false,"rate_limits":null}"#,
        );
        assert_eq!(reply(&line), Reply::Limits(Vec::new()));
    }

    #[test]
    fn an_error_answer_is_a_refusal() {
        let line = control_error("Unknown subtype");
        assert_eq!(reply(&line), Reply::Refused);
    }

    #[test]
    fn other_messages_and_garbage_are_passed_over() {
        assert_eq!(reply(r#"{"type":"system","subtype":"init"}"#), Reply::Other);
        assert_eq!(reply("not json"), Reply::Other);
    }

    #[test]
    fn the_request_asks_only_for_usage_without_the_accounts_habits() {
        let sent: Value = serde_json::from_str(REQUEST.trim_end()).unwrap();
        assert_eq!(sent["type"], "control_request");
        assert_eq!(sent["request"]["subtype"], "get_usage");
        assert_eq!(sent["request"]["skip_behaviors"], true);
        assert_eq!(sent["request_id"], control::REQUEST_ID);
        assert!(REQUEST.ends_with('\n'));
    }

    #[test]
    fn reset_times_are_read_in_any_offset_and_to_the_millisecond() {
        assert_eq!(epoch_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch_ms("1970-01-01T09:00:00+09:00"), Some(0));
        assert_eq!(epoch_ms("1969-12-31T19:00:00.5-05:00"), Some(500));
        assert_eq!(epoch_ms("2000-03-01T00:00:00Z"), Some(951_868_800_000));
        assert_eq!(
            epoch_ms("2026-10-22T14:00:00+00:00"),
            Some(1_792_677_600_000)
        );
    }

    #[test]
    fn a_reset_time_that_is_not_one_is_none() {
        assert_eq!(epoch_ms(""), None);
        assert_eq!(epoch_ms("tomorrow"), None);
        assert_eq!(epoch_ms("2026-13-01T00:00:00Z"), None);
        assert_eq!(epoch_ms("2026-10-22T14:00:00"), None);
    }

    #[tokio::test]
    async fn the_probe_writes_the_usage_request_and_reads_the_answer() {
        let answer = answer();
        let (_dir, p) = program(
            "claude",
            &format!(
                "IFS= read -r line\nprintf '%s\\n' \"$line\" > \"$(dirname \"$0\")/stdin\"\necho '{{\"type\":\"system\"}}'\necho '{answer}'\nexec sleep 30"
            ),
        );
        let dir = p.parent().unwrap();
        let got = probe(launching(&p), dir, Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(got.len(), 3);
        let written = std::fs::read_to_string(dir.join("stdin")).unwrap();
        assert_eq!(written, REQUEST);
    }

    #[tokio::test]
    async fn a_refusal_leaves_the_limits_unknown() {
        let refusal = control_error("Unknown subtype");
        let (_dir, p) = program(
            "claude",
            &format!("read line\necho '{refusal}'\nexec sleep 30"),
        );
        let got = probe(launching(&p), p.parent().unwrap(), Duration::from_secs(5)).await;
        assert_eq!(got, None);
    }

    /// Needs a `claude` on PATH that is logged in to a plan with limits, and
    /// the network, which CI has not.
    #[tokio::test]
    #[ignore = "needs Claude Code installed and logged in to a plan with limits"]
    async fn claude_code_answers_with_the_session_limit() {
        let got = probe(
            launching(Path::new("claude")),
            &std::env::temp_dir(),
            PROBE_TIMEOUT,
        )
        .await
        .expect("an answer");
        assert!(got.iter().any(|l| l.window == Window::Session), "{got:?}");
    }
}
