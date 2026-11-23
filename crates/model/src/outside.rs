//! The keys the user gives programs outside Adroit.

use serde::{Deserialize, Serialize};

use crate::MessageId;

/// A key the user gave a program, by what is known of it but the key itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutsideKey {
    pub id: MessageId,
    /// The program it was given to, which acts under this name.
    pub name: String,
    pub issued_ms: u64,
    /// When it was last used; none while it never was.
    pub used_ms: Option<u64>,
}
