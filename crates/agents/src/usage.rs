//! How much of an account's usage limits is used, kept for a while as
//! asking costs a process and a request over the network.

use std::time::{Duration, Instant};

pub use adroit_model::usage::*;

/// How long an agent is given to answer for the limits, longer than for its
/// commands as it asks the account's usage over the network.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// How long the limits are shown before they are asked for again. Asking
/// starts a process and makes the agent ask the network, while the limits
/// move slowly against windows of hours and days.
pub const USAGE_TTL: Duration = Duration::from_secs(5 * 60);

/// The limits last heard, with when they were asked for: old limits are shown
/// until fresh ones arrive, and they are asked for once at a time.
#[derive(Debug, Default)]
pub struct Fresh {
    limits: Option<Vec<Limit>>,
    asked: Option<Instant>,
    asking: bool,
}

impl Fresh {
    /// The limits last heard, however old; `None` before the first answer.
    pub fn get(&self) -> Option<&[Limit]> {
        self.limits.as_deref()
    }

    /// Whether to ask now, at `now`: never asked, or asked longer than
    /// [`USAGE_TTL`] ago, and not being asked. A yes counts as asking until
    /// [`Fresh::settle`].
    pub fn want(&mut self, now: Instant) -> bool {
        let stale = self
            .asked
            .is_none_or(|at| now.saturating_duration_since(at) >= USAGE_TTL);
        if self.asking || !stale {
            return false;
        }
        self.asking = true;
        true
    }

    /// Take the answer, at `now`. A failure keeps the limits there were, and
    /// waits the same time as an answer before asking again.
    pub fn settle(&mut self, now: Instant, got: Option<Vec<Limit>>) {
        self.asking = false;
        self.asked = Some(now);
        if got.is_some() {
            self.limits = got;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limits_are_asked_for_once_until_they_are_answered() {
        let mut fresh = Fresh::default();
        let t = Instant::now();
        assert!(fresh.want(t));
        assert!(!fresh.want(t + USAGE_TTL * 2));
        assert_eq!(fresh.get(), None);
    }

    #[test]
    fn fresh_limits_are_shown_without_asking_again() {
        let mut fresh = Fresh::default();
        let t = Instant::now();
        fresh.want(t);
        fresh.settle(t, Some(Vec::new()));
        assert!(!fresh.want(t + USAGE_TTL - Duration::from_secs(1)));
        assert_eq!(fresh.get(), Some(&[][..]));
    }

    #[test]
    fn stale_limits_are_asked_for_again_once_and_shown_meanwhile() {
        let mut fresh = Fresh::default();
        let t = Instant::now();
        let limit = |percent| Limit {
            window: Window::Session,
            percent,
            resets_ms: None,
            severity: Severity::Normal,
        };
        fresh.want(t);
        fresh.settle(t, Some(vec![limit(3.0)]));
        let later = t + USAGE_TTL;
        assert!(fresh.want(later));
        assert!(!fresh.want(later));
        assert_eq!(fresh.get(), Some(&[limit(3.0)][..]));
        fresh.settle(later, Some(vec![limit(9.0)]));
        assert_eq!(fresh.get(), Some(&[limit(9.0)][..]));
    }

    #[test]
    fn a_failure_keeps_the_limits_there_were_and_waits_before_asking_again() {
        let mut fresh = Fresh::default();
        let t = Instant::now();
        fresh.want(t);
        fresh.settle(t, Some(Vec::new()));
        let later = t + USAGE_TTL;
        fresh.want(later);
        fresh.settle(later, None);
        assert_eq!(fresh.get(), Some(&[][..]));
        assert!(!fresh.want(later + Duration::from_secs(1)));
        assert!(fresh.want(later + USAGE_TTL));
    }
}
