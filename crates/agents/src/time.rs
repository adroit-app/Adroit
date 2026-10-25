//! The times the agents write down, read as milliseconds since the Unix
//! epoch.

use serde_json::Value;

/// Days since the Unix epoch of the civil date `y`-`m`-`d`, after Howard
/// Hinnant's `days_from_civil`.
pub(crate) fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date, as year, month and day, `days` after the Unix epoch, after
/// Howard Hinnant's `civil_from_days`.
pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Milliseconds since the Unix epoch of a UTC time such as
/// `2026-10-19T14:13:00.648Z`.
pub(crate) fn epoch_ms(s: &str) -> Option<u64> {
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let frac = s
        .get(19..)
        .and_then(|r| r.strip_prefix('.'))
        .map(|r| r.trim_end_matches('Z'))
        .filter(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()))
        .map_or(0, |r| {
            let r = &r[..r.len().min(3)];
            r.parse::<i64>().unwrap_or(0) * 10_i64.pow(3 - r.len() as u32)
        });
    let days = days_from_civil(y, mo, d);
    let ms = ((days * 24 + h) * 60 + mi) * 60_000 + sec * 1000 + frac;
    u64::try_from(ms).ok()
}

/// [`epoch_ms`] of a transcript's `timestamp` field.
pub(crate) fn timestamp_ms(value: &Value) -> Option<u64> {
    value.as_str().and_then(epoch_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_civil_date_counts_its_days_from_the_epoch() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(2026, 10, 19), 20_745);
    }

    #[test]
    fn days_from_the_epoch_give_back_their_civil_date() {
        for (y, m, d) in [
            (1970, 1, 1),
            (1969, 12, 31),
            (2000, 2, 29),
            (2000, 3, 1),
            (2026, 10, 19),
        ] {
            assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }
    }

    #[test]
    fn a_utc_time_is_read_to_the_millisecond() {
        assert_eq!(epoch_ms("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(epoch_ms("2000-03-01T00:00:00Z"), Some(951_868_800_000));
        assert_eq!(
            epoch_ms("2026-10-19T14:13:00.648Z"),
            Some(1_792_419_180_648)
        );
        assert_eq!(epoch_ms("yesterday"), None);
    }
}
