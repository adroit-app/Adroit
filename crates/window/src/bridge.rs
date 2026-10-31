//! The window's view state held for Dioxus: the state `step` changes, one
//! signal per slice for components to read, and the runner that carries out
//! what each message asks.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;

use adroit_connect::environments::{Catalog, Environment};
use adroit_core::actions::Action;
use adroit_core::chrome::Chrome;
use adroit_core::config::Config;
use adroit_core::console::ConsoleView;
use adroit_core::content::{Area, ContentView, Tab, kept_text, read_kept};
use adroit_core::details::{Details, Scroll};
use adroit_core::effect::Effect;
use adroit_core::host::{Host as HostView, Hosts};
use adroit_core::msg::Msg;
use adroit_core::overlay::{self, Holder, Overlays};
use adroit_core::picker::PickerView;
use adroit_core::prompt::PromptView;
use adroit_core::state::ViewState;
use adroit_core::tree::TreeView;
use adroit_core::tree::nav::Item;
use adroit_core::update::step;
use adroit_model::{EnvironmentId, Key};
use adroit_protocol::{Answer, Ask, Request};
use dioxus::core::{Runtime, current_scope_id};
use dioxus::prelude::*;

use crate::layout::{Kept as _, Layout};
use crate::link::{Events, Link, Links, Route, To, Toward, journal_root};
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

impl<L: HostLink + ?Sized> HostLink for Rc<L> {
    fn send(&self, request: Request) {
        (**self).send(request);
    }
}

/// What runs an effect handed to `App`.
type Handed<E = Effect> = Rc<dyn Fn(E)>;

/// The effects handed to `App` and what runs them. The view state takes in
/// the host's first answers before the window is drawn, so effects are
/// handed before anything runs them; they wait, and those handed later wait
/// behind them, so each runs in the order it was asked.
struct Handoff<E = Effect> {
    run: Option<Handed<E>>,
    waiting: VecDeque<E>,
}

impl<E> Default for Handoff<E> {
    fn default() -> Self {
        Self {
            run: None,
            waiting: VecDeque::new(),
        }
    }
}

impl<E> Handoff<E> {
    /// `effect` with what runs it now, or none while it waits.
    fn hand(&mut self, effect: E) -> Option<(Handed<E>, E)> {
        match &self.run {
            Some(run) if self.waiting.is_empty() => Some((run.clone(), effect)),
            _ => {
                self.waiting.push_back(effect);
                None
            }
        }
    }

    /// `run` runs the effects from now on, those waiting first.
    fn over(&mut self, run: Handed<E>) {
        self.run = Some(run);
    }

    /// The effect waiting longest, with what runs it, once something does.
    fn next_waiting(&mut self) -> Option<(Handed<E>, E)> {
        let run = self.run.clone()?;
        let effect = self.waiting.pop_front()?;
        Some((run, effect))
    }
}

/// What runs an action handed to `App`, with the key that ran it.
type Performer = Rc<dyn Fn(Action, Option<Key>)>;

/// What components read of the view state and how they change it. A slice's
/// signal is set only when `step` changed that slice, so a component reading
/// one slice redraws for it alone.
#[derive(Clone, Copy)]
pub(crate) struct AppCtx {
    view: CopyValue<ViewState>,
    links: CopyValue<Links<Rc<Link>>>,
    pub(crate) chrome: Signal<Chrome>,
    pub(crate) config: Signal<Config>,
    /// The host of the window's own environment.
    pub(crate) host: Signal<HostView>,
    /// The host of every environment the window shows.
    pub(crate) hosts: Signal<Hosts>,
    pub(crate) content: Signal<ContentView>,
    pub(crate) tree: Signal<TreeView>,
    pub(crate) details: Signal<Details>,
    pub(crate) picker: Signal<PickerView>,
    pub(crate) overlays: Signal<Overlays>,
    pub(crate) console: Signal<ConsoleView>,
    pub(crate) prompt: Signal<PromptView>,
    pub(crate) drafts_ready: Signal<bool>,
    /// What the window knows of each profile's login.
    pub(crate) logins: Signal<crate::logins::Logins>,
    /// How the window stands with each environment on a work machine, as
    /// last looked at.
    pub(crate) remotes: Signal<BTreeMap<EnvironmentId, crate::link::RemoteStatus>>,
    /// The environment whose work settings the Settings window edits.
    settings_environment: CopyValue<EnvironmentId>,
    /// The configuration last read for the Settings window from the files
    /// of an environment other than the window's own.
    settings_config: CopyValue<Option<(adroit_protocol::ConfigWork, Config)>>,
    /// The time, in milliseconds, kept going while a link to a work machine
    /// is not connected, so how old its work is counts up.
    pub(crate) clock: Signal<u64>,
    /// The environments kept whose routes could not be opened, told once
    /// each while they stay kept.
    unopened: CopyValue<BTreeSet<EnvironmentId>>,
    /// The effects whose state `App` still holds, and what runs them.
    handed: CopyValue<Handoff>,
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
        let route = Route::selected(&instance_state().join("execution.toml"));
        let environment = route.environment.clone();
        let (link, events, first) =
            Link::selected(route, work_host, journal_root(&instance_state(), None));
        let links = Links::new(environment.clone(), Rc::new(link));
        let mut events = vec![(environment.clone(), events)];
        // Drawn without the work, should the host not answer in time.
        let _ = first.recv_timeout(FIRST_WORK);
        let mut view = ViewState::default();
        // A window that kept no content yet shows the one tab it kept before
        // the content was, or else what the instance's one window kept
        // before there could be several.
        let kept = std::fs::read_to_string(content_path())
            .or_else(|_| std::fs::read_to_string(shown_path()))
            .or_else(|_| std::fs::read_to_string(instance_state().join("shown")))
            .map(|text| read_kept(&text))
            .unwrap_or_default();
        // The layout kept comes first: hiding a panel gives its keys away,
        // and a panel kept hidden was not hidden now. The host's first
        // answers are taken in before the window is drawn, so it opens in
        // the configuration's zoom and theme.
        let mut effects = pump(
            &mut view,
            &links,
            &mut events,
            [
                Msg::PanelsKept(Layout::load()),
                Msg::WindowFocused(dioxus::desktop::window().is_focused()),
                Msg::Opened {
                    home: home(),
                    environment: environment.clone(),
                },
            ],
        );
        // Where a session started with nothing open runs, as the host
        // answered.
        let scratch = view.config.scratch.clone();
        effects.extend(pump(
            &mut view,
            &links,
            &mut events,
            [Msg::ContentOpened { kept, scratch }],
        ));
        crate::client_settings::apply(&mut view.config.settings);
        view.chrome.zoom = view.config.settings.zoom;
        let app = AppCtx {
            chrome: Signal::new(view.chrome.clone()),
            config: Signal::new(view.config.clone()),
            host: Signal::new(view.hosts.own().clone()),
            hosts: Signal::new(view.hosts.clone()),
            content: Signal::new(view.content.clone()),
            tree: Signal::new(view.tree.clone()),
            details: Signal::new(view.details.clone()),
            picker: Signal::new(view.picker.clone()),
            overlays: Signal::new(view.overlays.clone()),
            console: Signal::new(view.console.clone()),
            prompt: Signal::new(view.prompt.clone()),
            drafts_ready: Signal::new(false),
            logins: Signal::new(Default::default()),
            remotes: Signal::new(BTreeMap::new()),
            clock: Signal::new(adroit_model::time::now_ms()),
            settings_environment: CopyValue::new(environment.clone()),
            settings_config: CopyValue::new(None),
            unopened: CopyValue::new(Default::default()),
            handed: CopyValue::new(Handoff::default()),
            performer: CopyValue::new(None),
            scope: current_scope_id(),
            view: CopyValue::new(view),
            links: CopyValue::new(links),
        };
        provide_context(app);
        app.look_at_remotes();
        app.run(effects);
        spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                app.refresh_client_settings();
                app.follow_catalog();
                app.look_at_remotes();
            }
        });
        for (environment, events) in events {
            app.listen(environment, events);
        }
        let mut asked = crate::environments::listen();
        spawn(async move {
            while let Some(environment) = asked.recv().await {
                app.show_environment(&environment);
                // Asked from the Environments window too, which stays in
                // front otherwise.
                let window = dioxus::desktop::window();
                window.set_visible(true);
                window.set_focus();
            }
        });
        // Not while the window is first drawn: listing one changes what is
        // drawn.
        spawn(async move { app.follow_catalog() });
        app
    })
}

impl AppCtx {
    /// Dispatch every event `events` tells as `environment`'s host's, for
    /// as long as the window.
    fn listen(&self, environment: EnvironmentId, mut events: Events) {
        let app = *self;
        as_from(self.scope, || {
            spawn(async move {
                while let Some(event) = events.recv().await {
                    // A link dropped may still tell what it was telling.
                    if !app.links.peek().shows(&environment) {
                        break;
                    }
                    app.dispatch(Msg::Host {
                        environment: environment.clone(),
                        event,
                    });
                }
            })
        });
    }

    /// Select `environment`'s heading, once the window lists what the
    /// catalog keeps now; one archived is not listed, and says so.
    pub(crate) fn show_environment(&self, environment: &Environment) {
        self.follow_catalog();
        let id = EnvironmentId::new(&environment.id);
        if self.links.peek().shows(&id) {
            self.dispatch(Msg::TreeSelect(Some(Item::Environment(id))));
        } else if environment.archived {
            self.notice(format!("{} is removed; restore it first", environment.name));
        }
    }

    /// List every environment the catalog keeps, and only those: one added
    /// is linked, one archived or deleted is closed, and one renamed is
    /// called by its new name. A catalog that cannot be read changes nothing.
    fn follow_catalog(&self) {
        let Ok(catalog) = Catalog::load(&Catalog::path()) else {
            return;
        };
        let following = {
            let links = self.links.peek();
            let linked = links.each().map(|(environment, _)| environment);
            crate::environments::follow(&catalog, linked, links.own_environment())
        };
        let mut unopened = self.unopened;
        unopened.write().retain(|id| {
            following
                .link
                .iter()
                .any(|e| EnvironmentId::new(&e.id) == *id)
        });
        for environment in following.unlink {
            self.dispatch(Msg::EnvironmentRemoved(environment));
        }
        for environment in &following.link {
            self.link_environment(environment);
        }
        for environment in catalog.environments.iter().filter(|e| !e.archived) {
            let id = EnvironmentId::new(&environment.id);
            let renamed =
                self.links.peek().shows(&id) && self.hosts.peek().name(&id) != environment.name;
            if renamed {
                self.dispatch(Msg::EnvironmentOpened {
                    environment: id,
                    name: environment.name.clone(),
                });
            }
        }
    }

    /// Link `environment` beside those the window lists, its host greeted.
    /// A route that cannot be opened is tried again at the next look and
    /// told only the first time.
    fn link_environment(&self, environment: &Environment) {
        let id = EnvironmentId::new(&environment.id);
        let (link, events) = match open_shown(environment) {
            Ok(opened) => opened,
            Err(text) => {
                let mut unopened = self.unopened;
                if unopened.write().insert(id) {
                    self.notice(format!("cannot open {}: {text}", environment.name));
                }
                return;
            }
        };
        let mut unopened = self.unopened;
        unopened.write().remove(&id);
        let mut links = self.links;
        links.write().add(id.clone(), || Rc::new(link));
        self.listen(id.clone(), events);
        self.keep(crate::hosts::look(*self, id.clone()));
        if let Err(e) = crate::editor::install_shown(
            &crate::panes::adroit_program(),
            &crate::window_socket::windows(),
            environment,
        ) {
            self.status(Some(format!("cannot write the editor script: {e}")));
        }
        self.dispatch(Msg::EnvironmentOpened {
            environment: id,
            name: environment.name.clone(),
        });
    }

    /// The link to `environment`, the window's own for one it does not show.
    fn link_of(&self, environment: &EnvironmentId) -> Rc<Link> {
        self.links.peek().of(environment).clone()
    }

    fn own_link(&self) -> Rc<Link> {
        self.links.peek().own().clone()
    }

    /// The terminal host of `environment`, where its sessions run.
    pub(crate) fn daemon_of(&self, environment: &EnvironmentId) -> adroit_term::Daemon {
        self.link_of(environment).daemon()
    }

    /// How the window stands with `environment`'s work machine, none for
    /// one on this computer.
    pub(crate) fn remote_status_of(
        &self,
        environment: &EnvironmentId,
    ) -> Option<crate::link::RemoteStatus> {
        self.link_of(environment).remote_status()
    }

    /// How the window stands now with each environment it shows that is on
    /// a work machine.
    pub(crate) fn remote_statuses(&self) -> BTreeMap<EnvironmentId, crate::link::RemoteStatus> {
        let links = self.links.peek();
        links
            .each()
            .filter_map(|(environment, link)| Some((environment.clone(), link.remote_status()?)))
            .collect()
    }

    /// Look again at how the window stands with each environment on a work
    /// machine.
    fn look_at_remotes(&self) {
        let remotes = self.remote_statuses();
        let mut clock = self.clock;
        if remotes
            .values()
            .any(|s| s.phase != crate::link::RemotePhase::Connected)
        {
            clock.set(adroit_model::time::now_ms());
        }
        set_changed(self.remotes, &remotes);
    }

    pub(crate) fn retry_remote(&self, environment: &EnvironmentId) {
        self.link_of(environment).retry_remote();
    }
    pub(crate) fn query_remote(
        &self,
        environment: &EnvironmentId,
        stamp: adroit_protocol::MutationStamp,
    ) {
        self.link_of(environment).query_remote(stamp);
    }
    pub(crate) fn remote_draft(
        &self,
        environment: &EnvironmentId,
        stamp: adroit_protocol::MutationStamp,
        forget: bool,
        checked: bool,
    ) -> impl std::future::Future<Output = Result<(), String>> + use<> {
        self.link_of(environment)
            .remote_draft(stamp, forget, checked)
    }
    pub(crate) fn resolve_remote(
        &self,
        environment: &EnvironmentId,
        stamp: adroit_protocol::MutationStamp,
        checked: bool,
    ) -> impl std::future::Future<Output = Result<(), String>> + use<> {
        self.link_of(environment).resolve_remote(stamp, checked)
    }
    /// `msg` applied to the view state, the slices it changed shown, and
    /// what it asks carried out. Called from handlers and tasks, never while
    /// rendering.
    pub(crate) fn dispatch(&self, msg: Msg) {
        let mut view = self.view;
        let from_host = matches!(msg, Msg::Host { .. });
        let mut effects = step(&mut view.write(), msg);
        if from_host {
            crate::client_settings::apply(&mut view.write().config.settings);
            let zoom = view.read().config.settings.zoom;
            view.write().chrome.zoom = zoom;
            for effect in &mut effects {
                if let Effect::Zoom(level) = effect {
                    *level = zoom;
                }
            }
        }
        self.publish();
        as_from(self.scope, || self.run(effects));
    }

    fn refresh_client_settings(&self) {
        let mut view = self.view;
        let mut next = view.read().config.settings.clone();
        crate::client_settings::apply(&mut next);
        if next != view.read().config.settings {
            let zoom = next.zoom;
            view.write().config.settings = next;
            view.write().chrome.zoom = zoom;
            self.publish();
            let _ = dioxus::desktop::window().webview.zoom(zoom);
        }
    }

    fn publish(&self) {
        let view = self.view.read();
        set_changed(self.chrome, &view.chrome);
        set_changed(self.config, &view.config);
        crate::feed::publish(&self.settings_config(&view));
        set_changed(self.host, view.hosts.own());
        set_changed(self.hosts, &view.hosts);
        set_changed(self.content, &view.content);
        set_changed(self.tree, &view.tree);
        set_changed(self.details, &view.details);
        set_changed(self.picker, &view.picker);
        set_changed(self.overlays, &view.overlays);
        set_changed(self.console, &view.console);
        set_changed(self.prompt, &view.prompt);
    }

    /// Bring the Settings window forward on the work settings of
    /// `environment`, opening it when it is not open on them.
    pub(crate) fn open_settings(&self, environment: &EnvironmentId) {
        let mut edited = self.settings_environment;
        edited.set(environment.clone());
        self.publish();
        let own = *environment == self.environment();
        let data_dir = if own {
            crate::environments::work_data_dir()
        } else {
            crate::environments::data_dir_of(environment)
        };
        let name = if own {
            crate::environments::active_name()
        } else {
            self.environment_name(environment)
        };
        let link = (*self.link_of(environment)).clone();
        crate::settings_window::open(
            self.agents_of(environment),
            link,
            crate::settings_window::Edited {
                environment: environment.clone(),
                name,
                data_dir,
            },
        );
    }

    /// The configuration the Settings window shows: that of the
    /// environment it edits, with this computer's preferences over it.
    fn settings_config(&self, view: &ViewState) -> Config {
        let environment = self.settings_environment.peek().clone();
        if environment == view.hosts.environment {
            return view.config.clone();
        }
        let files = view
            .hosts
            .work(&environment)
            .map(|w| w.config.clone())
            .unwrap_or_default();
        let mut kept = self.settings_config;
        if let Some((was, config)) = &*kept.peek()
            && *was == files
        {
            return config.clone();
        }
        let mut config = Config::of_work(&files);
        crate::client_settings::apply(&mut config.settings);
        kept.set(Some((files, config.clone())));
        config
    }

    /// Run `task` for as long as the window, whatever component started it.
    pub(crate) fn keep(&self, task: impl std::future::Future<Output = ()> + 'static) {
        as_from(self.scope, || {
            spawn(task);
        });
    }

    /// Ask the host `ask`, and what came of it.
    pub(crate) fn ask(&self, ask: Ask) -> impl std::future::Future<Output = Answer> + use<> {
        self.own_link().ask(ask)
    }

    /// Ask `environment`'s host `ask`, and what came of it.
    pub(crate) fn ask_in(
        &self,
        environment: &EnvironmentId,
        ask: Ask,
    ) -> impl std::future::Future<Output = Answer> + use<> {
        self.link_of(environment).ask(ask)
    }

    /// Tell `environment`'s host what the view state does not hold.
    pub(crate) fn tell_host_in(&self, environment: &EnvironmentId, request: Request) {
        self.link_of(environment).send(request);
    }

    pub(crate) fn ask_prompt(
        &self,
        environment: &EnvironmentId,
        ask: Ask,
        prompt: adroit_connect::journal::PromptContext,
    ) -> impl std::future::Future<Output = Answer> + use<> {
        self.link_of(environment).ask_with_prompt(ask, Some(prompt))
    }

    /// Hand the effects whose state still lives in `App` to `run`: the
    /// panes, the prompt and the console, terminals started and images shown.
    pub(crate) fn hand_over(&self, run: impl Fn(Effect) + 'static) {
        let mut handed = self.handed;
        handed.write().over(Rc::new(run));
        // Not run here: this is called while the window is drawn, and the
        // effects change what is drawn.
        as_from(self.scope, || {
            spawn(async move {
                loop {
                    let next = handed.write().next_waiting();
                    let Some((run, effect)) = next else {
                        break;
                    };
                    run(effect);
                }
            })
        });
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
        // Taken out, as a request run may show another environment.
        let links = self.links.peek().clone();
        let local = |effect: Effect| match refused(&effect, |e| self.remote_status_of(e).is_some())
        {
            Some(why) => self.notice(why.into()),
            None => self.run_local(effect),
        };
        run(effects, &links, local);
    }

    /// Carry out `effect`, one the window does itself rather than a host.
    fn run_local(&self, effect: Effect) {
        match effect {
            Effect::Zoom(level) => {
                let _ = dioxus::desktop::window().webview.zoom(level);
            }
            Effect::Open { target, .. } => open([target]),
            Effect::Reveal { path, .. } => open(["-R".into(), path]),
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
            Effect::ShowSession(environment, id) => {
                self.dispatch(Msg::ShowSession(environment, id))
            }
            Effect::Show { thing, how } => self.dispatch(Msg::Show {
                thing,
                place: None,
                how: Some(how),
            }),
            Effect::KeepContent(kept) => {
                let path = content_path();
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if std::fs::write(path, kept_text(&kept)).is_ok() {
                    let _ = std::fs::remove_file(shown_path());
                }
            }
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
            Effect::OpenSettings(environment) => self.open_settings(&environment),
            Effect::EnvironmentClosed(environment) => {
                let mut links = self.links;
                links.write().remove(&environment);
            }
            Effect::ArchiveEnvironment(environment) => {
                match crate::environments::archive(&Catalog::path(), &environment) {
                    Ok(()) => self.follow_catalog(),
                    Err(e) => self.status(Some(format!("cannot remove the environment: {e}"))),
                }
            }
            Effect::OpenEnvironments => crate::environment_manager::open(),
            Effect::CloseWindow => dioxus::desktop::window().close(),
            Effect::OpenWindow => {
                if let Err(e) = open_another_window() {
                    self.notice(format!("cannot open another window: {e}"));
                }
            }
            effect @ (Effect::Viewer { .. }
            | Effect::TakeSuggestion { .. }
            | Effect::EndShell { .. }
            | Effect::FilterFocus(_)
            | Effect::PromptOffer(_)
            | Effect::PromptCommands(_)
            | Effect::Palette(_)
            | Effect::PromptRoot(_)
            | Effect::StartTerminal(_)
            | Effect::OpenLogin { .. }
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
            | Effect::KeepImageFiles(_)
            | Effect::AskInput(_)
            | Effect::PromptRecall { .. }
            | Effect::RemoveImage(_)
            | Effect::TakePrompt
            | Effect::SendPrompt(_)
            | Effect::StartDraft(_)
            | Effect::PromptSent
            | Effect::PromptRestore { .. }
            | Effect::PromptShow(_)
            | Effect::Resume { .. }
            | Effect::RestartDaemon(_)
            | Effect::TypeInto { .. }) => self.hand(effect),
            Effect::Perform(action) => self.perform(action, None),
            Effect::PerformKey(action, key) => self.perform(action, Some(key)),
            Effect::AskInputAfter(after) => self.later(after, Msg::AskInput),
            Effect::LapsePull { pull, after } => self.later(after, Msg::PullLapsed(pull)),
            Effect::LapseCompose { asked, after } => {
                self.later(after, Msg::ComposeLapsed(asked));
            }
            Effect::KeepLayout(layout) => layout.save(),
            Effect::ScrollKeysHelp(lines) => crate::keys_help_view::scroll(lines),
            Effect::LapseNotice {
                notice,
                text,
                after,
            } => self.later(after, Msg::NoticeLapsed { notice, text }),
            Effect::Host { .. } => unreachable!("requests go to the host link"),
        }
    }

    /// Dispatch `msg` once `after` has passed.
    fn later(&self, after: std::time::Duration, msg: Msg) {
        let app = *self;
        spawn(async move {
            tokio::time::sleep(after).await;
            app.dispatch(msg);
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
        let mut handed = self.handed;
        let now = handed.write().hand(effect);
        if let Some((run, effect)) = now {
            run(effect);
        }
    }
}

/// Why the window does not carry out `effect` itself, where `remote` says
/// which environments are on a work machine: a path of such an environment
/// is not on this computer for a local application, the Finder or a viewer
/// to open, while an address opens from anywhere and a path of an
/// environment on this computer still opens.
fn refused(effect: &Effect, remote: impl Fn(&EnvironmentId) -> bool) -> Option<&'static str> {
    match effect {
        Effect::Open {
            environment,
            target,
        } if remote(environment)
            && !target.starts_with("https://")
            && !target.starts_with("http://") =>
        {
            Some("Opening a remote path in a local application is unavailable")
        }
        Effect::Reveal { environment, .. } if remote(environment) => {
            Some("This path is on the work machine and cannot be revealed here")
        }
        Effect::Viewer { environment, .. } if remote(environment) => {
            Some("Remote file previews are unavailable")
        }
        _ => None,
    }
}

/// `/usr/bin/open` run with `args`, without waiting for it.
pub(crate) fn open<const N: usize>(args: [String; N]) {
    run_detached("/usr/bin/open", args);
}

/// `program` run with `args`, without waiting for it.
pub(crate) fn run_detached(
    program: &str,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args);
    spawn(async move {
        let _ = cmd.status().await;
    });
}

/// The work host listening at `socket` for an environment the window
/// shows, its own or one beside it, started as this program serving the
/// work instance `work` with its terminal host at `terminal`. Its routing is
/// named rather than inherited: the window process carries none, as it
/// shows several environments.
fn work_host(socket: std::path::PathBuf, terminal: &std::path::Path, work: &str) -> To {
    let mut args: Vec<String> = ["ADROIT_ENVIRONMENT", "ADROIT_REMOTE"]
        .into_iter()
        .flat_map(|name| ["-u".to_string(), name.to_string()])
        .collect();
    args.extend([
        format!("ADROIT_WORK_INSTANCE={work}"),
        format!("ADROIT_HOST={}", socket.display()),
        format!("ADROIT_DAEMON={}", terminal.display()),
        crate::panes::adroit_program()
            .to_string_lossy()
            .into_owned(),
        "host".into(),
    ]);
    To {
        socket,
        program: "/usr/bin/env".into(),
        args,
        build: crate::build::build(),
    }
}

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

/// Where this window keeps its content, under its number.
fn content_path() -> std::path::PathBuf {
    crate::window_socket::kept_path("content.json")
}

/// Where this window kept the one tab its focused window showed, before it
/// kept the whole content.
fn shown_path() -> std::path::PathBuf {
    crate::window_socket::kept_path("shown")
}

impl AppCtx {
    /// The environment the window's work belongs to.
    pub(crate) fn environment(&self) -> EnvironmentId {
        self.view.read().hosts.environment.clone()
    }

    /// What `environment`, one the window shows, is called.
    pub(crate) fn environment_name(&self, environment: &EnvironmentId) -> String {
        self.view.read().hosts.name(environment)
    }

    /// What the content's focused window shows now.
    pub(crate) fn active(&self) -> Option<Tab> {
        self.content.peek().active().cloned()
    }

    /// The area that has the keys now.
    pub(crate) fn focus(&self) -> Area {
        self.content.peek().focus
    }

    /// The environment of what the content's focused window shows, the
    /// window's own with nothing open.
    pub(crate) fn active_environment(&self) -> EnvironmentId {
        let active = self.active();
        self.hosts.peek().environment_of(active.as_ref()).clone()
    }

    /// The environment of what the middle shows, read so a component or an
    /// effect follows it: the details panel and the prompt read from its
    /// host.
    pub(crate) fn shown_environment(&self) -> EnvironmentId {
        adroit_core::content::shown_environment_of(&self.content.read(), &self.hosts.read())
    }

    /// What `environment`'s work host told of each agent.
    pub(crate) fn agents_of(&self, environment: &EnvironmentId) -> adroit_protocol::AgentsWork {
        self.hosts
            .peek()
            .work(environment)
            .map(|w| w.agents.clone())
            .unwrap_or_default()
    }

    /// The agent the prompt of `tab` goes to, as the host of its
    /// environment lists its sessions, read so a component or an effect
    /// follows it.
    pub(crate) fn agent_of(&self, tab: Option<&Tab>) -> adroit_model::Agent {
        let hosts = self.hosts.read();
        let tree = adroit_core::tree::listed(hosts.work(hosts.environment_of(tab)));
        let settings = &self.config.read().settings;
        self.content.read().drafts.agent_of(tree, tab, settings)
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

/// `msgs` stepped in turn with every event the hosts send back, each as its
/// environment's, the requests among their effects sent through `links`: the
/// effects left for the window to carry out.
fn pump(
    view: &mut ViewState,
    links: &Links<impl HostLink>,
    events: &mut [(EnvironmentId, Events)],
    msgs: impl IntoIterator<Item = Msg>,
) -> Vec<Effect> {
    let mut queue: std::collections::VecDeque<Msg> = msgs.into_iter().collect();
    let mut local = Vec::new();
    while let Some(msg) = queue.pop_front() {
        run(step(view, msg), links, |e| local.push(e));
        for (environment, events) in events.iter_mut() {
            while let Ok(event) = events.try_recv() {
                queue.push_back(Msg::Host {
                    environment: environment.clone(),
                    event,
                });
            }
        }
    }
    local
}

/// `effects` carried out in order: requests sent through the link of the
/// environment they are for, the rest done by the window through `local`.
fn run(effects: Vec<Effect>, links: &Links<impl HostLink>, mut local: impl FnMut(Effect)) {
    for effect in effects {
        match effect {
            Effect::Host {
                environment,
                request,
            } => match links.get(&environment) {
                Some(link) => link.send(request),
                None => eprintln!(
                    "adroit: no link to environment {environment:?} for {request:?}; dropped"
                ),
            },
            effect => local(effect),
        }
    }
}

/// A link to `environment`, shown beside the window's own, and what its
/// host tells.
fn open_shown(environment: &Environment) -> Result<(Link, Events), String> {
    let route = Route::shown(
        environment,
        &adroit_model::paths::instance(),
        &adroit_model::paths::state_dir(),
        Toward::own(),
    )?;
    let journal = journal_root(&instance_state(), Some(&route.environment));
    let (link, events, _) = Link::selected(route, work_host, journal);
    Ok((link, events))
}

#[cfg(test)]
mod tests {
    use adroit_core::host::Link;
    use adroit_core::zoom::Step;
    use adroit_protocol::Event;
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
    /// its effects run, and every event a host sends back stepped in turn.
    struct Window {
        view: ViewState,
        links: Links<Answering>,
        events: Vec<(EnvironmentId, Events)>,
        drawn: Vec<Effect>,
    }

    /// The work of a host that read these settings.
    fn work_reading(settings: &str) -> WorkSnapshot {
        let config = ConfigWork {
            settings: Some(ConfigFile {
                path: "/c/settings.toml".into(),
                text: FileText::Read {
                    text: settings.into(),
                },
            }),
            ..ConfigWork::default()
        };
        snapshot().with(|w| w.config = config).build()
    }

    /// A host holding `work`, and the events it sends back.
    fn answering(work: WorkSnapshot) -> (Answering, Events) {
        let (sent, events) = tokio::sync::mpsc::unbounded_channel();
        let link = Answering { work, events: sent };
        (link, Events::Local(events))
    }

    impl Window {
        /// A window opened on a host that read these settings.
        fn open(settings: &str) -> Self {
            let own = EnvironmentId::default();
            let (link, events) = answering(work_reading(settings));
            let mut window = Self {
                view: ViewState::default(),
                links: Links::new(own.clone(), link),
                events: vec![(own.clone(), events)],
                drawn: Vec::new(),
            };
            window.dispatch(Msg::Opened {
                home: Default::default(),
                environment: own,
            });
            window
        }

        /// The window links to `environment` too, whose host holds `work`.
        fn link(&mut self, environment: EnvironmentId, work: WorkSnapshot) {
            let (link, events) = answering(work);
            self.links.add(environment.clone(), || link);
            self.events.push((environment, events));
        }

        fn dispatch(&mut self, msg: Msg) {
            let drawn = pump(&mut self.view, &self.links, &mut self.events, [msg]);
            self.drawn.extend(drawn);
        }
    }

    #[test]
    fn each_environment_s_host_is_greeted_through_its_own_link_and_its_work_kept_as_its_own() {
        let mut window = Window::open("zoom = 1.25\n");
        let there = EnvironmentId::new("there");
        window.link(there.clone(), work_reading("zoom = 2.0\n"));
        window.dispatch(Msg::EnvironmentOpened {
            environment: there.clone(),
            name: "There".into(),
        });
        assert_eq!(
            window.view.hosts.work(&there),
            Some(&work_reading("zoom = 2.0\n"))
        );
        assert_eq!(
            window.view.hosts.own().work,
            Some(work_reading("zoom = 1.25\n"))
        );
        assert_eq!(
            window.view.hosts.get(&there).map(|h| &h.link),
            Some(&Link::Ready {
                version: *SPOKEN.iter().max().unwrap()
            })
        );
    }

    #[test]
    fn a_work_host_starts_in_the_work_instance_its_route_names_with_its_sockets_named() {
        let to = work_host(
            "/w/host/host.sock".into(),
            std::path::Path::new("/w/felis/daemon.sock"),
            "work",
        );
        assert_eq!(to.program, std::path::Path::new("/usr/bin/env"));
        let (named, run) = to.args.split_at(to.args.len() - 2);
        assert!(named.contains(&"ADROIT_WORK_INSTANCE=work".to_string()));
        assert!(named.contains(&"ADROIT_HOST=/w/host/host.sock".to_string()));
        assert!(named.contains(&"ADROIT_DAEMON=/w/felis/daemon.sock".to_string()));
        assert!(
            named
                .windows(2)
                .any(|pair| pair == ["-u", "ADROIT_ENVIRONMENT"]),
            "the window's own environment is not handed down: {named:?}"
        );
        assert_eq!(run[1], "host");
    }

    #[test]
    fn the_window_s_own_work_host_is_started_with_its_routing_named_as_an_added_one_s_is() {
        let route = Route::bound(
            &adroit_connect::environments::Binding {
                environment: "own".into(),
                name: "Own".into(),
                target: adroit_connect::Target::Local,
                scope: Some("scope-a".into()),
                client: "client".into(),
                work_instance: Some("work".into()),
            },
            "client",
            std::path::Path::new("/state"),
            crate::link::Toward::own(),
        );
        let Ok(crate::link::Toward::Local {
            host,
            terminal,
            work,
        }) = route.toward
        else {
            panic!("a local route");
        };
        let args = work_host(host, &terminal, &work).args;
        assert!(
            args.contains(&"ADROIT_WORK_INSTANCE=work".to_string()),
            "{args:?}"
        );
        assert!(
            args.contains(&"ADROIT_HOST=/state/adroit/instances/work/host/host.sock".to_string()),
            "{args:?}"
        );
    }

    #[test]
    fn each_request_goes_to_the_link_of_its_environment_and_one_to_none_is_dropped() {
        #[derive(Default)]
        struct Recording(std::cell::RefCell<Vec<Request>>);
        impl HostLink for Recording {
            fn send(&self, request: Request) {
                self.0.borrow_mut().push(request);
            }
        }
        let mut links = Links::new(EnvironmentId::new("here"), Recording::default());
        links.add(EnvironmentId::new("there"), Recording::default);
        let mut local = Vec::new();
        run(
            vec![
                Effect::Host {
                    environment: EnvironmentId::new("there"),
                    request: Request::Relist,
                },
                Effect::Host {
                    environment: EnvironmentId::new("here"),
                    request: Request::Snapshot,
                },
                Effect::Host {
                    environment: EnvironmentId::new("nowhere"),
                    request: Request::ReadAgain,
                },
            ],
            &links,
            |effect| local.push(effect),
        );
        let sent = |environment: &str| {
            links
                .get(&EnvironmentId::new(environment))
                .unwrap()
                .0
                .borrow()
                .clone()
        };
        assert_eq!(sent("here"), vec![Request::Snapshot]);
        assert_eq!(sent("there"), vec![Request::Relist]);
        assert!(local.is_empty());
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
    fn a_local_environment_s_files_open_here_while_a_remote_one_is_shown_too() {
        let (here, there) = (EnvironmentId::new("here"), EnvironmentId::new("there"));
        let remote = |environment: &EnvironmentId| *environment == there;
        let window = adroit_core::content::WindowId::default();
        let action = adroit_core::viewers::ViewerAction::ZoomIn;
        // (effect, refused)
        let cases = [
            (
                Effect::Open {
                    environment: here.clone(),
                    target: "/w/a.rs".into(),
                },
                false,
            ),
            (
                Effect::Reveal {
                    environment: here.clone(),
                    path: "/w/a.rs".into(),
                },
                false,
            ),
            (
                Effect::Viewer {
                    environment: here.clone(),
                    window,
                    action,
                },
                false,
            ),
            (
                Effect::Open {
                    environment: there.clone(),
                    target: "https://a.dev".into(),
                },
                false,
            ),
            (
                Effect::Open {
                    environment: there.clone(),
                    target: "/w/a.rs".into(),
                },
                true,
            ),
            (
                Effect::Reveal {
                    environment: there.clone(),
                    path: "/w/a.rs".into(),
                },
                true,
            ),
            (
                Effect::Viewer {
                    environment: there.clone(),
                    window,
                    action,
                },
                true,
            ),
        ];
        for (effect, want) in cases {
            assert_eq!(refused(&effect, remote).is_some(), want, "{effect:?}");
        }
    }

    #[test]
    fn an_opened_window_greets_the_host_and_holds_the_work() {
        let window = Window::open("zoom = 1.25\n");
        assert_eq!(
            window.view.hosts.own().link,
            Link::Ready {
                version: *SPOKEN.iter().max().unwrap()
            }
        );
        assert!(window.view.hosts.own().work.is_some());
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

    #[test]
    fn effects_handed_before_anything_runs_them_run_first_and_in_order_once_it_is_there() {
        let ran = Rc::new(std::cell::RefCell::new(Vec::new()));
        let run_all = |handoff: &mut Handoff<u8>| {
            while let Some((run, effect)) = handoff.next_waiting() {
                run(effect);
            }
        };
        let mut handoff = Handoff::default();
        assert!(handoff.hand(1).is_none());
        assert!(handoff.hand(2).is_none());
        handoff.over(Rc::new({
            let ran = ran.clone();
            move |effect| ran.borrow_mut().push(effect)
        }));
        assert!(
            handoff.hand(3).is_none(),
            "waits behind those handed before"
        );
        run_all(&mut handoff);
        let (run, effect) = handoff.hand(4).expect("nothing waits, so it runs now");
        run(effect);
        run_all(&mut handoff);
        assert_eq!(*ran.borrow(), [1, 2, 3, 4]);
    }
}
