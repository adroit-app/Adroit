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
}

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
    fn a_neovim_failing_for_ever_never_waits_past_the_longest_wait() {
        let mut r = Restarts::default();
        for _ in 0..100 {
            assert!(r.wait(None) <= LONGEST_WAIT);
        }
    }
}
