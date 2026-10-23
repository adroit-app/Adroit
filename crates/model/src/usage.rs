//! How much of the account's usage limits is used.

/// The span a limit counts usage over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Window {
    /// The current session, the few hours since usage began.
    Session,
    /// The week, every model together.
    Week,
    /// The week, for the one model or surface named.
    WeekOf(String),
    /// A limit this release of adroit does not know, by Claude Code's name
    /// for its kind.
    Other(String),
}

/// How near Claude Code says a limit is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Normal,
    Warning,
    Critical,
}

/// One limit and how much of it is used.
#[derive(Debug, Clone, PartialEq)]
pub struct Limit {
    pub window: Window,
    /// Used, from 0 to 100; above 100 once the limit is passed.
    pub percent: f64,
    /// When the window starts again, in milliseconds since the Unix epoch.
    pub resets_ms: Option<u64>,
    pub severity: Severity,
}
