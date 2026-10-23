//! Viewers for the files Neovim cannot show: images, movies, HTML pages and
//! PDFs, each drawn by the window and only reading its file.

mod pdf;
mod serve;
mod trial;
mod view;

pub use adroit_core::viewers::*;
pub use pdf::*;
pub use serve::*;
pub use trial::launch as try_in_window;
pub use trial::{Resolved, resolve};
pub use view::*;
