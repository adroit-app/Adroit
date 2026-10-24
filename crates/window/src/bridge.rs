//! The window's view state held for Dioxus: the state `step` changes, one
//! signal per slice for components to read, and the runner that carries out
//! what each message asks.

use std::rc::Rc;

use adroit_core::actions::Action;
use adroit_core::chrome::Chrome;
use adroit_core::config::Config;
use adroit_core::console::ConsoleView;
use adroit_core::content::{Area, ContentView, Tab, kept_name};
use adroit_core::details::{Details, Scroll};
use adroit_core::effect::Effect;
use adroit_core::host::Host as HostView;
use adroit_core::msg::Msg;
use adroit_core::overlay::{self, Holder, Overlays};
use adroit_core::picker::PickerView;
use adroit_core::prompt::PromptView;
use adroit_core::state::ViewState;
use adroit_core::tree::TreeView;
use adroit_core::update::step;
use adroit_model::Key;
use adroit_protocol::{Answer, Ask, Event, Request};
use dioxus::core::{Runtime, current_scope_id};
use dioxus::prelude::*;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::layout::{Kept as _, Layout};
use crate::link::{Link, To};
use crate::paths::{home, instance_state};

/// How long the window waits for the work before it is first drawn, so it
/// opens in the configuration's zoom and theme; a work host started for it
/// takes a moment.
const FIRST_WORK: std::time::Duration = std::time::Duration::from_secs(10);

/// How the window reaches the work host.
trait HostLink {
    fn send(&self, request: Request);
}

impl HostLink for Link {
    fn send(&self, request: Request) {
        Link::send(self, request);
    }
}

/// What runs an effect handed to `App`.
type Handed = Rc<dyn Fn(Effect)>;

/// What runs an action handed to `App`, with the key that ran it.
type Performer = Rc<dyn Fn(Action, Option<Key>)>;

/// What components read of the view state and how they change it. A slice's
/// signal is set only when `step` changed that slice, so a component reading
/// one slice redraws for it alone.
#[derive(Clone, Copy)]
pub(crate) struct AppCtx {
    view: CopyValue<ViewState>,
    link: CopyValue<Rc<Link>>,
    pub(crate) chrome: Signal<Chrome>,
    pub(crate) config: Signal<Config>,
    pub(crate) host: Signal<HostView>,
    pub(crate) content: Signal<ContentView>,
    pub(crate) tree: Signal<TreeView>,
    pub(crate) details: Signal<Details>,
    pub(crate) picker: Signal<PickerView>,
    pub(crate) overlays: Signal<Overlays>,
    pub(crate) console: Signal<ConsoleView>,
    pub(crate) prompt: Signal<PromptView>,
    /// What runs the effects whose state `App` still holds.
    handed: CopyValue<Option<Handed>>,
    /// What runs an action where the keys are now, which `App` still does.
    performer: CopyValue<Option<Performer>>,
    /// The component that opened the view state, whose tasks live as long
    /// as the window.
    scope: ScopeId,
}

/// The window's view state, opened once for the component that calls it,
/// with the work host linked and its events fed back as messages.
pub(crate) fn use_app() -> AppCtx {
    use_hook(|| {
        let (link, mut events, first) = Link::open(work_host());
        let link = Rc::new(link);
        // Drawn without the work, should the host not answer in time.
        let _ = first.recv_timeout(FIRST_WORK);
        let mut view = ViewState::default();
        // A window that kept nothing yet shows what the instance's one
        // window kept before there could be several.
        let kept = std::fs::read_to_string(shown_path())
            .or_else(|_| std::fs::read_to_string(instance_state().join("shown")))
            .ok()
            .and_then(|name| adroit_core::content::from_kept(&name));
        // The layout kept comes first: hiding a panel gives its keys away,
        // and a panel kept hidden was not hidden now. The host's first
        // answers are taken in before the window is drawn, so it opens in
        // the configuration's zoom and theme.
        let mut effects = pump(
            &mut view,
            &*link,
            &mut events,
            [
                Msg::PanelsKept(Layout::load()),
                Msg::WindowFocused(dioxus::desktop::window().is_focused()),
                Msg::Opened { home: home() },
            ],
        );
        // Where a session started with nothing open runs, as the host
        // answered.
        let scratch = view.config.scratch.clone();
        effects.extend(pump(
            &mut view,
            &*link,
            &mut events,
            [Msg::ContentOpened { kept, scratch }],
        ));
        let app = AppCtx {
            chrome: Signal::new(view.chrome.clone()),
            config: Signal::new(view.config.clone()),
            host: Signal::new(view.host.clone()),
            content: Signal::new(view.content.clone()),
            tree: Signal::new(view.tree.clone()),
            details: Signal::new(view.details.clone()),
            picker: Signal::new(view.picker.clone()),
            overlays: Signal::new(view.overlays.clone()),
            console: Signal::new(view.console.clone()),
            prompt: Signal::new(view.prompt.clone()),
            handed: CopyValue::new(None),
            performer: CopyValue::new(None),
            scope: current_scope_id(),
            view: CopyValue::new(view),
            link: CopyValue::new(link),
        };
        provide_context(app);
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
        as_from(self.scope, || self.run(effects));
    }

    fn publish(&self) {
        let view = self.view.read();
        set_changed(self.chrome, &view.chrome);
        set_changed(self.config, &view.config);
        crate::feed::publish(&view.config);
        set_changed(self.host, &view.host);
        set_changed(self.content, &view.content);
        set_changed(self.tree, &view.tree);
        set_changed(self.details, &view.details);
        set_changed(self.picker, &view.picker);
        set_changed(self.overlays, &view.overlays);
        set_changed(self.console, &view.console);
        set_changed(self.prompt, &view.prompt);
    }

    /// Tell the host what the view state does not hold, as it happens many
    /// times a second: that the console's screen was drawn anew.
    pub(crate) fn tell_host(&self, request: Request) {
        let link = self.link.read().clone();
        link.send(request);
    }

    /// Bring the Settings window forward, opening it when it is not open.
    pub(crate) fn open_settings(&self) {
        let link = (**self.link.read()).clone();
        crate::settings_window::open(self.agents(), link);
    }

    /// Ask the host `ask`, and what came of it.
    pub(crate) fn ask(&self, ask: Ask) -> impl std::future::Future<Output = Answer> + use<> {
        self.link.read().ask(ask)
    }

    /// Hand the effects whose state still lives in `App` to `run`: the
    /// panes, the prompt and the console, terminals started and images shown.
    pub(crate) fn hand_over(&self, run: impl Fn(Effect) + 'static) {
        let mut handed = self.handed;
        handed.set(Some(Rc::new(run)));
    }

    /// Hand the actions a menu entry runs to `perform`.
    pub(crate) fn hand_perform(&self, perform: impl Fn(Action, Option<Key>) + 'static) {
        let mut performer = self.performer;
        performer.set(Some(Rc::new(perform)));
    }

    /// What is drawn over the window and holds the keys, the first to
    /// take a key first.
    pub(crate) fn held(&self) -> Vec<Holder> {
        overlay::holding(
            &self.overlays.peek(),
            &self.picker.peek(),
            &self.chrome.peek(),
        )
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
                let told = match crate::clipboard::copy_to_clipboard(&text) {
                    Ok(()) => done,
                    Err(e) => format!("cannot copy: {e}"),
                };
                self.notice(told);
            }
            Effect::ScrollDetails(scroll) => {
                let by = match scroll {
                    Scroll::Down => "17",
                    Scroll::Up => "-17",
                    Scroll::HalfDown => "b.clientHeight / 2",
                    Scroll::HalfUp => "-b.clientHeight / 2",
                    Scroll::Top => "-b.scrollHeight",
                    Scroll::Bottom => "b.scrollHeight",
                };
                document::eval(&format!(
                    "const b = document.querySelector('.details-scroll'); if (b) b.scrollBy(0, {by});"
                ));
            }
            Effect::Notice(text) => self.notice(text),
            Effect::ShowSession(id) => self.dispatch(Msg::ShowSession(id)),
            Effect::KeepShown(shown) => match shown {
                Some(tab) => {
                    if let Some(name) = kept_name(&tab) {
                        let path = shown_path();
                        if let Some(dir) = path.parent() {
                            let _ = std::fs::create_dir_all(dir);
                        }
                        let _ = std::fs::write(path, name);
                    }
                }
                None => {
                    let _ = std::fs::remove_file(shown_path());
                }
            },
            // Told only while the window is not in front, and by one window
            // of the instance alone.
            Effect::NotifyWaiting(waiting) => {
                if !dioxus::desktop::window().is_focused()
                    && crate::window_socket::first_open(
                        &crate::window_socket::windows(),
                        crate::window_socket::number(),
                    )
                {
                    for (name, why) in waiting {
                        spawn(crate::alerts::notify_waiting(&name, &why));
                    }
                }
            }
            Effect::OpenSettings => self.open_settings(),
            Effect::CloseWindow => dioxus::desktop::window().close(),
            Effect::OpenWindow => {
                if let Err(e) = open_another_window() {
                    self.notice(format!("cannot open another window: {e}"));
                }
            }
            effect @ (Effect::ShowShot { .. }
            | Effect::Viewer { .. }
            | Effect::TakeSuggestion { .. }
            | Effect::EndShell(_)
            | Effect::FilterFocus(_)
            | Effect::PromptOffer(_)
            | Effect::PromptCommands(_)
            | Effect::Palette(_)
            | Effect::PromptRoot(_)
            | Effect::StartTerminal(_)
            | Effect::PromptClose(_)
            | Effect::StartWriting
            | Effect::KeepPanes { .. }
            | Effect::PaneKey(_)
            | Effect::PaneScrollKey(_)
            | Effect::PaneText { .. }
            | Effect::PaneWheel(_)
            | Effect::ReaderKey(_)
            | Effect::ReadConsole(_)
            | Effect::ConsoleLive
            | Effect::PromptKey(_)
            | Effect::PromptText { .. }
            | Effect::KeepImage { .. }
            | Effect::AskInput(_)
            | Effect::PromptRecall { .. }
            | Effect::RemoveImage(_)
            | Effect::TakePrompt
            | Effect::SendPrompt(_)
            | Effect::StartDraft(_)
            | Effect::PromptSent
            | Effect::PromptRestore { .. }
            | Effect::PromptShow(_)
            | Effect::Resume(_)
            | Effect::RestartDaemon
            | Effect::TypeInto { .. }) => self.hand(effect),
            Effect::Perform(action) => self.perform(action, None),
            Effect::PerformKey(action, key) => self.perform(action, Some(key)),
            Effect::AskInputAfter(after) => {
                let app = *self;
                spawn(async move {
                    tokio::time::sleep(after).await;
                    app.dispatch(Msg::AskInput);
                });
            }
            Effect::LapsePull { pull, after } => {
                let app = *self;
                spawn(async move {
                    tokio::time::sleep(after).await;
                    app.dispatch(Msg::PullLapsed(pull));
                });
            }
            Effect::LapseCompose { asked, after } => {
                let app = *self;
                spawn(async move {
                    tokio::time::sleep(after).await;
                    app.dispatch(Msg::ComposeLapsed(asked));
                });
            }
            Effect::KeepLayout(layout) => layout.save(),
            Effect::ScrollKeysHelp(lines) => crate::keys_help_view::scroll(lines),
            Effect::LapseNotice {
                notice,
                text,
                after,
            } => {
                let app = *self;
                spawn(async move {
                    tokio::time::sleep(after).await;
                    app.dispatch(Msg::NoticeLapsed { notice, text });
                });
            }
            Effect::Host(_) => unreachable!("requests go to the host link"),
        });
    }
}

impl AppCtx {
    fn perform(&self, action: Action, key: Option<Key>) {
        let performer = self.performer.read().clone();
        if let Some(perform) = performer {
            perform(action, key);
        }
    }

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

/// The work host of the window's instance, started as this program serving
/// it.
fn work_host() -> To {
    To {
        socket: adroit_model::paths::work_host_socket(),
        program: crate::panes::adroit_program(),
        args: vec!["host".into()],
        build: crate::build::build(),
    }
}

/// Where the tab shown is kept for the next run of the window.
/// Open another window of this instance, in a process of its own as every
/// window is.
fn open_another_window() -> std::io::Result<()> {
    let program = crate::panes::adroit_program();
    let mut window = std::process::Command::new(program)
        .args([
            "--instance",
            &adroit_model::paths::instance(),
            "--new-window",
        ])
        .stdin(std::process::Stdio::null())
        .spawn()?;
    // Waited for, so a window closed before this one leaves nothing behind.
    std::thread::spawn(move || window.wait());
    Ok(())
}

/// Where this window keeps what its focused window shows, under its number.
fn shown_path() -> std::path::PathBuf {
    crate::window_socket::kept_path("shown")
}

impl AppCtx {
    /// What the content's focused window shows now.
    pub(crate) fn active(&self) -> Option<Tab> {
        self.content.peek().active().cloned()
    }

    /// The area that has the keys now.
    pub(crate) fn focus(&self) -> Area {
        self.content.peek().focus
    }

    /// What the work host told of each agent.
    pub(crate) fn agents(&self) -> adroit_protocol::AgentsWork {
        let host = self.host.peek();
        host.work
            .as_ref()
            .map(|w| w.agents.clone())
            .unwrap_or_default()
    }

    /// The agent the prompt of `tab` goes to, read so a component or an
    /// effect follows it.
    pub(crate) fn agent_of(&self, tab: Option<&Tab>) -> adroit_model::Agent {
        let host = self.host.read();
        let tree = adroit_core::tree::listed(host.work.as_ref());
        let default = self.config.read().settings.agent;
        self.content.read().drafts.agent_of(tree, tab, default)
    }
}

impl AppCtx {
    /// Show `line` in the status line until something replaces it, or
    /// clear it.
    pub(crate) fn status(&self, line: Option<String>) {
        self.dispatch(Msg::Status(line));
    }

    /// Show `text` in the status line for a while.
    pub(crate) fn notice(&self, text: String) {
        self.dispatch(Msg::Notice(text));
    }
}

fn set_changed<T: Clone + PartialEq + 'static>(mut signal: Signal<T>, now: &T) {
    if *signal.peek() != *now {
        signal.set(now.clone());
    }
}

/// `run` called as from the component at `scope`, so the tasks it spawns
/// live as long as that component: the handler that asked may belong to an
/// overlay its own message closes.
fn as_from<O>(scope: ScopeId, run: impl FnOnce() -> O) -> O {
    Runtime::current().in_scope(scope, run)
}

/// `msgs` stepped in turn with every event the host sends back, the
/// requests among their effects sent through `link`: the effects left for
/// the window to carry out.
fn pump(
    view: &mut ViewState,
    link: &dyn HostLink,
    events: &mut UnboundedReceiver<Event>,
    msgs: impl IntoIterator<Item = Msg>,
) -> Vec<Effect> {
    let mut queue: std::collections::VecDeque<Msg> = msgs.into_iter().collect();
    let mut local = Vec::new();
    while let Some(msg) = queue.pop_front() {
        run(step(view, msg), link, |e| local.push(e));
        while let Ok(event) = events.try_recv() {
            queue.push_back(Msg::Host(event));
        }
    }
    local
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
    use adroit_protocol::testing::snapshot;
    use adroit_protocol::{ConfigFile, ConfigWork, FileText, SPOKEN, WorkSnapshot, agree};
    use tokio::sync::mpsc::UnboundedSender;

    use super::*;

    /// A work host holding `work`, which answers hello as one does.
    struct Answering {
        work: WorkSnapshot,
        events: UnboundedSender<Event>,
    }

    impl HostLink for Answering {
        fn send(&self, request: Request) {
            if let Request::Hello { versions } = request {
                let version = agree(SPOKEN, &versions).expect("a version both speak");
                let _ = self.events.send(Event::Welcome { version });
                let _ = self
                    .events
                    .send(Event::Snapshot(Box::new(self.work.clone())));
            }
        }
    }

    /// The window as `AppCtx` runs it, without Dioxus: each message stepped,
    /// its effects run, and every event the host sends back stepped in turn.
    struct Window {
        view: ViewState,
        link: Answering,
        events: UnboundedReceiver<Event>,
        drawn: Vec<Effect>,
    }

    impl Window {
        /// A window opened on a host that read these settings.
        fn open(settings: &str) -> Self {
            let config = ConfigWork {
                settings: Some(ConfigFile {
                    path: "/c/settings.toml".into(),
                    text: FileText::Read {
                        text: settings.into(),
                    },
                }),
                ..ConfigWork::default()
            };
            let (sent, events) = tokio::sync::mpsc::unbounded_channel();
            let link = Answering {
                work: snapshot().with(|w| w.config = config).build(),
                events: sent,
            };
            let mut window = Self {
                view: ViewState::default(),
                link,
                events,
                drawn: Vec::new(),
            };
            window.dispatch(Msg::Opened {
                home: Default::default(),
            });
            window
        }

        fn dispatch(&mut self, msg: Msg) {
            let drawn = pump(&mut self.view, &self.link, &mut self.events, [msg]);
            self.drawn.extend(drawn);
        }
    }

    /// A task an overlay's handler starts through the window, as a picker
    /// does when Enter resumes a conversation and closes it.
    #[tokio::test]
    async fn work_started_from_an_overlay_outlives_the_overlay() {
        use std::cell::Cell;

        #[derive(Clone)]
        struct Asked {
            window: ScopeId,
            done: Rc<Cell<bool>>,
        }

        #[component]
        fn Overlay(onmount: EventHandler<()>) -> Element {
            let asked = use_context::<Asked>();
            use_hook(move || {
                as_from(asked.window, || {
                    let done = asked.done.clone();
                    spawn(async move {
                        tokio::task::yield_now().await;
                        done.set(true);
                    });
                });
                onmount.call(());
            });
            rsx! { div {} }
        }

        #[component]
        fn Frame() -> Element {
            let done = use_context::<Rc<Cell<bool>>>();
            use_context_provider(|| Asked {
                window: current_scope_id(),
                done,
            });
            let mut open = use_signal(|| true);
            rsx! {
                if open() {
                    Overlay { onmount: move |_| open.set(false) }
                }
            }
        }

        let done = Rc::new(Cell::new(false));
        let mut dom = VirtualDom::new(Frame).with_root_context(done.clone());
        dom.rebuild_in_place();
        for _ in 0..4 {
            let _ = tokio::time::timeout(std::time::Duration::from_millis(50), dom.wait_for_work())
                .await;
            dom.render_immediate(&mut dioxus::core::NoOpMutations);
        }
        assert!(done.get());
    }

    #[test]
    fn an_opened_window_greets_the_host_and_holds_the_work() {
        let window = Window::open("zoom = 1.25\n");
        assert_eq!(
            window.view.host.link,
            Link::Ready {
                version: *SPOKEN.iter().max().unwrap()
            }
        );
        assert!(window.view.host.work.is_some());
        assert_eq!(window.view.config.settings.zoom, 1.25);
        assert_eq!(window.drawn, vec![Effect::Zoom(1.25)]);
    }

    #[test]
    fn the_zoom_keys_redraw_the_window_and_never_reach_the_host() {
        let mut window = Window::open("");
        window.dispatch(Msg::Zoom(Step::In));
        window.dispatch(Msg::Zoom(Step::In));
        window.dispatch(Msg::Zoom(Step::Reset));
        assert_eq!(
            window.drawn,
            vec![Effect::Zoom(1.1), Effect::Zoom(1.25), Effect::Zoom(1.0)]
        );
        assert_eq!(window.view.chrome.zoom, 1.0);
    }
}
