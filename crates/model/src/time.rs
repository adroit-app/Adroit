//! The clock, read the one way every part of adroit stamps times.

use std::time::{SystemTime, UNIX_EPOCH};

/// Now, in milliseconds since the Unix epoch; 0 on a clock set before it.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_counted_in_milliseconds_since_the_epoch() {
        // 2026-01-01T00:00:00Z, well after the epoch in seconds would read.
        assert!(now_ms() > 1_767_225_600_000);
        let before = now_ms();
        assert!(now_ms() >= before);
    }
}
