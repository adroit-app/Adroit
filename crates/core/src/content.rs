//! The content: its tabs and the windows they split into, what each window
//! shows, which area has the keys, and the agent a draft starts.

pub mod draft;
mod focus;
mod layout;
mod tab;

pub use focus::*;
pub use layout::*;
pub use tab::*;
