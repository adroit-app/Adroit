use serde::{Deserialize, Serialize};

/// A version of the work protocol, raised only when what is sent changes
/// shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Version(pub u32);

/// The versions this build speaks, oldest first.
pub const SPOKEN: &[Version] = &[Version(1)];

/// The version two peers work in: the newest both speak, or none.
pub fn agree(ours: &[Version], theirs: &[Version]) -> Option<Version> {
    ours.iter().filter(|v| theirs.contains(v)).max().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peers_work_in_the_newest_version_both_speak() {
        let v = Version;
        let cases: &[(&[Version], &[Version], Option<Version>)] = &[
            (&[v(1)], &[v(1)], Some(v(1))),
            (&[v(1), v(2)], &[v(1), v(2), v(3)], Some(v(2))),
            (&[v(2), v(1)], &[v(1), v(2)], Some(v(2))),
            (&[v(1)], &[v(2)], None),
            (&[v(1)], &[], None),
        ];
        for (ours, theirs, want) in cases {
            assert_eq!(agree(ours, theirs), *want, "{ours:?} with {theirs:?}");
        }
    }

    #[test]
    fn this_build_speaks_at_least_one_version() {
        assert_eq!(agree(SPOKEN, SPOKEN), SPOKEN.iter().max().copied());
        assert!(agree(SPOKEN, SPOKEN).is_some());
    }
}
