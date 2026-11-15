//! A cursor over a list of `len` rows, by its index: moved round the ends or
//! stopped at them.

/// The index `delta` places from `at`, going round at either end. 0 in an
/// empty list.
pub fn wrap(at: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (at as isize + delta).rem_euclid(len as isize) as usize
}

/// The index `delta` places from `at`, stopped at the first and the last.
/// 0 in an empty list.
pub fn clamp(at: usize, delta: isize, len: usize) -> usize {
    let last = len.saturating_sub(1) as isize;
    (at as isize + delta).clamp(0, last) as usize
}

/// `at`, or the last index when the list is shorter. 0 in an empty list.
pub fn within(at: usize, len: usize) -> usize {
    at.min(len.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_goes_round_both_ends() {
        assert_eq!(wrap(2, 1, 3), 0);
        assert_eq!(wrap(0, -1, 3), 2);
        assert_eq!(wrap(1, 4, 3), 2);
        assert_eq!(wrap(1, -1, 3), 0);
    }

    #[test]
    fn wrapping_in_an_empty_list_stays_at_the_start() {
        assert_eq!(wrap(0, 1, 0), 0);
        assert_eq!(wrap(0, -1, 0), 0);
    }

    #[test]
    fn clamping_stops_at_both_ends() {
        assert_eq!(clamp(0, 1, 3), 1);
        assert_eq!(clamp(2, 1, 3), 2);
        assert_eq!(clamp(0, -1, 3), 0);
        assert_eq!(clamp(1, -5, 3), 0);
        assert_eq!(clamp(1, 5, 3), 2);
    }

    #[test]
    fn clamping_in_an_empty_list_stays_at_the_start() {
        assert_eq!(clamp(0, 1, 0), 0);
        assert_eq!(clamp(0, -1, 0), 0);
    }

    #[test]
    fn within_keeps_an_index_in_the_list_and_pulls_one_past_it_to_the_last() {
        assert_eq!(within(1, 3), 1);
        assert_eq!(within(3, 3), 2);
        assert_eq!(within(5, 0), 0);
    }
}
