//! Viewers for files the window draws itself: images, movies, HTML pages,
//! Markdown documents and PDFs, each only reading its file.

mod markdown;
mod pdf;
mod serve;
mod trial;
mod view;

pub use adroit_core::viewers::*;
pub use markdown::*;
pub use pdf::*;
pub use serve::*;
pub use trial::launch as try_in_window;
pub use trial::{Resolved, resolve};
pub use view::*;
