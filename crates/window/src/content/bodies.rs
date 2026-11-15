//! What a window shows, by the kind of thing it shows.

mod draft;
mod focused;
mod github;
mod session;
mod terminal;
mod viewer;

pub(crate) use draft::*;
pub(crate) use focused::*;
pub(crate) use github::*;
pub(crate) use session::*;
pub(crate) use terminal::*;
pub(crate) use viewer::*;
