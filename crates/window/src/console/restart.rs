//! When the window starts one of its Neovims again once it quits.

use std::time::Duration;

/// How long a Neovim has to live for its quitting to be taken as the user's,
/// and not as one that cannot start.
const SETTLE: Duration = Duration::from_secs(2);

/// The wait before the first start again of a Neovim that quit as it
/// started, doubled each time it does so again.
const FIRST_WAIT: Duration = Duration::from_secs(1);

/// The longest wait between starts of a Neovim that keeps quitting as it
/// starts.
const LONGEST_WAIT: Duration = Duration::from_secs(60);

/// Counts the starts in a row that ended as they began, to wait longer
/// before each start after them.
#[derive(Debug, Default)]
pub(crate) struct Restarts {
    failed: u32,
    /// Starts in a row ended by a configuration saved as Neovim started.
    saved_at_start: u32,
}

/// How long a Neovim has to live for a save of its configuration to be taken
/// as the user's, and not as the configuration writing itself as it starts.
const SAVED_SETTLE: Duration = Duration::from_secs(1);

/// How many starts in a row a configuration may end by saving itself before
/// the next start waits.
const SAVED_AT_ONCE: u32 = 2;

impl Restarts {
    /// How long to wait before starting the Neovim again, once it lived for
    /// `lived`, or could not start at all with [`None`]: no time after it
    /// ran a while, else a wait that doubles with each failure in a row.
    pub(crate) fn wait(&mut self, lived: Option<Duration>) -> Duration {
        if lived.is_some_and(|l| l >= SETTLE) {
            self.failed = 0;
            return Duration::ZERO;
        }
        let wait = FIRST_WAIT
            .saturating_mul(1 << self.failed.min(16))
            .min(LONGEST_WAIT);
        self.failed += 1;
        wait
    }

    /// How long to wait before starting the Neovim again, once it lived for
    /// `lived` and was quit as its configuration was saved: no time, unless
    /// saves keep ending it as it starts, as a configuration that writes
    /// itself would, when it waits as one that quits as it starts.
    pub(crate) fn saved(&mut self, lived: Duration) -> Duration {
        if lived >= SAVED_SETTLE {
            self.saved_at_start = 0;
            self.failed = 0;
            return Duration::ZERO;
        }
        self.saved_at_start += 1;
        if self.saved_at_start <= SAVED_AT_ONCE {
            return Duration::ZERO;
        }
        self.wait(Some(lived))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUICK: Option<Duration> = Some(Duration::from_millis(100));
    const LONG: Option<Duration> = Some(Duration::from_secs(30));

    #[test]
    fn a_neovim_that_ran_a_while_starts_again_at_once() {
        assert_eq!(Restarts::default().wait(LONG), Duration::ZERO);
    }

    #[test]
    fn a_neovim_that_quits_as_it_starts_starts_again_later_each_time_up_to_a_minute() {
        let mut r = Restarts::default();
        let waits: Vec<u64> = (0..10).map(|_| r.wait(QUICK).as_secs()).collect();
        assert_eq!(waits, [1, 2, 4, 8, 16, 32, 60, 60, 60, 60]);
    }

    #[test]
    fn a_neovim_that_cannot_start_is_tried_again_as_one_that_quits_as_it_starts() {
        let mut r = Restarts::default();
        assert_eq!(r.wait(None), Duration::from_secs(1));
        assert_eq!(r.wait(QUICK), Duration::from_secs(2));
    }

    #[test]
    fn a_run_that_lasts_forgets_the_failures_before_it() {
        let mut r = Restarts::default();
        r.wait(QUICK);
        r.wait(QUICK);
        assert_eq!(r.wait(LONG), Duration::ZERO);
        assert_eq!(r.wait(QUICK), Duration::from_secs(1));
    }

    #[test]
    fn a_neovim_started_again_on_a_saved_configuration_starts_at_once() {
        let mut r = Restarts::default();
        for _ in 0..10 {
            assert_eq!(r.saved(Duration::from_secs(5)), Duration::ZERO);
        }
    }

    #[test]
    fn a_configuration_saved_as_neovim_starts_waits_once_it_keeps_doing_so() {
        let mut r = Restarts::default();
        let quick = Duration::from_millis(300);
        let waits: Vec<u64> = (0..6).map(|_| r.saved(quick).as_secs()).collect();
        assert_eq!(waits, [0, 0, 1, 2, 4, 8]);
        // A save the user makes later starts it at once again.
        assert_eq!(r.saved(Duration::from_secs(5)), Duration::ZERO);
        assert_eq!(r.saved(quick), Duration::ZERO);
    }

    #[test]
    fn a_neovim_failing_for_ever_never_waits_past_the_longest_wait() {
        let mut r = Restarts::default();
        for _ in 0..100 {
            assert!(r.wait(None) <= LONGEST_WAIT);
        }
    }
}
