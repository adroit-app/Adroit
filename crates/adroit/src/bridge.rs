//! The window's view state held for Dioxus: the state `step` changes, one
//! signal per slice for components to read, and the runner that carries out
//! what each message asks.

use std::rc::Rc;

use adroit_core::chrome::Chrome;
use adroit_core::details::{Details, Scroll};
use adroit_core::effect::Effect;
use adroit_core::host::Host as HostView;
use adroit_core::msg::Msg;
use adroit_core::state::ViewState;
use adroit_core::update::step;
use adroit_runtime::work::{Host, HostLink, Loopback};
use dioxus::prelude::*;

use crate::settings;

/// What runs an effect handed to `App`.
type Handed = Rc<dyn Fn(Effect)>;

/// What components read of the view state and how they change it. A slice's
/// signal is set only when `step` changed that slice, so a component reading
/// one slice redraws for it alone.
#[derive(Clone, Copy)]
pub(crate) struct AppCtx {
    view: CopyValue<ViewState>,
    link: CopyValue<Rc<dyn HostLink>>,
    pub(crate) chrome: Signal<Chrome>,
    pub(crate) host: Signal<HostView>,
    pub(crate) details: Signal<Details>,
    /// What runs the effects whose state `App` still holds.
    handed: CopyValue<Option<Handed>>,
}

/// The window's view state, opened once for the component that calls it,
/// with the work host run in-process and its events fed back as messages.
pub(crate) fn use_app() -> AppCtx {
    use_hook(|| {
        let (link, mut events) = Loopback::new(Host::default());
        link.watch();
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
            details: Signal::new(view.details.clone()),
            handed: CopyValue::new(None),
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
        set_changed(self.details, &view.details);
    }

    /// Hand the effects whose state still lives in `App` to `run`: showing
    /// a session or an image, and telling the user.
    pub(crate) fn hand_over(&self, run: impl Fn(Effect) + 'static) {
        let mut handed = self.handed;
        handed.set(Some(Rc::new(run)));
    }

    fn run(&self, effects: Vec<Effect>) {
        let link = self.link.read().clone();
        run(effects, &*link, |effect| match effect {
            Effect::Zoom(level) => {
                let _ = dioxus::desktop::window().webview.zoom(level);
            }
            Effect::Open(target) => open([target]),
            Effect::Reveal(path) => open(["-R".into(), path]),
            Effect::Copy { text, done } => {
                let told = match crate::app::copy_to_clipboard(&text) {
                    Ok(()) => done,
                    Err(e) => format!("cannot copy: {e}"),
                };
                self.hand(Effect::Notice(told));
            }
            Effect::ScrollDetails(scroll) => {
                let by = match scroll {
                    Scroll::Down => "17",
                    Scroll::Up => "-17",
                    Scroll::Top => "-b.scrollHeight",
                    Scroll::Bottom => "b.scrollHeight",
                };
                document::eval(&format!(
                    "const b = document.querySelector('.details-scroll'); if (b) b.scrollBy(0, {by});"
                ));
            }
            effect @ (Effect::ShowSession(_) | Effect::ShowShot { .. } | Effect::Notice(_)) => {
                self.hand(effect)
            }
            Effect::Host(_) => unreachable!("requests go to the host link"),
        });
    }
}

impl AppCtx {
    fn hand(&self, effect: Effect) {
        let handed = self.handed.read().clone();
        if let Some(run) = handed {
            run(effect);
        }
    }
}

/// `/usr/bin/open` run with `args`, without waiting for it.
pub(crate) fn open<const N: usize>(args: [String; N]) {
    let mut cmd = tokio::process::Command::new("/usr/bin/open");
    cmd.args(args);
    spawn(async move {
        let _ = cmd.status().await;
    });
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
