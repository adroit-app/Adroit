//! What the window knows of the work host: whether they speak, and the work
//! as the host last described it.

use adroit_protocol::{Event, Request, SPOKEN, Sight, Told, Version, WorkPatch, WorkSnapshot};

use crate::chrome::status_line;
use crate::content::Tab;
use crate::effect::Effect;
use crate::state::ViewState;

/// How the window stands with the work host.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Link {
    /// The window said hello and waits for the answer.
    #[default]
    Opening,
    /// They work together in `version`.
    Ready { version: Version },
    /// They share no version; the host speaks these.
    Refused { versions: Vec<Version> },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Host {
    pub link: Link,
    /// The work as of the last snapshot and the patches after it, or none
    /// before the first snapshot.
    pub work: Option<WorkSnapshot>,
    /// A snapshot was asked for and has not come yet.
    resyncing: bool,
    /// What the host was last asked to keep read.
    pub(crate) watched: Sight,
}

/// The first request a window sends.
pub(crate) fn hello() -> Effect {
    Effect::Host(Request::Hello {
        versions: SPOKEN.to_vec(),
    })
}

/// The host told the window `event`.
pub(crate) fn receive(host: &mut Host, event: Event) -> Vec<Effect> {
    match event {
        Event::Welcome { version } if SPOKEN.contains(&version) => {
            host.link = Link::Ready { version };
        }
        Event::Welcome { version } => {
            host.link = Link::Refused {
                versions: vec![version],
            };
        }
        Event::Refused { versions } => host.link = Link::Refused { versions },
        Event::Snapshot(snapshot) => {
            host.work = Some(*snapshot);
            host.resyncing = false;
        }
        Event::Patch(patch) => return patched(host, patch),
        // Told the window as a whole, not its link; an answer goes to
        // what asked.
        Event::Told(_) | Event::Type { .. } | Event::Answer { .. } => {}
    }
    Vec::new()
}

/// The host told the window `event`: how a request it carried out went,
/// said in the status line, and what to type into a session, typed into its
/// pane; anything else changes the work as [`receive`] says.
pub(crate) fn told(state: &mut ViewState, event: Event) -> Vec<Effect> {
    let status = &mut state.chrome.status;
    match event {
        Event::Told(Told::Notice { text }) => status_line::notice(status, text),
        Event::Told(Told::Failure { text }) => status_line::set(status, Some(text)),
        Event::Type { session, typing } => vec![Effect::TypeInto {
            tab: Tab::Session(session),
            typing,
        }],
        event => receive(&mut state.host, event),
    }
}

/// A patch is applied only on top of the one before it. One the window
/// already holds is dropped; one past a gap means patches were lost, and
/// the window asks for the whole work again, once.
fn patched(host: &mut Host, patch: WorkPatch) -> Vec<Effect> {
    let Some(work) = host.work.as_mut() else {
        return Vec::new();
    };
    if patch.seq == work.seq.next() {
        apply(work, patch);
        return Vec::new();
    }
    if patch.seq <= work.seq || host.resyncing {
        return Vec::new();
    }
    host.resyncing = true;
    vec![Effect::Host(Request::Snapshot)]
}

fn apply(work: &mut WorkSnapshot, patch: WorkPatch) {
    for change in patch.changes {
        work.apply(change);
    }
    work.seq = patch.seq;
}

#[cfg(test)]
mod tests {
    use adroit_protocol::{Event, Request, Version};

    use super::Link;
    use crate::effect::Effect;
    use crate::msg::Msg;
    use crate::scenario::Scenario;
    use crate::testing;

    // Events rather than builders, so one case can mix patches and snapshots.
    fn patch(seq: u64) -> Event {
        testing::patch(seq).into()
    }

    fn snapshot(seq: u64) -> Event {
        testing::snapshot().seq(seq).into()
    }

    #[test]
    fn opening_the_window_says_hello_in_every_version_it_speaks() {
        let run = Scenario::new().send(Msg::Opened {
            home: Default::default(),
        });
        assert!(run.last_effects().contains(&Effect::Host(Request::Hello {
            versions: adroit_protocol::SPOKEN.to_vec(),
        })));
        assert_eq!(run.state().host.link, Link::Opening);
    }

    #[test]
    fn the_answer_to_hello_decides_the_link() {
        let cases = [
            (
                Event::Welcome {
                    version: Version(1),
                },
                Link::Ready {
                    version: Version(1),
                },
            ),
            (
                Event::Welcome {
                    version: Version(99),
                },
                Link::Refused {
                    versions: vec![Version(99)],
                },
            ),
            (
                Event::Refused {
                    versions: vec![Version(7)],
                },
                Link::Refused {
                    versions: vec![Version(7)],
                },
            ),
        ];
        for (event, link) in cases {
            let run = Scenario::new().event(event.clone());
            assert_eq!(run.state().host.link, link, "{event:?}");
        }
    }

    #[test]
    fn patches_apply_in_order_and_a_gap_asks_for_the_work_again_once() {
        // (events after a snapshot at 3, seq held, snapshots asked for)
        let cases: Vec<(Vec<Event>, Option<u64>, usize)> = vec![
            (vec![patch(4), patch(5)], Some(5), 0),
            (vec![patch(3)], Some(3), 0),
            (vec![patch(2), patch(4)], Some(4), 0),
            (vec![patch(5)], Some(3), 1),
            (vec![patch(5), patch(6), patch(7)], Some(3), 1),
            (vec![patch(5), snapshot(6), patch(8)], Some(6), 2),
            (vec![patch(5), snapshot(6), patch(7)], Some(7), 1),
        ];
        for (events, seq, asked) in cases {
            let run = Scenario::new().event(snapshot(3)).events(events.clone());
            assert_eq!(
                run.state().host.work.as_ref().map(|w| w.seq.0),
                seq,
                "{events:?}"
            );
            let asks = run
                .effects()
                .iter()
                .filter(|e| **e == Effect::Host(Request::Snapshot))
                .count();
            assert_eq!(asks, asked, "{events:?}");
        }
    }

    #[test]
    fn patches_before_the_first_snapshot_wait_for_it() {
        let run = Scenario::new().events([patch(1), patch(5)]);
        assert_eq!(run.state().host.work, None);
        assert!(run.effects().is_empty());
    }

    #[test]
    fn what_the_host_says_of_a_request_is_said_in_the_status_line_and_typing_goes_to_the_pane() {
        use adroit_protocol::{Told, Typing};

        use crate::content::Tab;

        let said = |told: Told| {
            Scenario::new()
                .event(Event::Told(told))
                .state()
                .chrome
                .status
                .line
                .clone()
        };
        let notice = said(Told::Notice {
            text: "ended fix".into(),
        });
        assert_eq!(notice.as_deref(), Some("ended fix"));
        let failure = said(Told::Failure {
            text: "cannot end fix".into(),
        });
        assert_eq!(failure.as_deref(), Some("cannot end fix"));
        let typing = vec![Typing::Paste {
            text: "/rename fixer".into(),
        }];
        let run = Scenario::new().event(Event::Type {
            session: "0a".into(),
            typing: typing.clone(),
        });
        assert!(run.last_effects().contains(&Effect::TypeInto {
            tab: Tab::Session("0a".into()),
            typing,
        }));
    }
}
