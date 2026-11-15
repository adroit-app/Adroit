//! The details panel as the window draws it: its tabs, the components of
//! the tab shown and of its foot, and the rows opened from them.

mod changes;
mod github;
mod log;
mod message;
mod overview;
mod panel;
mod parts;
mod process;
mod subagent;
mod timeline;
mod usage;

pub(crate) use adroit_core::details::*;
pub(crate) use github::GitHubCard;
pub(crate) use panel::*;
pub(crate) use usage::Gauges;
