//! A window run without a window: messages and scripted host events in,
//! state and effects out, for tests to read.

use adroit_protocol::{Change, Event, Request, Seq, WorkPatch};

use crate::effect::Effect;
use crate::msg::Msg;
use crate::state::ViewState;
use crate::update::step;

type Script = Box<dyn FnMut(&Request) -> Vec<Event>>;

pub struct Scenario {
    state: ViewState,
    effects: Vec<Effect>,
    last: Vec<Effect>,
    host: Script,
}

impl Default for Scenario {
    fn default() -> Self {
        Self::new()
    }
}

impl Scenario {
    /// A window whose host answers nothing unless told with `event`.
    pub fn new() -> Self {
        Self::answering(|_| Vec::new())
    }

    /// A window whose host answers each request with what `host` returns,
    /// fed back as messages before the next one is sent.
    pub fn answering(host: impl FnMut(&Request) -> Vec<Event> + 'static) -> Self {
        Self {
            state: ViewState::default(),
            effects: Vec::new(),
            last: Vec::new(),
            host: Box::new(host),
        }
    }

    pub fn send(mut self, msg: Msg) -> Self {
        let mut queue = std::collections::VecDeque::from([msg]);
        let mut last = Vec::new();
        while let Some(msg) = queue.pop_front() {
            for effect in step(&mut self.state, msg) {
                if let Effect::Host(request) = &effect {
                    queue.extend((self.host)(request).into_iter().map(Msg::Host));
                }
                last.push(effect);
            }
        }
        self.effects.extend(last.iter().cloned());
        self.last = last;
        self
    }

    pub fn send_all(self, msgs: impl IntoIterator<Item = Msg>) -> Self {
        msgs.into_iter().fold(self, Self::send)
    }

    /// The host told the window `event` of its own accord.
    pub fn event(self, event: impl Into<Event>) -> Self {
        self.send(Msg::Host(event.into()))
    }

    pub fn events<E: Into<Event>>(self, events: impl IntoIterator<Item = E>) -> Self {
        events.into_iter().fold(self, Self::event)
    }

    /// The host told the window `change` as the patch after the last word
    /// it heard.
    pub fn tell(self, change: impl Into<Change>) -> Self {
        let seq = self.state.host.work.as_ref().map_or(Seq(0), |w| w.seq);
        self.event(Event::Patch(WorkPatch {
            seq: seq.next(),
            changes: vec![change.into()],
        }))
    }

    pub fn state(&self) -> &ViewState {
        &self.state
    }

    /// Every effect asked for, in order.
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }

    /// The effects the last message sent asked for, with those of the host's
    /// answers to it.
    pub fn last_effects(&self) -> &[Effect] {
        &self.last
    }
}

#[cfg(test)]
mod tests {
    use adroit_protocol::{Event, Request, Seq, Version};

    use super::*;
    use crate::host::Link;
    use crate::testing::snapshot;

    #[test]
    fn a_scripted_host_answers_the_hello_and_the_window_holds_the_work() {
        let run = Scenario::answering(|request| match request {
            Request::Hello { .. } => vec![
                Event::Welcome {
                    version: Version(1),
                },
                snapshot().seq(2).into(),
            ],
            _ => vec![],
        })
        .send(Msg::Opened {
            home: Default::default(),
        });
        assert_eq!(
            run.state().host.link,
            Link::Ready {
                version: Version(1)
            }
        );
        assert_eq!(run.state().host.work.as_ref().map(|w| w.seq), Some(Seq(2)));
        assert_eq!(run.state().chrome.zoom, 1.0);
    }
}
