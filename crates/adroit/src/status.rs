//! The rule for the status line when a source of errors, such as the key
//! bindings file, fails and recovers.

/// What the status line becomes when a source that last failed with `last`
/// now fails with `now`, or reads with `None`; `None` leaves it as it is.
/// A recovered source takes back only its own error, never a message shown
/// since.
pub fn after_read(
    line: Option<&str>,
    last: Option<&str>,
    now: Option<&str>,
) -> Option<Option<String>> {
    match now {
        _ if now == last => None,
        Some(error) => Some(Some(error.to_string())),
        None if line == last => Some(None),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_shows_once_and_goes_once_the_source_reads() {
        assert_eq!(
            after_read(None, None, Some("bad")),
            Some(Some("bad".into()))
        );
        assert_eq!(after_read(Some("bad"), Some("bad"), Some("bad")), None);
        assert_eq!(after_read(Some("bad"), Some("bad"), None), Some(None));
    }

    #[test]
    fn a_message_shown_since_the_error_stays_when_the_source_reads() {
        assert_eq!(after_read(Some("sent"), Some("bad"), None), None);
    }

    #[test]
    fn a_new_error_from_the_same_source_replaces_its_last() {
        assert_eq!(
            after_read(Some("bad"), Some("bad"), Some("worse")),
            Some(Some("worse".into()))
        );
    }
}
