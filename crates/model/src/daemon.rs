//! How the window writes the id the daemon gives a session it runs: the
//! `id` of a [`crate::Session`] adroit runs.

/// The daemon's id as the window writes it.
pub fn hex(id: u128) -> String {
    format!("{id:032x}")
}

/// The daemon's id a session of adroit's goes by in the window.
pub fn id_of(hex: &str) -> Option<u128> {
    (hex.len() == 32)
        .then(|| u128::from_str_radix(hex, 16).ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_daemon_id_reads_as_one() {
        assert_eq!(id_of("not an id"), None);
        assert_eq!(id_of(&hex(u128::MAX)), Some(u128::MAX));
    }
}
