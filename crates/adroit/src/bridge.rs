//! The window's view state held for Dioxus: the state `step` changes, one
//! signal per slice for components to read, and the runner that carries out
//! what each message asks.

use std::rc::Rc;

use adroit_core::chrome::Chrome;
use adroit_core::effect::Effect;
use adroit_core::host::Host as HostView;
use adroit_core::msg::Msg;
use adroit_core::state::ViewState;
use adroit_core::update::step;
use adroit_runtime::work::{Host, HostLink, Loopback};
use dioxus::prelude::*;

use crate::settings;

/// What components read of the view state and how they change it. A slice's
/// signal is set only when `step` changed that slice, so a component reading
/// one slice redraws for it alone.
#[derive(Clone, Copy)]
pub(crate) struct AppCtx {
    view: CopyValue<ViewState>,
    link: CopyValue<Rc<dyn HostLink>>,
    pub(crate) chrome: Signal<Chrome>,
    pub(crate) host: Signal<HostView>,
}

/// The window's view state, opened once for the component that calls it,
/// with the work host run in-process and its events fed back as messages.
pub(crate) fn use_app() -> AppCtx {
    use_hook(|| {
        let (link, mut events) = Loopback::new(Host::default());
        let link: Rc<dyn HostLink> = Rc::new(link);
        let mut view = ViewState::default();
        let effects = step(
            &mut view,
            Msg::Opened {
                zoom: settings::current().zoom,
            },
        );
        let app = AppCtx {
            chrome: Signal::new(view.chrome.clone()),
            host: Signal::new(view.host.clone()),
            view: CopyValue::new(view),
            link: CopyValue::new(link),
        };
        app.run(effects);
        spawn(async move {
            while let Some(event) = events.recv().await {
                app.dispatch(Msg::Host(event));
            }
        });
        app
    })
}

impl AppCtx {
    /// `msg` applied to the view state, the slices it changed shown, and
    /// what it asks carried out. Called from handlers and tasks, never while
    /// rendering.
    pub(crate) fn dispatch(&self, msg: Msg) {
        let mut view = self.view;
        let effects = step(&mut view.write(), msg);
        self.publish();
        self.run(effects);
    }

    fn publish(&self) {
        let view = self.view.read();
        set_changed(self.chrome, &view.chrome);
        set_changed(self.host, &view.host);
    }

    fn run(&self, effects: Vec<Effect>) {
        let link = self.link.read().clone();
        run(effects, &*link, |effect| match effect {
            Effect::Zoom(level) => {
                let _ = dioxus::desktop::window().webview.zoom(level);
            }
            Effect::Host(_) => unreachable!("requests go to the host link"),
        });
    }
}

fn set_changed<T: Clone + PartialEq + 'static>(mut signal: Signal<T>, now: &T) {
    if *signal.peek() != *now {
        signal.set(now.clone());
    }
}

/// `effects` carried out in order: requests sent to the host through
/// `link`, the rest done by the window through `local`.
fn run(effects: Vec<Effect>, link: &dyn HostLink, mut local: impl FnMut(Effect)) {
    for effect in effects {
        match effect {
            Effect::Host(request) => link.send(request),
            effect => local(effect),
        }
    }
}

#[cfg(test)]
mod tests {
    use adroit_core::host::Link;
    use adroit_core::zoom::Step;
    use adroit_protocol::{SPOKEN, WorkSnapshot};
    use tokio::sync::mpsc::UnboundedReceiver;

    use std::collections::VecDeque;

    use adroit_protocol::Event;

    use super::*;

    /// The window as `AppCtx` runs it, without Dioxus: each message stepped,
    /// its effects run, and every event the host sends back stepped in turn.
    struct Window {
        view: ViewState,
        link: Loopback,
        events: UnboundedReceiver<Event>,
        drawn: Vec<Effect>,
    }

    impl Window {
        fn open(zoom: f64) -> Self {
            let (link, events) = Loopback::new(Host::default());
            let mut window = Self {
                view: ViewState::default(),
                link,
                events,
                drawn: Vec::new(),
            };
            window.dispatch(Msg::Opened { zoom });
            window
        }

        fn dispatch(&mut self, msg: Msg) {
            let mut queue = VecDeque::from([msg]);
            while let Some(msg) = queue.pop_front() {
                let effects = step(&mut self.view, msg);
                run(effects, &self.link, |e| self.drawn.push(e));
                while let Ok(event) = self.events.try_recv() {
                    queue.push_back(Msg::Host(event));
                }
            }
        }
    }

    #[test]
    fn an_opened_window_greets_the_host_and_holds_the_work() {
        let window = Window::open(1.25);
        assert_eq!(
            window.view.host.link,
            Link::Ready {
                version: *SPOKEN.iter().max().unwrap()
            }
        );
        assert_eq!(window.view.host.work, Some(WorkSnapshot::default()));
        assert_eq!(window.drawn, vec![Effect::Zoom(1.25)]);
    }

    #[test]
    fn the_zoom_keys_redraw_the_window_and_never_reach_the_host() {
        let mut window = Window::open(1.0);
        window.dispatch(Msg::Zoom(Step::In));
        window.dispatch(Msg::Zoom(Step::In));
        window.dispatch(Msg::Zoom(Step::Reset));
        assert_eq!(
            window.drawn,
            vec![
                Effect::Zoom(1.0),
                Effect::Zoom(1.1),
                Effect::Zoom(1.25),
                Effect::Zoom(1.0)
            ]
        );
        assert_eq!(window.view.chrome.zoom, 1.0);
    }
}
