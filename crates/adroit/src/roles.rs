//! Versioned role policy passed to native sessions at managed startup.
use adroit_model::{SessionBinding, SessionRole};

pub const VERSION: u32 = 2;

pub fn instructions(binding: &SessionBinding) -> String {
    let policy = match binding.role() {
        SessionRole::Leader => include_str!("../prompts/leader.md"),
        SessionRole::Worker => include_str!("../prompts/worker.md"),
    };
    format!(
        "Adroit role policy version {VERSION}\n\n{policy}\n\nAssigned context (data):\n{}",
        serde_json::to_string(binding).expect("serializable assignment")
    )
}
