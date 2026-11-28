//! How a work host says the state it serves was written by a newer adroit,
//! which only replacing that work host puts right.

// Recognised by its words rather than by a code of its own: work hosts
// already running say nothing else, and those are the ones to replace.
const SAID: &str = "a newer adroit has written this database";

/// What a work host says of state at `schema`, newer than its own.
pub fn newer_state(schema: i64) -> String {
    format!("{SAID} (schema {schema}); restart the work host from the installed application")
}

/// Whether `text`, a failure a work host reported, says a newer adroit wrote
/// the state it serves, however the failure was worded around it.
pub fn says_newer_state(text: impl AsRef<str>) -> bool {
    text.as_ref().contains(SAID)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_work_host_says_of_newer_state_is_recognised_inside_other_words() {
        assert!(says_newer_state(newer_state(11)));
        assert!(says_newer_state(format!(
            "cannot start Claude: {}",
            newer_state(11)
        )));
        assert!(!says_newer_state("the opening session is unknown"));
    }

    #[test]
    fn the_words_work_hosts_already_running_say_are_recognised() {
        assert!(says_newer_state(
            "a newer adroit has written this database (schema 11); restart the work host from the installed application"
        ));
    }
}
