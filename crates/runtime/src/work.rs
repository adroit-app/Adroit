//! The work host: the work it holds, how it answers a window's requests,
//! and the link a window reaches it through.

use std::sync::{Arc, Mutex};

use adroit_protocol::{DetailsWork, Event, Request, SPOKEN, Seq, WorkSnapshot, agree};
use tokio::sync::mpsc;

/// Everything the host holds of the work.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Work {
    seq: Seq,
    details: DetailsWork,
}

impl Work {
    /// The work as a window is first shown it.
    pub fn snapshot(&self) -> WorkSnapshot {
        WorkSnapshot {
            seq: self.seq,
            details: self.details.clone(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Host {
    work: Work,
}

impl Host {
    pub fn new(work: Work) -> Self {
        Self { work }
    }

    pub fn work(&self) -> &Work {
        &self.work
    }

    /// What the host tells the window that asked `request`.
    pub fn handle(&mut self, request: Request) -> Vec<Event> {
        match request {
            Request::Hello { versions } => match agree(SPOKEN, &versions) {
                Some(version) => vec![
                    Event::Welcome { version },
                    Event::Snapshot(self.work.snapshot()),
                ],
                None => vec![Event::Refused {
                    versions: SPOKEN.to_vec(),
                }],
            },
            Request::Snapshot => vec![Event::Snapshot(self.work.snapshot())],
            Request::Watch(_) => Vec::new(),
        }
    }
}

/// How a window reaches the work host. Sending never waits: what the host
/// says, in answer or of its own accord, arrives on the receiver the link
/// was made with.
pub trait HostLink {
    fn send(&self, request: Request);
}

/// A host run in the window's own process.
pub struct Loopback {
    host: Arc<Mutex<Host>>,
    events: mpsc::UnboundedSender<Event>,
}

impl Loopback {
    pub fn new(host: Host) -> (Self, mpsc::UnboundedReceiver<Event>) {
        let (events, received) = mpsc::unbounded_channel();
        let link = Self {
            host: Arc::new(Mutex::new(host)),
            events,
        };
        (link, received)
    }
}

impl HostLink for Loopback {
    fn send(&self, request: Request) {
        let answer = self
            .host
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .handle(request);
        for event in answer {
            // A window that stopped listening has closed; nothing is owed it.
            let _ = self.events.send(event);
        }
    }
}

/// A host run without a window: `requests` in turn, the work they leave and
/// every event they draw, for tests to read.
pub fn scenario(work: Work, requests: impl IntoIterator<Item = Request>) -> (Work, Vec<Event>) {
    let mut host = Host::new(work);
    let events = requests
        .into_iter()
        .flat_map(|request| host.handle(request))
        .collect();
    (host.work, events)
}

#[cfg(test)]
mod tests {
    use adroit_protocol::{Event, Request, SPOKEN, Seq, Version, WorkSnapshot};

    use super::*;

    fn hello(versions: &[u32]) -> Request {
        Request::Hello {
            versions: versions.iter().copied().map(Version).collect(),
        }
    }

    #[test]
    fn the_host_answers_hello_by_the_versions_both_speak() {
        let work = Work {
            seq: Seq(4),
            ..Default::default()
        };
        let cases = [
            (
                hello(&[1]),
                vec![
                    Event::Welcome {
                        version: Version(1),
                    },
                    Event::Snapshot(WorkSnapshot {
                        seq: Seq(4),
                        ..Default::default()
                    }),
                ],
            ),
            (
                hello(&[99]),
                vec![Event::Refused {
                    versions: SPOKEN.to_vec(),
                }],
            ),
            (
                hello(&[]),
                vec![Event::Refused {
                    versions: SPOKEN.to_vec(),
                }],
            ),
        ];
        for (request, want) in cases {
            let (_, events) = scenario(work.clone(), [request.clone()]);
            assert_eq!(events, want, "{request:?}");
        }
    }

    #[test]
    fn a_window_that_lost_track_is_sent_the_whole_work() {
        let (work, events) = scenario(
            Work {
                seq: Seq(9),
                ..Default::default()
            },
            [Request::Snapshot],
        );
        assert_eq!(
            events,
            vec![Event::Snapshot(WorkSnapshot {
                seq: Seq(9),
                ..Default::default()
            })]
        );
        assert_eq!(
            work,
            Work {
                seq: Seq(9),
                ..Default::default()
            }
        );
    }

    #[test]
    fn the_loopback_hands_the_answers_to_its_receiver_in_order() {
        let (link, mut events) = Loopback::new(Host::default());
        link.send(hello(&[1]));
        link.send(Request::Snapshot);
        let mut got = Vec::new();
        while let Ok(event) = events.try_recv() {
            got.push(event);
        }
        assert_eq!(
            got,
            vec![
                Event::Welcome {
                    version: Version(1)
                },
                Event::Snapshot(WorkSnapshot::default()),
                Event::Snapshot(WorkSnapshot::default()),
            ]
        );
    }

    #[test]
    fn a_closed_window_does_not_stop_the_host() {
        let (link, events) = Loopback::new(Host::default());
        drop(events);
        link.send(Request::Snapshot);
    }
}
