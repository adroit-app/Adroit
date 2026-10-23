//! What is picked from a list drawn over the window: a session to go to or
//! a directory to start one in, and a past conversation to resume.

mod choices;
pub mod filer;
pub mod fuzzy;
pub mod resume;

pub use choices::*;
