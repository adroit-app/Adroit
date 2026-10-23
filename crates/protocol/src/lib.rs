//! What the window and the work host say to each other: the window's
//! requests, the host's events, the work they describe, the versions each
//! side speaks, and how a message is written on the wire.

mod codec;
mod message;
mod version;
mod work;

pub use codec::*;
pub use message::*;
pub use version::*;
pub use work::*;
