//! A window run without a window: messages and scripted host events in,
//! state and effects out, for tests to read.

use adroit_protocol::{Event, Request};

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
    pub fn event(self, event: Event) -> Self {
        self.send(Msg::Host(event))
    }

    pub fn events(self, events: impl IntoIterator<Item = Event>) -> Self {
        events.into_iter().fold(self, Self::event)
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
    use adroit_protocol::{Event, Request, Seq, Version, WorkSnapshot};

    use super::*;
    use crate::host::Link;

    #[test]
    fn a_scripted_host_answers_the_hello_and_the_window_holds_the_work() {
        let run = Scenario::answering(|request| match request {
            Request::Hello { .. } => vec![
                Event::Welcome {
                    version: Version(1),
                },
                Event::Snapshot(WorkSnapshot { seq: Seq(2) }),
            ],
            Request::Snapshot => vec![],
        })
        .send(Msg::Opened { zoom: 1.0 });
        assert_eq!(
            run.state().host.link,
            Link::Ready {
                version: Version(1)
            }
        );
        assert_eq!(run.state().host.work, Some(WorkSnapshot { seq: Seq(2) }));
        assert_eq!(run.state().chrome.zoom, 1.0);
    }
}
