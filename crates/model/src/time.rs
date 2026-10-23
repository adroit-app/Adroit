//! The clock, read the one way every part of adroit stamps times.

use std::time::{SystemTime, UNIX_EPOCH};

/// Now, in milliseconds since the Unix epoch; 0 on a clock set before it.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// `ms` milliseconds back, in the largest whole unit.
pub fn ago(ms: u64) -> String {
    let min = ms / 60_000;
    match min {
        0 => "just now".into(),
        1..60 => format!("{min} min ago"),
        60..1440 => format!("{} h ago", min / 60),
        _ => match min / 1440 {
            1 => "1 day ago".into(),
            d => format!("{d} days ago"),
        },
    }
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

    #[test]
    fn how_long_ago_reads_in_the_largest_whole_unit() {
        assert_eq!(ago(30_000), "just now");
        assert_eq!(ago(90_000), "1 min ago");
        assert_eq!(ago(3 * 3_600_000 + 1), "3 h ago");
        assert_eq!(ago(2 * 86_400_000), "2 days ago");
    }
}
