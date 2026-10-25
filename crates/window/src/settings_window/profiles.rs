//! The Agents pages of the Settings window: a page for each profile sessions
//! start from, with the login it keeps, and one to make a profile.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adroit_core::agent_pages::AgentPage;
use adroit_core::profiles::{self, Login, LoginFolder};
use adroit_model::Agent;
use adroit_model::config::{CodexOptions, Launch, OpenCodeOptions, PiOptions, SessionOptions};
use adroit_model::paths::tilde;
use adroit_model::profile::{AgentProfile, LoginStanding, ProfileId};
use adroit_protocol::{AgentsWork, Answer, Ask, Request};
use dioxus::prelude::*;

use super::agents::{CodexGroup, OpenCodeGroup, PiGroup, SessionGroup};
use super::{Change, LaunchGroup};
use crate::components::{SettingGroup, SettingRow, SettingsError, TextField};
use crate::icons::{Icon, IconView};
use crate::link::{Link, failure, refusal};
use crate::settings::{AgentOptions, Settings};

/// A change to the profiles that may be refused, with what came of it when
/// it is not.
type Attempt<T> = Rc<dyn Fn(&mut Settings) -> Result<T, String>>;

/// Make `op` the change, run on the settings as they are when it is made,
/// and say what it gave; when it is refused, the settings stay as they are
/// and `refused` says why.
fn attempt<T: 'static>(
    change: Callback<Change>,
    mut refused: Signal<Option<String>>,
    op: Attempt<T>,
) -> Option<T> {
    let came = Rc::new(std::cell::RefCell::new(None));
    {
        let came = came.clone();
        change(Box::new(move |s: &mut Settings| {
            let mut trial = s.clone();
            let result = op(&mut trial);
            if result.is_ok() {
                *s = trial;
            }
            *came.borrow_mut() = Some(result);
        }));
    }
    match came.take() {
        Some(Ok(made)) => {
            refused.set(None);
            Some(made)
        }
        Some(Err(why)) => {
            refused.set(Some(why));
            None
        }
        None => None,
    }
}

/// Make `edit` to the profile `id` the change; a profile gone by then is
/// left gone.
fn edit_profile(
    change: Callback<Change>,
    id: &ProfileId,
    edit: impl FnOnce(&mut AgentProfile) + 'static,
) {
    let id = id.clone();
    change(Box::new(move |s: &mut Settings| {
        if let Ok(p) = profiles::profile_mut(s, &id) {
            edit(p);
        }
    }));
}

/// The folders that exist, written as the system writes them through their
/// links, for a login's folder to be written once that way.
fn canonical(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

/// What the login of a profile is known as, kept while the pages change so
/// a login is not asked again on each: by the profile and the folder it
/// keeps its login in, so a profile whose login changes is asked anew.
type Probes = HashMap<(ProfileId, Option<PathBuf>), Probed>;

/// The folder of its own each profile had before it was given its
/// terminal's login while Settings is open, to take again when its own is
/// chosen back.
pub(super) type LeftHomes = HashMap<ProfileId, PathBuf>;

/// The page under Agents that is `page`, `choose` going to another; `left`
/// the folders profiles left.
#[component]
pub(super) fn AgentsPane(
    page: AgentPage,
    now: Settings,
    agents: AgentsWork,
    change: Callback<Change>,
    choose: Callback<AgentPage>,
    left: Signal<LeftHomes>,
) -> Element {
    let probed = use_signal(Probes::new);
    // What became of a profile deleted, told on the page shown after.
    let mut told = use_signal(|| None::<(AgentPage, String)>);
    use_effect(use_reactive(&page, move |page| {
        if told.peek().as_ref().is_some_and(|(on, _)| *on != page) {
            told.set(None);
        }
    }));
    let notice = told
        .read()
        .as_ref()
        .filter(|(on, _)| *on == page)
        .map(|(_, said)| said.clone());
    rsx! {
        if let Some(said) = notice {
            div { class: "settings-notice", role: "status", "{said}" }
        }
        match page {
            AgentPage::Profile(id) => rsx! {
                ProfilePage { key: "profile-{id}", id, now, agents, change, choose, probed, left, told }
            },
            AgentPage::NewProfile => rsx! {
                NewProfilePage { now, change, choose }
            },
        }
    }
}

/// The page's title, with `mark` beside it when there is one.
#[component]
fn PageHead(title: String, #[props(default)] mark: Option<String>) -> Element {
    rsx! {
        div { class: "settings-page-head",
            h1 { "{title}" }
            if let Some(mark) = mark {
                span { class: "binding-mark", "{mark}" }
            }
        }
    }
}

/// Everything about the profile `id`: its name, agent and login, its new
/// sessions' options and its launch, and what can be done to it.
#[component]
fn ProfilePage(
    id: ProfileId,
    now: Settings,
    agents: AgentsWork,
    change: Callback<Change>,
    choose: Callback<AgentPage>,
    probed: Signal<Probes>,
    left: Signal<LeftHomes>,
    told: Signal<Option<(AgentPage, String)>>,
) -> Element {
    let refused = use_signal(|| None::<String>);
    let mut reset = use_signal(|| None::<String>);
    let deleting = use_signal(|| false);
    let Some(profile) = now.profiles.iter().find(|p| p.id == id).cloned() else {
        return rsx! {};
    };
    let agent = profile.agent();
    let default = now.profile == profile.id;
    let program = agents
        .of(agent)
        .map(|f| f.program.clone())
        .unwrap_or_default();
    let act = move |op: Attempt<()>| attempt(change, refused, op).is_some();
    let mut switch_agent = {
        let id = id.clone();
        move |to: Agent| {
            let id = id.clone();
            let op: Attempt<Option<String>> = Rc::new(move |s| profiles::change_agent(s, &id, to));
            if let Some(said) = attempt(change, refused, op) {
                reset.set(said);
            }
        }
    };
    let duplicate = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            let op: Attempt<ProfileId> = Rc::new(move |s| {
                profiles::duplicate_profile(s, &id, &adroit_model::paths::data_dir(), canonical)
            });
            if let Some(copy) = attempt(change, refused, op) {
                choose(AgentPage::Profile(copy));
            }
        }
    };
    let set_login = {
        let (id, home) = (id.clone(), profile.home.clone());
        let mut left = left;
        move |login: Login| {
            let login = match login {
                Login::Own if home.is_none() => left
                    .peek()
                    .get(&id)
                    .cloned()
                    .map_or(Login::Own, Login::Again),
                login => login,
            };
            let to_terminal = login == Login::Terminal;
            let op_id = id.clone();
            let changed = act(Rc::new(move |s| {
                profiles::set_login(
                    s,
                    &op_id,
                    login.clone(),
                    &adroit_model::paths::data_dir(),
                    canonical,
                )
            }));
            if changed {
                match (&home, to_terminal) {
                    (Some(had), true) => {
                        left.write().insert(id.clone(), had.clone());
                    }
                    (None, false) => {
                        left.write().remove(&id);
                    }
                    _ => {}
                }
            }
        }
    };
    let folder = profiles::login_folder(&profile, &adroit_model::paths::data_dir(), canonical);
    rsx! {
        PageHead {
            title: profile.label().to_string(),
            mark: default.then(|| "Used for new sessions".to_string()),
        }
        p { class: "settings-agent-note",
            "Starts {agent.label()}. Changes reach new sessions; running ones keep theirs."
        }
        div { class: "settings-page-actions",
            if !default {
                button {
                    r#type: "button",
                    class: "primary",
                    title: "New sessions start from this profile unless the draft picks another",
                    onclick: {
                        let id = id.clone();
                        move |_| {
                            let id = id.clone();
                            act(Rc::new(move |s| profiles::make_default(s, &id)));
                        }
                    },
                    IconView { icon: Icon::Check, size: 13 }
                    " Use for new sessions"
                }
            }
            button { r#type: "button", onclick: duplicate,
                IconView { icon: Icon::Copy, size: 13 }
                " Duplicate"
            }
            if !default {
                button {
                    r#type: "button",
                    class: "danger",
                    disabled: deleting(),
                    onclick: {
                        let mut deleting = deleting;
                        move |_| deleting.set(true)
                    },
                    "Delete…"
                }
            }
        }
        if let Some(why) = refused() {
            SettingsError { "{why}" }
        }
        if deleting() {
            DeleteProfile {
                id: id.clone(),
                name: profile.label().to_string(),
                folder,
                then: AgentPage::Profile(now.profile.clone()),
                change,
                choose,
                refused,
                deleting,
                told,
            }
        }
        SettingGroup {
            SettingRow { label: "Name", hint: "",
                TextField {
                    value: "{profile.name}",
                    aria_label: "Profile name",
                    onchange: {
                        let id = id.clone();
                        move |name: String| {
                            let id = id.clone();
                            act(Rc::new(move |s| profiles::rename_profile(s, &id, &name)));
                        }
                    },
                }
            }
            SettingRow { label: "Agent", hint: "Changing it resets the model, options, login and command.",
                select {
                    aria_label: "Profile agent",
                    onchange: move |e| {
                        if let Some(a) = Agent::parse(&e.value()) {
                            switch_agent(a);
                        }
                    },
                    for a in Agent::ALL {
                        option { key: "{a.key()}", value: "{a.key()}", selected: agent == a, "{a.label()}" }
                    }
                }
            }
            if let Some(said) = reset() {
                div { class: "settings-row",
                    div { class: "binding-note warn", role: "status", "{said}" }
                }
            }
            LoginRow {
                profile: id.clone(),
                agent,
                home: profile.home.clone(),
                choose_login: set_login,
                probed,
            }
        }
        match profile.options.clone() {
            AgentOptions::Claude(options) => rsx! {
                SessionGroup {
                    key: "claude-{profile.id}",
                    title: "New sessions",
                    options,
                    set: {
                        let id = profile.id.clone();
                        move |options: SessionOptions| edit_profile(change, &id, move |p| p.options = AgentOptions::Claude(options))
                    },
                }
            },
            AgentOptions::Codex(options) => rsx! {
                CodexGroup {
                    key: "codex-{profile.id}",
                    title: "New sessions",
                    options,
                    set: {
                        let id = profile.id.clone();
                        move |options: CodexOptions| edit_profile(change, &id, move |p| p.options = AgentOptions::Codex(options))
                    },
                }
            },
            AgentOptions::Pi(options) => rsx! {
                PiGroup {
                    key: "pi-{profile.id}",
                    title: "New sessions",
                    options,
                    set: {
                        let id = profile.id.clone();
                        move |options: PiOptions| edit_profile(change, &id, move |p| p.options = AgentOptions::Pi(options))
                    },
                }
            },
            AgentOptions::OpenCode(options) => rsx! {
                OpenCodeGroup {
                    key: "opencode-{profile.id}",
                    title: "New sessions",
                    options,
                    set: {
                        let id = profile.id.clone();
                        move |options: OpenCodeOptions| edit_profile(change, &id, move |p| p.options = AgentOptions::OpenCode(options))
                    },
                }
            },
        }
        details { class: "settings-advanced",
            summary { "Advanced: how {agent.label()} is started" }
            LaunchGroup {
                key: "launch-{profile.id}",
                title: "Launch",
                hint: "The program and its arguments; empty for {program}.",
                placeholder: program.clone(),
                launch: profile.launch.clone(),
                set: {
                    let id = profile.id.clone();
                    move |launch: Launch| edit_profile(change, &id, move |p| p.launch = launch)
                },
            }
        }
    }
}

/// A profile to make: its name and agent; it opens once made.
#[component]
fn NewProfilePage(now: Settings, change: Callback<Change>, choose: Callback<AgentPage>) -> Element {
    let refused = use_signal(|| None::<String>);
    let mut name = use_signal(String::new);
    let mut agent = use_signal(|| now.default_agent());
    let create = move || {
        let (name, agent) = (name(), agent());
        let op: Attempt<ProfileId> = Rc::new(move |s| profiles::add_profile(s, &name, agent));
        if let Some(id) = attempt(change, refused, op) {
            choose(AgentPage::Profile(id));
        }
    };
    rsx! {
        PageHead { title: "New profile" }
        p { class: "settings-agent-note",
            "A profile is an agent, where it keeps its login and how its new sessions start. A draft picks one before its first prompt."
        }
        if let Some(why) = refused() {
            SettingsError { "{why}" }
        }
        SettingGroup {
            SettingRow { label: "Name", hint: "It can be renamed later.",
                input {
                    class: "settings-command",
                    r#type: "text",
                    aria_label: "New profile's name",
                    placeholder: "Work",
                    value: "{name}",
                    oninput: move |e| name.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter {
                            create();
                        }
                    },
                }
            }
            SettingRow { label: "Agent", hint: "",
                select {
                    aria_label: "New profile's agent",
                    onchange: move |e| {
                        if let Some(a) = Agent::parse(&e.value()) {
                            agent.set(a);
                        }
                    },
                    for a in Agent::ALL {
                        option { key: "{a.key()}", value: "{a.key()}", selected: agent() == a, "{a.label()}" }
                    }
                }
            }
            div { class: "settings-row",
                div { class: "settings-row-label" }
                div { class: "settings-row-control",
                    button { r#type: "button", class: "primary", onclick: move |_| create(),
                        IconView { icon: Icon::Plus, size: 13 }
                        " Create profile"
                    }
                }
            }
        }
    }
}

/// Asks before deleting the profile `id`, called `name`, whose login is
/// `folder`: whether to delete that folder too, when adroit made it, left
/// unchosen. The folder goes first, through the work host, which refuses it
/// while sessions run there; the profile goes once it has. What became of
/// it is `told` on `then`, the page shown next.
#[component]
fn DeleteProfile(
    id: ProfileId,
    name: String,
    folder: LoginFolder,
    then: AgentPage,
    change: Callback<Change>,
    choose: Callback<AgentPage>,
    refused: Signal<Option<String>>,
    deleting: Signal<bool>,
    told: Signal<Option<(AgentPage, String)>>,
) -> Element {
    let link = use_context::<Link>();
    let mut with_folder = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let shown = |p: &Path| tilde(p, &crate::paths::home());
    let confirm = {
        let (id, name, folder) = (id.clone(), name.clone(), folder.clone());
        move |_| {
            let mut refused = refused;
            let (id, name, folder, next) = (id.clone(), name.clone(), folder.clone(), then.clone());
            let gone = with_folder() && matches!(folder, LoginFolder::Deletable(_));
            let asked_for = id.clone();
            let finish = move |gone: bool| {
                let op: Attempt<AgentProfile> = {
                    let id = id.clone();
                    Rc::new(move |s| profiles::delete_profile(s, &id))
                };
                if attempt(change, refused, op).is_some() {
                    let mut told = told;
                    let mut deleting = deleting;
                    deleting.set(false);
                    told.set(Some((
                        next.clone(),
                        profiles::deleted_notice(&name, &folder, gone, shown),
                    )));
                    choose(next);
                }
            };
            if !gone {
                finish(false);
                return;
            }
            busy.set(true);
            let asked = link.ask(Ask::DeleteLogin { profile: asked_for });
            spawn(async move {
                match failure(asked.await) {
                    None => finish(true),
                    why => refused.set(why),
                }
                busy.set(false);
            });
        }
    };
    rsx! {
        div { class: "settings-confirm", role: "alertdialog", aria_label: "Delete {name}",
            p { class: "settings-confirm-head", "Delete {name}?" }
            p { class: "settings-agent-note",
                "Sessions that ran from it keep running, and are resumed, as they started."
            }
            match &folder {
                LoginFolder::Terminal => rsx! {
                    p { class: "settings-agent-note",
                        "It uses your terminal's login, which is left as it is."
                    }
                },
                LoginFolder::Deletable(home) => rsx! {
                    label { class: "settings-confirm-choice",
                        input {
                            r#type: "checkbox",
                            checked: with_folder(),
                            onchange: move |e| with_folder.set(e.checked()),
                        }
                        span { "Also delete its login folder " code { "{shown(home)}" } }
                    }
                    p { class: if with_folder() { "binding-note bad" } else { "binding-note warn" },
                        IconView { icon: Icon::AlertTriangle, size: 12 }
                        " Deleting the folder loses the login, history and settings kept in it, and the sessions that ran in it can no longer be resumed. Left unchosen, the folder stays."
                    }
                },
                LoginFolder::Kept(home) => rsx! {
                    p { class: "settings-agent-note",
                        "Its login folder "
                        code { "{shown(home)}" }
                        " was not made by Adroit, so it is left as it is."
                    }
                },
            }
            div { class: "settings-page-actions",
                button {
                    r#type: "button",
                    class: "danger",
                    disabled: busy(),
                    onclick: confirm,
                    if with_folder() && matches!(folder, LoginFolder::Deletable(_)) {
                        "Delete profile and folder"
                    } else {
                        "Delete profile"
                    }
                }
                button {
                    r#type: "button",
                    disabled: busy(),
                    onclick: move |_| {
                        let mut deleting = deleting;
                        deleting.set(false);
                    },
                    "Cancel"
                }
            }
        }
    }
}

/// What is known of a profile's login while the Settings window shows it.
#[derive(Debug, Clone, PartialEq)]
enum Probed {
    Asking,
    /// Its login terminal, by its id, is open; it is asked again once that
    /// ends.
    LoggingIn(u128),
    /// As the CLI the profile starts said.
    Is(LoginStanding),
    Failed(String),
}

/// Ask the work host whether the profile `key` names is logged in where it
/// keeps its login, once the terminal `after` has ended when one is named,
/// into `probed`.
fn probe(
    link: &Link,
    mut probed: Signal<Probes>,
    key: (ProfileId, Option<PathBuf>),
    after: Option<u128>,
) {
    let waited = match (after, probed.peek().get(&key)) {
        (Some(terminal), _) | (None, Some(&Probed::LoggingIn(terminal))) => Some(terminal),
        _ => None,
    };
    probed.write().insert(
        key.clone(),
        waited.map_or(Probed::Asking, Probed::LoggingIn),
    );
    let asked = link.ask(Ask::LoginStanding {
        profile: key.0.clone(),
        after,
    });
    spawn(async move {
        let (got, standing) = match asked.await {
            Answer::LoginStanding { standing } => (Probed::Is(standing), Some(standing)),
            answer => (Probed::Failed(refusal(answer)), None),
        };
        let mut probed = probed.write();
        // A login terminal still open is waited for still, unless this
        // answer came once it ended or says the login is done.
        if let Some(Probed::LoggingIn(terminal)) = probed.get(&key)
            && !crate::logins::ends_wait(*terminal, after, standing)
        {
            return;
        }
        probed.insert(key, got);
    });
}

/// Open a terminal that runs the CLI's login where the profile `key` names
/// keeps its login, and ask again whether it is logged in once it ends.
fn open_login(link: Link, probed: Signal<Probes>, key: (ProfileId, Option<PathBuf>)) {
    let mut probed = probed;
    let asked = link.ask(Ask::OpenLogin {
        profile: key.0.clone(),
        cols: 0,
        rows: 0,
    });
    spawn(async move {
        match asked.await {
            Answer::Terminal { terminal } => probe(&link, probed, key, Some(terminal.id)),
            answer => {
                probed.write().insert(key, Probed::Failed(refusal(answer)));
            }
        }
    });
}

/// The value of the login select for a login of the profile's own.
const OWN: &str = "own";

/// Where the profile `profile` keeps its login, `home` for a folder of its
/// own, which `choose_login` changes, and whether it is logged in there:
/// asked as the row is shown unless it was already, with what to do next,
/// open its login terminal or ask again; while that terminal is open, ask
/// again or cancel the login.
#[component]
fn LoginRow(
    profile: ProfileId,
    agent: Agent,
    home: Option<PathBuf>,
    choose_login: EventHandler<Login>,
    probed: Signal<Probes>,
) -> Element {
    let link = use_context::<Link>();
    {
        let link = link.clone();
        // Asked again for each login the row comes to show, as the
        // profile's login changes under it.
        use_effect(use_reactive((&profile, &home), move |(profile, home)| {
            let key = (profile, home);
            if !probed.peek().contains_key(&key) {
                probe(&link, probed, key, None);
            }
        }));
    }
    let key = (profile, home.clone());
    let now = probed.read().get(&key).cloned();
    let (chip, class) = match &now {
        None | Some(Probed::Asking) => ("Checking…".to_string(), "login-standing"),
        Some(Probed::LoggingIn(_)) => ("Waiting for login…".to_string(), "login-standing"),
        Some(Probed::Is(s)) => (
            s.label().to_string(),
            match s {
                LoginStanding::LoggedIn => "login-standing in",
                LoginStanding::LoggedOut => "login-standing out",
                LoginStanding::Unknown => "login-standing",
                LoginStanding::Missing => "login-standing missing",
            },
        ),
        Some(Probed::Failed(_)) => ("Unknown".to_string(), "login-standing"),
    };
    let said = match &now {
        Some(Probed::Is(LoginStanding::Missing)) => format!(
            "{} isn't installed. Install it, or set the profile's launch command.",
            agent.label()
        ),
        Some(Probed::Is(LoginStanding::Unknown)) => {
            format!("{} didn't say whether it is logged in.", agent.label())
        }
        Some(Probed::Failed(why)) => why.clone(),
        Some(Probed::Is(LoginStanding::LoggedOut)) => crate::logins::without_login(agent)
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    };
    let whose = match &home {
        None => format!(
            "{} as it runs in your terminal: logging in or out there changes it here too.",
            agent.label()
        ),
        Some(folder) => format!(
            "Kept apart from your terminal's in {}. {} logs in itself; Adroit keeps no passwords.",
            tilde(folder, &crate::paths::home()),
            agent.label()
        ),
    };
    let hint = format!("{whose} {said}").trim().to_string();
    let logged_out = matches!(now, Some(Probed::Is(LoginStanding::LoggedOut)));
    let busy = matches!(now, None | Some(Probed::Asking));
    let waiting = match now {
        Some(Probed::LoggingIn(terminal)) => Some(terminal),
        _ => None,
    };
    let (a, b, c) = (link.clone(), link.clone(), link);
    let (again, login) = (key.clone(), key);
    rsx! {
        SettingRow { label: "Login", hint,
            select {
                aria_label: "Profile login",
                onchange: move |e: Event<FormData>| {
                    choose_login.call(match e.value().as_str() {
                        OWN => Login::Own,
                        _ => Login::Terminal,
                    });
                },
                option { value: "terminal", selected: home.is_none(), "{profiles::TERMINAL}" }
                option { value: OWN, selected: home.is_some(), "{profiles::OWN}" }
            }
            span { class, role: "status", "{chip}" }
            button {
                r#type: "button",
                title: "Check again",
                aria_label: "Check again",
                disabled: busy,
                onclick: move |_| probe(&a, probed, again.clone(), None),
                IconView { icon: Icon::Refresh, size: 13 }
            }
            if let Some(terminal) = waiting {
                button {
                    r#type: "button",
                    onclick: move |_| c.send(Request::EndTerminal { id: terminal }),
                    "Cancel"
                }
            } else {
                button {
                    r#type: "button",
                    class: if logged_out { "primary" } else { "" },
                    disabled: busy,
                    onclick: move |_| open_login(b.clone(), probed, login.clone()),
                    IconView { icon: Icon::Terminal, size: 13 }
                    " Open login terminal"
                }
            }
        }
    }
}
