//! The details panel as the window draws it: its sheet tabs, the overview,
//! the changes and the log, the rows opened from them, and the usage at its
//! foot.

mod changes;
mod log;
mod message;
mod overview;
mod panel;
mod process;
mod subagent;
mod usage;

pub(crate) use adroit_core::details::*;
pub(crate) use panel::*;
pub(crate) use usage::MachineUsage;
