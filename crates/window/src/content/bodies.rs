//! What a window shows, by the kind of thing it shows.

mod draft;
mod focused;
mod session;
mod terminal;
mod viewer;

pub(crate) use draft::*;
pub(crate) use focused::*;
pub(crate) use session::*;
pub(crate) use terminal::*;
pub(crate) use viewer::*;
