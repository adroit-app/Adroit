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

/// A time written as GitHub and RFC 3339 write it in UTC,
/// `2026-11-13T16:00:21Z`, in milliseconds since the Unix epoch.
pub fn parse_utc(text: &str) -> Option<u64> {
    let text = text.strip_suffix('Z')?;
    let (date, clock) = text.split_once('T')?;
    let mut date = date.split('-').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let clock = clock.split('.').next()?;
    let mut clock = clock.split(':').map(str::parse::<i64>);
    let (hour, minute, second) = (
        clock.next()?.ok()?,
        clock.next()?.ok()?,
        clock.next()?.ok()?,
    );
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Days from the civil date, as Howard Hinnant's algorithm counts them.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let seconds = days * 86_400 + hour * 3600 + minute * 60 + second;
    u64::try_from(seconds).ok().map(|s| s * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_utc_time_reads_as_milliseconds_since_the_epoch() {
        assert_eq!(parse_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_utc("2026-11-13T16:00:21Z"), Some(1_794_585_621_000));
        assert_eq!(parse_utc("2024-02-29T12:00:00.5Z"), Some(1_709_208_000_000));
        assert_eq!(parse_utc("2026-11-13 16:00:21"), None);
        assert_eq!(parse_utc(""), None);
    }

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
