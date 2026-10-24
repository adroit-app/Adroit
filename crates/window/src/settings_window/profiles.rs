//! The Agents pages of the Settings window: a page for each profile sessions
//! start from, and one for each account they run in with the login it keeps.
//! settings.toml and the code call an account a connection.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use adroit_core::agent_pages::AgentPage;
use adroit_core::profiles::{self, NewHome};
use adroit_model::Agent;
use adroit_model::config::{CodexOptions, Launch, SessionOptions};
use adroit_model::paths::tilde;
use adroit_model::profile::{ConnectionId, LoginStanding, ProfileId};
use adroit_protocol::{AgentsWork, Answer, Ask};
use dioxus::prelude::*;

use super::{Change, CodexGroup, LaunchGroup, SessionGroup};
use crate::components::{SettingGroup, SettingRow, SettingsError, TextField};
use crate::icons::{Icon, IconView};
use crate::link::Link;
use crate::paths::home;
use crate::settings::{AgentOptions, Settings};

/// A change to the profiles or accounts that may be refused, with what came
/// of it when it is not.
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

/// The folder a new account keeps its state in when none is chosen, as it
/// is shown: `<id>` standing for the id it is given.
fn own_home() -> String {
    let data = adroit_model::paths::data_dir();
    tilde(
        &data.join("adroit").join("connections").join("<id>"),
        &home(),
    )
}

/// An account's own folder, under the data folder by its id, written as
/// the folders that exist are.
fn own_account(s: &mut Settings, id: &ProfileId, name: &str) -> Result<ConnectionId, String> {
    let data = adroit_model::paths::data_dir();
    profiles::add_connection_for(s, id, name, NewHome::Own { data }, |p| {
        std::fs::canonicalize(p).ok()
    })
}

/// The page under Agents that is `page`, `choose` going to another.
#[component]
pub(super) fn AgentsPane(
    page: AgentPage,
    now: Settings,
    agents: AgentsWork,
    change: Callback<Change>,
    choose: Callback<AgentPage>,
) -> Element {
    // Kept while the pages change, so a login is not asked again on each.
    let probed = use_signal(HashMap::<ConnectionId, Probed>::new);
    match page {
        AgentPage::Profile(id) => rsx! {
            ProfilePage { key: "profile-{id}", id, now, agents, change, choose, probed }
        },
        AgentPage::NewProfile => rsx! {
            NewProfilePage { now, change, choose }
        },
        AgentPage::Accounts => rsx! {
            AccountsPage { now, change, choose }
        },
        AgentPage::Account(id) => rsx! {
            AccountPage { key: "account-{id}", id, now, change, choose, probed }
        },
        AgentPage::Archived => rsx! {
            ArchivedPage { now, change }
        },
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

/// The value of the account select that makes a new account.
const NEW_ACCOUNT: &str = "+new";

/// Everything about the profile `id`: its name, agent and account with the
/// account's login, its new sessions' options and its launch, and what can
/// be done to it.
#[component]
fn ProfilePage(
    id: ProfileId,
    now: Settings,
    agents: AgentsWork,
    change: Callback<Change>,
    choose: Callback<AgentPage>,
    probed: Signal<HashMap<ConnectionId, Probed>>,
) -> Element {
    let refused = use_signal(|| None::<String>);
    let mut told = use_signal(|| None::<String>);
    let mut making = use_signal(|| false);
    let mut account_name = use_signal(String::new);
    let Some(profile) = now.profiles.iter().find(|p| p.id == id).cloned() else {
        return rsx! {};
    };
    let agent = profile.agent();
    let default = now.profile == profile.id;
    let program = agents
        .of(agent)
        .map(|f| f.program.clone())
        .unwrap_or_default();
    let choices = profiles::connection_choices(&now, agent, &profile.connection);
    let act = move |op: Attempt<()>| attempt(change, refused, op).is_some();
    let mut switch_agent = {
        let id = id.clone();
        move |to: Agent| {
            let id = id.clone();
            let op: Attempt<Option<String>> = Rc::new(move |s| profiles::change_agent(s, &id, to));
            if let Some(said) = attempt(change, refused, op) {
                told.set(said);
                making.set(false);
            }
        }
    };
    let duplicate = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            let op: Attempt<ProfileId> = Rc::new(move |s| profiles::duplicate_profile(s, &id));
            if let Some(copy) = attempt(change, refused, op) {
                choose(AgentPage::Profile(copy));
            }
        }
    };
    let make_account = {
        let id = id.clone();
        move |_| {
            let (id, name) = (id.clone(), account_name());
            let op: Attempt<ConnectionId> = Rc::new(move |s| own_account(s, &id, &name));
            if attempt(change, refused, op).is_some() {
                making.set(false);
                account_name.set(String::new());
            }
        }
    };
    let account_hint = match profile.connection.implicit_agent() {
        Some(_) => format!(
            "Logging in or out of {} in a terminal changes it too.",
            agent.label()
        ),
        None => format!("A login, history and settings of {}'s own.", agent.label()),
    };
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
            button {
                r#type: "button",
                onclick: {
                    let id = id.clone();
                    move |_| {
                        let id = id.clone();
                        act(Rc::new(move |s| profiles::archive_profile(s, &id)));
                    }
                },
                "Archive"
            }
        }
        if let Some(why) = refused() {
            SettingsError { "{why}" }
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
            SettingRow { label: "Agent", hint: "Changing it resets the model, options, account and command.",
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
            if let Some(said) = told() {
                div { class: "settings-row",
                    div { class: "binding-note warn", role: "status", "{said}" }
                }
            }
            SettingRow { label: "Account", hint: account_hint,
                select {
                    aria_label: "Profile account",
                    onchange: {
                        let id = id.clone();
                        move |e: Event<FormData>| {
                            if e.value() == NEW_ACCOUNT {
                                making.set(true);
                                return;
                            }
                            making.set(false);
                            if let Ok(to) = e.value().parse::<ConnectionId>() {
                                let id = id.clone();
                                act(Rc::new(move |s| profiles::set_connection(s, &id, &to)));
                            }
                        }
                    },
                    for c in choices {
                        option {
                            key: "{c.id}",
                            value: "{c.id}",
                            selected: !making() && c.id == profile.connection,
                            if c.archived { "{c.name} (archived)" } else { "{c.name}" }
                        }
                    }
                    option { value: NEW_ACCOUNT, selected: making(), "New account…" }
                }
            }
            if making() {
                SettingRow {
                    label: "New account",
                    hint: "A login of its own for {agent.label()}, kept in a folder of its own.",
                    input {
                        r#type: "text",
                        aria_label: "New account's name",
                        placeholder: "Name",
                        onmounted: move |e| async move {
                            let _ = e.set_focus(true).await;
                        },
                        value: "{account_name}",
                        oninput: move |e| account_name.set(e.value()),
                        onkeydown: {
                            let mut make_account = make_account.clone();
                            move |e: KeyboardEvent| {
                                if e.key() == Key::Enter {
                                    make_account(());
                                }
                            }
                        },
                    }
                    button {
                        r#type: "button",
                        class: "primary",
                        onclick: {
                            let mut make_account = make_account.clone();
                            move |_| make_account(())
                        },
                        "Create"
                    }
                    button {
                        r#type: "button",
                        onclick: move |_| {
                            making.set(false);
                            account_name.set(String::new());
                        },
                        "Cancel"
                    }
                }
            }
            LoginRow {
                id: profile.connection.clone(),
                agent,
                now: now.clone(),
                asking: Some(id.clone()),
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
                        move |options: SessionOptions| {
                            let id = id.clone();
                            change(Box::new(move |s: &mut Settings| {
                                if let Ok(p) = profiles::profile_mut(s, &id) {
                                    p.options = AgentOptions::Claude(options);
                                }
                            }))
                        }
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
                        move |options: CodexOptions| {
                            let id = id.clone();
                            change(Box::new(move |s: &mut Settings| {
                                if let Ok(p) = profiles::profile_mut(s, &id) {
                                    p.options = AgentOptions::Codex(options);
                                }
                            }))
                        }
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
                    move |launch: Launch| {
                        let id = id.clone();
                        change(Box::new(move |s: &mut Settings| {
                            if let Ok(p) = profiles::profile_mut(s, &id) {
                                p.launch = launch;
                            }
                        }))
                    }
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
            "A profile is an agent, the account it uses and how its new sessions start. A draft picks one before its first prompt."
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

/// What accounts are, and one to make, in a folder of its own or one
/// chosen for it; it opens once made.
#[component]
fn AccountsPage(now: Settings, change: Callback<Change>, choose: Callback<AgentPage>) -> Element {
    let refused = use_signal(|| None::<String>);
    let mut name = use_signal(String::new);
    let mut agent = use_signal(|| now.default_agent());
    let mut folder = use_signal(String::new);
    let own_home = own_home();
    let create = move |_| {
        let (name, agent, chosen) = (name(), agent(), folder());
        let op: Attempt<ConnectionId> = Rc::new(move |s| {
            let home = match chosen.trim() {
                "" => NewHome::Own {
                    data: adroit_model::paths::data_dir(),
                },
                path => NewHome::Chosen {
                    path: path.to_string(),
                    home: home(),
                },
            };
            profiles::add_connection(s, &name, agent, home, |p| std::fs::canonicalize(p).ok())
        });
        if let Some(id) = attempt(change, refused, op) {
            choose(AgentPage::Account(id));
        }
    };
    rsx! {
        PageHead { title: "Accounts" }
        p { class: "settings-agent-note",
            "An account is a separate login, history and settings for an agent. Each profile uses one, and profiles can share one. Every agent also has the account shared with your terminal. The agent logs in itself; Adroit keeps no passwords."
        }
        if let Some(why) = refused() {
            SettingsError { "{why}" }
        }
        SettingGroup { title: "New account",
            SettingRow { label: "Name", hint: "",
                input {
                    class: "settings-command",
                    r#type: "text",
                    aria_label: "New account's name",
                    placeholder: "Work",
                    value: "{name}",
                    oninput: move |e| name.set(e.value()),
                }
            }
            SettingRow { label: "Agent", hint: "",
                select {
                    aria_label: "New account's agent",
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
            SettingRow { label: "Folder", hint: "Empty for a folder of its own. It cannot be changed later.",
                input {
                    class: "settings-command",
                    r#type: "text",
                    aria_label: "New account's folder",
                    placeholder: "{own_home}",
                    value: "{folder}",
                    oninput: move |e| folder.set(e.value()),
                }
            }
            div { class: "settings-row",
                div { class: "settings-row-label" }
                div { class: "settings-row-control",
                    button { r#type: "button", class: "primary", onclick: create,
                        IconView { icon: Icon::Plus, size: 13 }
                        " Create account"
                    }
                }
            }
        }
    }
}

/// The account `id`: its name and folder when it is one of its own, the
/// profiles that use it, its login, and archiving it.
#[component]
fn AccountPage(
    id: ConnectionId,
    now: Settings,
    change: Callback<Change>,
    choose: Callback<AgentPage>,
    probed: Signal<HashMap<ConnectionId, Probed>>,
) -> Element {
    let refused = use_signal(|| None::<String>);
    let own = now.connections.iter().find(|c| c.id == id).cloned();
    let Some(agent) = id
        .implicit_agent()
        .or_else(|| own.as_ref().map(|c| c.agent))
    else {
        return rsx! {};
    };
    let act = move |op: Attempt<()>| attempt(change, refused, op).is_some();
    let users: Vec<(ProfileId, String)> = profiles::users(&now, &id)
        .iter()
        .map(|p| (p.id.clone(), p.label().to_string()))
        .collect();
    let note = match &own {
        None => format!(
            "{} as it runs in your own terminal: logging in or out there changes it here too.",
            agent.label()
        ),
        Some(_) => format!(
            "A login, history and settings of {}'s own, apart from your terminal's. {} logs in itself; Adroit keeps no passwords.",
            agent.label(),
            agent.label()
        ),
    };
    rsx! {
        PageHead {
            title: profiles::connection_name(&now, &id),
            mark: Some(agent.label().to_string()),
        }
        p { class: "settings-agent-note", "{note}" }
        if let Some(why) = refused() {
            SettingsError { "{why}" }
        }
        SettingGroup {
            if let Some(c) = own.clone() {
                SettingRow { label: "Name", hint: "",
                    TextField {
                        value: "{c.name}",
                        aria_label: "Account name",
                        onchange: {
                            let id = id.clone();
                            move |name: String| {
                                let id = id.clone();
                                act(Rc::new(move |s| profiles::rename_connection(s, &id, &name)));
                            }
                        },
                    }
                }
                SettingRow { label: "Folder", hint: tilde(&c.home, &home()),
                    button {
                        r#type: "button",
                        onclick: {
                            let folder: PathBuf = c.home.clone();
                            move |_| super::open_with("-R", &folder, "")
                        },
                        IconView { icon: Icon::Folder, size: 13 }
                        " Show in Finder"
                    }
                }
            }
            div { class: "settings-row",
                div { class: "settings-row-label",
                    div { "Used by" }
                    if users.is_empty() {
                        div { class: "settings-hint", "No profile uses it." }
                    } else {
                        div { class: "settings-used-by",
                            for (pid, label) in users {
                                button {
                                    key: "{pid}",
                                    r#type: "button",
                                    class: "settings-link",
                                    onclick: move |_| choose(AgentPage::Profile(pid.clone())),
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
            LoginRow {
                id: id.clone(),
                agent,
                now: now.clone(),
                probed,
            }
            if own.is_some() {
                SettingRow { label: "Archive", hint: "Logs nothing out and keeps its folder.",
                    button {
                        r#type: "button",
                        onclick: {
                            let id = id.clone();
                            move |_| {
                                let id = id.clone();
                                act(Rc::new(move |s| profiles::archive_connection(s, &id)));
                            }
                        },
                        "Archive"
                    }
                }
            }
        }
    }
}

/// The profiles and accounts archived, each to bring back.
#[component]
fn ArchivedPage(now: Settings, change: Callback<Change>) -> Element {
    let refused = use_signal(|| None::<String>);
    let act = move |op: Attempt<()>| attempt(change, refused, op).is_some();
    let rows: Vec<_> = profiles::profile_rows(&now)
        .into_iter()
        .filter(|r| r.archived)
        .collect();
    let accounts: Vec<_> = now
        .connections
        .iter()
        .filter(|c| c.archived)
        .cloned()
        .collect();
    rsx! {
        PageHead { title: "Archived" }
        p { class: "settings-agent-note",
            "Kept for the sessions that used them; nothing new starts from them."
        }
        if let Some(why) = refused() {
            SettingsError { "{why}" }
        }
        if !rows.is_empty() {
            SettingGroup { title: "Profiles",
                for row in rows {
                    SettingRow {
                        key: "{row.id}",
                        label: row.name.clone(),
                        hint: "{row.agent.label()} · {row.connection}",
                        button {
                            r#type: "button",
                            onclick: {
                                let id = row.id.clone();
                                move |_| {
                                    let id = id.clone();
                                    act(Rc::new(move |s| profiles::unarchive_profile(s, &id)));
                                }
                            },
                            "Unarchive"
                        }
                    }
                }
            }
        }
        if !accounts.is_empty() {
            SettingGroup { title: "Accounts",
                for c in accounts {
                    SettingRow {
                        key: "{c.id}",
                        label: profiles::connection_name(&now, &c.id),
                        hint: "{c.agent.label()} · {tilde(&c.home, &home())}",
                        button {
                            r#type: "button",
                            onclick: {
                                let id = c.id.clone();
                                move |_| {
                                    let id = id.clone();
                                    act(Rc::new(move |s| profiles::unarchive_connection(s, &id)));
                                }
                            },
                            "Unarchive"
                        }
                    }
                }
            }
        }
    }
}

/// What is known of an account's login while the Settings window shows it.
#[derive(Debug, Clone, PartialEq)]
enum Probed {
    Asking,
    /// Its login terminal is open; it is asked again once that ends.
    LoggingIn,
    /// As the CLI the profile it names starts there said.
    Is(LoginStanding, ProfileId),
    Failed(String),
}

/// Ask the work host whether the account `id` is logged in, once the
/// terminal `after` has ended when one is named, into `probed`.
fn probe(
    link: &Link,
    mut probed: Signal<HashMap<ConnectionId, Probed>>,
    id: ConnectionId,
    after: Option<u128>,
) {
    probed.write().insert(
        id.clone(),
        if after.is_some() {
            Probed::LoggingIn
        } else {
            Probed::Asking
        },
    );
    let asked = link.ask(Ask::LoginStanding {
        connection: id.clone(),
        profile: None,
        after,
    });
    spawn(async move {
        let got = match asked.await {
            Answer::LoginStanding { standing, profile } => Probed::Is(standing, profile),
            Answer::Failed { text } => Probed::Failed(text),
            answer => Probed::Failed(format!("the work host answered {answer:?}")),
        };
        probed.write().insert(id, got);
    });
}

/// Open a terminal that runs the CLI's login in the account `id`, and ask
/// again whether it is logged in once it ends.
fn open_login(link: Link, probed: Signal<HashMap<ConnectionId, Probed>>, id: ConnectionId) {
    let mut probed = probed;
    let asked = link.ask(Ask::OpenLogin {
        connection: id.clone(),
        profile: None,
        cols: 0,
        rows: 0,
    });
    spawn(async move {
        match asked.await {
            Answer::Terminal { terminal } => probe(&link, probed, id, Some(terminal.id)),
            Answer::Failed { text } => {
                probed.write().insert(id, Probed::Failed(text));
            }
            answer => {
                probed.write().insert(
                    id,
                    Probed::Failed(format!("the work host answered {answer:?}")),
                );
            }
        }
    });
}

/// Whether the account `id` is logged in, asked as the row is shown unless
/// it was already, with what to do next: open its login terminal, or ask
/// again. `now` names the profile whose CLI answered, unless it is
/// `asking`, the profile whose page the row is on.
#[component]
fn LoginRow(
    id: ConnectionId,
    agent: Agent,
    now: Settings,
    #[props(default)] asking: Option<ProfileId>,
    probed: Signal<HashMap<ConnectionId, Probed>>,
) -> Element {
    let link = use_context::<Link>();
    {
        let link = link.clone();
        // Asked again for each account the row comes to show, as a profile's
        // account changes under it.
        use_effect(use_reactive((&id,), move |(id,)| {
            if !probed.peek().contains_key(&id) {
                probe(&link, probed, id, None);
            }
        }));
    }
    let settings = now;
    let now = probed.read().get(&id).cloned();
    let (chip, class) = match &now {
        None | Some(Probed::Asking) => ("Checking…".to_string(), "login-standing"),
        Some(Probed::LoggingIn) => ("Waiting for login…".to_string(), "login-standing"),
        Some(Probed::Is(s, _)) => (
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
        Some(Probed::Is(LoginStanding::Missing, _)) => format!(
            "{} isn't installed. Install it, or set the profile's launch command.",
            agent.label()
        ),
        Some(Probed::Is(LoginStanding::Unknown, _)) => {
            format!("{} didn't say whether it is logged in.", agent.label())
        }
        Some(Probed::Failed(why)) => why.clone(),
        _ => String::new(),
    };
    let hint = match &now {
        Some(Probed::Is(_, profile)) if asking.as_ref() != Some(profile) => {
            let name = settings
                .find_profile(profile)
                .map_or_else(|| profile.to_string(), |p| p.label().to_string());
            format!("{said} Checked with {name}.").trim().to_string()
        }
        _ => said,
    };
    let logged_out = matches!(now, Some(Probed::Is(LoginStanding::LoggedOut, _)));
    let busy = matches!(now, None | Some(Probed::Asking) | Some(Probed::LoggingIn));
    let (a, b) = (link.clone(), link);
    let (again, login) = (id.clone(), id);
    rsx! {
        SettingRow { label: "Login", hint,
            span { class, role: "status", "{chip}" }
            button {
                r#type: "button",
                title: "Check again",
                aria_label: "Check again",
                disabled: busy,
                onclick: move |_| probe(&a, probed, again.clone(), None),
                IconView { icon: Icon::Refresh, size: 13 }
            }
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
