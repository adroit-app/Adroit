//! The status line: what the window last told the user, and the notices
//! that go again after a while.

use std::time::Duration;

use crate::effect::Effect;
use crate::status::after_read;

/// How long a notice stays in the status line. Errors and work under way stay
/// until something replaces them.
pub const NOTICE_FOR: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatusLine {
    pub line: Option<String>,
    /// How many notices it has shown.
    notices: u64,
}

pub(crate) fn set(status: &mut StatusLine, line: Option<String>) -> Vec<Effect> {
    status.line = line;
    Vec::new()
}

/// `text` shown for a while: what just happened, or a hint for what was just
/// tried, which would read as the state of things if it stayed.
pub(crate) fn notice(status: &mut StatusLine, text: String) -> Vec<Effect> {
    status.notices += 1;
    status.line = Some(text.clone());
    vec![Effect::LapseNotice {
        notice: status.notices,
        text,
        after: NOTICE_FOR,
    }]
}

/// Notice `notice`, saying `text`, lapsed: it goes, unless a notice came
/// after it or the line says something else now.
pub(crate) fn lapsed(status: &mut StatusLine, notice: u64, text: &str) -> Vec<Effect> {
    if status.notices == notice && status.line.as_deref() == Some(text) {
        status.line = None;
    }
    Vec::new()
}

/// A source that last failed with `last` now fails with `now`, or reads
/// with `None`.
pub(crate) fn failing(
    status: &mut StatusLine,
    last: Option<&str>,
    now: Option<&str>,
) -> Vec<Effect> {
    if let Some(line) = after_read(status.line.as_deref(), last, now) {
        status.line = line;
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::NOTICE_FOR;
    use crate::effect::Effect;
    use crate::msg::Msg;
    use crate::scenario::Scenario;

    fn notice(text: &str) -> Msg {
        Msg::Notice(text.into())
    }

    fn lapsed(notice: u64, text: &str) -> Msg {
        Msg::NoticeLapsed {
            notice,
            text: text.into(),
        }
    }

    #[test]
    fn a_notice_asks_to_be_told_when_it_lapses() {
        let run = Scenario::new().send(notice("Copied a line"));
        assert_eq!(
            run.last_effects(),
            [Effect::LapseNotice {
                notice: 1,
                text: "Copied a line".into(),
                after: NOTICE_FOR,
            }]
        );
    }

    #[test]
    fn a_notice_goes_once_it_lapses_unless_something_replaced_it() {
        // (messages, line shown)
        let cases: Vec<(Vec<Msg>, Option<&str>)> = vec![
            (vec![notice("a"), lapsed(1, "a")], None),
            (vec![notice("a"), notice("b"), lapsed(1, "a")], Some("b")),
            (vec![notice("a"), notice("a"), lapsed(1, "a")], Some("a")),
            (
                vec![
                    notice("a"),
                    Msg::Status(Some("failed".into())),
                    lapsed(1, "a"),
                ],
                Some("failed"),
            ),
            (
                vec![Msg::Status(Some("taking over".into()))],
                Some("taking over"),
            ),
            (vec![notice("a"), Msg::Status(None)], None),
        ];
        for (msgs, line) in cases {
            let run = Scenario::new().send_all(msgs.clone());
            assert_eq!(run.state().chrome.status.line.as_deref(), line, "{msgs:?}");
        }
    }

    #[test]
    fn a_failing_source_shows_its_error_and_takes_back_only_its_own() {
        let failing = |last: Option<&str>, now: Option<&str>| Msg::Failing {
            last: last.map(str::to_string),
            now: now.map(str::to_string),
        };
        // (messages, line shown)
        let cases: Vec<(Vec<Msg>, Option<&str>)> = vec![
            (vec![failing(None, Some("bad"))], Some("bad")),
            (
                vec![failing(None, Some("bad")), failing(Some("bad"), None)],
                None,
            ),
            (
                vec![
                    failing(None, Some("bad")),
                    notice("sent"),
                    failing(Some("bad"), None),
                ],
                Some("sent"),
            ),
        ];
        for (msgs, line) in cases {
            let run = Scenario::new().send_all(msgs.clone());
            assert_eq!(run.state().chrome.status.line.as_deref(), line, "{msgs:?}");
        }
    }
}
