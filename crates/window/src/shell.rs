//! The window's frame around the tree, the content and the details: the
//! root component that puts the window together, the header, the panels'
//! edges, the status bar and what is drawn over it all.

mod app;
mod body;
mod effects;
mod footer;
pub(crate) mod header;
mod local;
pub(crate) mod overlays;
pub(crate) mod panels;
mod socket;
pub(crate) mod status_bar;
mod ui;

pub use app::launch;
pub(crate) use local::Local;
pub(crate) use ui::Ui;
