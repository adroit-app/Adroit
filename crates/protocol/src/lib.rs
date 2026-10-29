//! What the window and the work host say to each other: the window's
//! requests, the host's events, the work they describe, the versions each
//! side speaks, and how a message is written on the wire.

mod agents;
mod ask;
#[cfg(feature = "client")]
pub mod client;
mod codec;
mod config;
mod console;
mod details;
mod drafts;
mod message;
mod mutation;
mod past;
mod pick;
mod place;
mod prompt;
mod remote;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
mod tree;
mod version;
mod wire;
mod work;

pub use agents::*;
pub use ask::*;
pub use codec::*;
pub use config::*;
pub use console::*;
pub use details::*;
pub use drafts::*;
pub use message::*;
pub use mutation::*;
pub use past::*;
pub use pick::*;
pub use place::*;
pub use prompt::*;
pub use remote::*;
pub use tree::*;
pub use version::*;
pub use work::*;
