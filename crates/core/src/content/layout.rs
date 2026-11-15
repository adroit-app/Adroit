//! What the content holds: tabs, each split into windows as Neovim splits its
//! tabs, every window showing one thing. Windows only show; what they show
//! runs on whether or not a window shows it.

mod kept;
mod node;
mod tab;
mod tabs;

pub use kept::*;
pub use node::*;
pub use tab::*;
pub use tabs::*;

#[cfg(test)]
mod tests;
