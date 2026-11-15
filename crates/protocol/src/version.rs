use serde::{Deserialize, Serialize};

/// A version of the work protocol, raised only when what is sent changes
/// shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Version(pub u32);

/// The versions this build speaks, oldest first.
pub const SPOKEN: &[Version] = &[Version(2)];

/// The version two peers work in: the newest both speak, or none.
pub fn agree(ours: &[Version], theirs: &[Version]) -> Option<Version> {
    ours.iter().filter(|v| theirs.contains(v)).max().copied()
}

/// What a work host says first on every connection, before it is asked
/// anything. Its shape is the same in every version, so any window can judge
/// any work host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub speaks: Vec<Version>,
    pub build: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<crate::RemoteEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<crate::RemoteEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation_epoch: Option<u64>,
    /// What the host takes beyond the versions it speaks, so a window asks
    /// for it only of a host that says so, and never leaves an older host's
    /// silence pending.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub offers: Vec<String>,
}

/// That the host types a reply into a session for what the session asks.
pub const ANSWER_ASKING: &str = "answer_asking";

/// What this build's work host offers beyond its versions.
pub const OFFERED: &[&str] = &[ANSWER_ASKING];

impl crate::Ask {
    /// What a host must offer to be asked this, none for what every host of
    /// a version takes.
    pub fn offer(&self) -> Option<&'static str> {
        match self {
            Self::AnswerAsking { .. } => Some(ANSWER_ASKING),
            _ => None,
        }
    }
}

/// What a window does with the work host listening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkHost {
    /// None listens: start one.
    Absent,
    /// Of this build and speaking a version the window does.
    Keep,
    /// One the window cannot work with, of another build, or one that says
    /// nothing: replace it, which ends no session.
    Replace,
}

/// How the work host stands against `build`, from what it said: `said` is
/// `None` where none listens, and `Some(None)` where one listens but says
/// nothing it can be judged by.
pub fn judge(said: Option<Option<&Hello>>, build: &str) -> WorkHost {
    match said {
        None => WorkHost::Absent,
        Some(Some(hello)) if agree(SPOKEN, &hello.speaks).is_some() && hello.build == build => {
            WorkHost::Keep
        }
        Some(_) => WorkHost::Replace,
    }
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

    fn hello(speaks: &[Version], build: &str) -> Hello {
        Hello {
            speaks: speaks.to_vec(),
            build: build.into(),
            endpoint: None,
            terminal: None,
            mutation_epoch: None,
            offers: Vec::new(),
        }
    }

    #[test]
    fn a_hello_says_what_its_host_offers_beyond_its_versions() {
        let offering = Hello {
            offers: vec![ANSWER_ASKING.into()],
            ..hello(&[Version(1)], "0.3.0")
        };
        assert_eq!(
            crate::encode(&offering),
            "{\"speaks\":[1],\"build\":\"0.3.0\",\"offers\":[\"answer_asking\"]}\n"
        );
        assert_eq!(
            crate::decode::<Hello>(&crate::encode(&offering)).unwrap(),
            offering
        );
    }

    #[test]
    fn a_hello_that_says_nothing_of_offers_offers_nothing() {
        let said: Hello = crate::decode("{\"speaks\":[1],\"build\":\"0.2.0\"}").unwrap();
        assert!(said.offers.is_empty());
    }

    #[test]
    fn a_build_that_knows_no_offers_still_reads_a_hello_that_names_them() {
        #[derive(Debug, Deserialize)]
        struct Earlier {
            speaks: Vec<Version>,
            build: String,
        }
        let said: Earlier = crate::decode(
            "{\"speaks\":[1],\"build\":\"0.3.0\",\"offers\":[\"answer_asking\",\"later\"]}",
        )
        .unwrap();
        assert_eq!(
            (said.speaks, said.build.as_str()),
            (vec![Version(1)], "0.3.0")
        );
    }

    #[test]
    fn answering_a_session_is_asked_only_of_a_host_that_offers_it() {
        use adroit_model::asking::{Asked, Asking, Reply};
        let answer = crate::Ask::AnswerAsking {
            session: "1".into(),
            asking: Asking {
                since_ms: 7,
                asked: Asked::Plan { plan: "Go".into() },
            },
            reply: Reply::Allow,
        };
        assert_eq!(answer.offer(), Some(ANSWER_ASKING));
        assert_eq!(crate::Ask::TerminalHost.offer(), None);
        assert!(OFFERED.contains(&ANSWER_ASKING));
    }

    #[test]
    fn a_work_host_of_this_build_speaking_a_shared_version_is_kept() {
        let mut speaks = SPOKEN.to_vec();
        speaks.push(Version(u32::MAX));
        assert_eq!(
            judge(Some(Some(&hello(&speaks, "0.3.0"))), "0.3.0"),
            WorkHost::Keep
        );
    }

    #[test]
    fn a_work_host_of_another_build_or_sharing_no_version_is_replaced() {
        assert_eq!(
            judge(Some(Some(&hello(SPOKEN, "0.2.0"))), "0.3.0"),
            WorkHost::Replace
        );
        assert_eq!(
            judge(Some(Some(&hello(&[Version(u32::MAX)], "0.3.0"))), "0.3.0"),
            WorkHost::Replace
        );
        assert_eq!(judge(Some(None), "0.3.0"), WorkHost::Replace);
    }

    #[test]
    fn where_no_work_host_listens_one_is_started() {
        assert_eq!(judge(None, "0.3.0"), WorkHost::Absent);
    }

    #[test]
    fn a_hello_is_written_as_a_work_host_before_versions_were_shared_wrote_it() {
        assert_eq!(
            crate::encode(&hello(&[Version(1)], "0.3.0")),
            "{\"speaks\":[1],\"build\":\"0.3.0\"}\n"
        );
    }

    #[test]
    fn this_build_speaks_at_least_one_version() {
        assert_eq!(agree(SPOKEN, SPOKEN), SPOKEN.iter().max().copied());
        assert!(agree(SPOKEN, SPOKEN).is_some());
    }
}
