//! How far the window is zoomed, a level at a time.

/// The zoom levels, from the smallest to the largest; 1.0 is as designed.
pub const LEVELS: [f64; 9] = [0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75];

/// What a key asks of the zoom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    In,
    Out,
    Reset,
}

/// The zoom after `step` from `now`: the next level up or down, staying at
/// either end, or back to 1.0.
pub fn next(now: f64, step: Step) -> f64 {
    match step {
        Step::Reset => 1.0,
        Step::In => LEVELS
            .iter()
            .copied()
            .find(|l| *l > now + 1e-9)
            .unwrap_or(LEVELS[LEVELS.len() - 1]),
        Step::Out => LEVELS
            .iter()
            .rev()
            .copied()
            .find(|l| *l < now - 1e-9)
            .unwrap_or(LEVELS[0]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_steps_through_the_levels_and_stops_at_either_end() {
        assert_eq!(next(1.0, Step::In), 1.1);
        assert_eq!(next(1.0, Step::Out), 0.9);
        assert_eq!(next(1.75, Step::In), 1.75);
        assert_eq!(next(0.67, Step::Out), 0.67);
        assert_eq!(next(1.5, Step::Reset), 1.0);
    }
}
