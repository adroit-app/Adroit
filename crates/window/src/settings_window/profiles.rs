//! The Agents pages of the Settings window: the profiles sessions start
//! from, and the connections they run in with the login each keeps.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use adroit_core::profiles::{self, NewHome};
use adroit_model::Agent;
use adroit_model::config::{CodexOptions, Launch, SessionOptions};
use adroit_model::paths::tilde;
use adroit_model::profile::{AgentConnection, ConnectionId, LoginStanding, ProfileId};
use adroit_protocol::{AgentsWork, Answer, Ask};
use dioxus::prelude::*;

use super::{AgentSection, Change, CodexGroup, LaunchGroup, SessionGroup};
use crate::components::{SettingGroup, SettingRow, SettingsError, TextField};
use crate::icons::{Icon, IconView};
use crate::link::Link;
use crate::paths::home;
use crate::settings::{AgentOptions, Settings};

/// A change to the profiles or connections that may be refused, with what
/// came of it when it is not.
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

#[component]
pub(super) fn ProfilesPane(now: Settings, agents: AgentsWork, change: Callback<Change>) -> Element {
    let mut chosen = use_signal(|| None::<ProfileId>);
    let refused = use_signal(|| None::<String>);
    let mut told = use_signal(|| None::<String>);
    let mut new_name = use_signal(String::new);
    let mut new_agent = use_signal(|| now.default_agent());
    let rows = profiles::profile_rows(&now);
    let editing = chosen()
        .filter(|id| now.profiles.iter().any(|p| &p.id == id))
        .or_else(|| rows.iter().find(|r| r.default).map(|r| r.id.clone()))
        .or_else(|| rows.first().map(|r| r.id.clone()));
    let (open, archived): (Vec<_>, Vec<_>) = rows.into_iter().partition(|r| !r.archived);
    let act = move |op: Attempt<()>| {
        attempt(change, refused, op);
    };
    let add = move |_| {
        let (name, agent) = (new_name(), new_agent());
        let op: Attempt<ProfileId> = Rc::new(move |s| profiles::add_profile(s, &name, agent));
        if let Some(id) = attempt(change, refused, op) {
            chosen.set(Some(id));
            new_name.set(String::new());
        }
    };
    let duplicate = move |id: ProfileId| {
        let op: Attempt<ProfileId> = Rc::new(move |s| profiles::duplicate_profile(s, &id));
        if let Some(copy) = attempt(change, refused, op) {
            chosen.set(Some(copy));
        }
    };
    rsx! {
        if let Some(why) = refused() {
            SettingsError { "{why}" }
        }
        p { class: "settings-agent-note",
            "Each session starts from a profile: an agent, the connection it runs in, and the defaults for its sessions. Changes reach new sessions only."
        }
        SettingGroup { title: "Profiles",
            for row in open {
                div {
                    key: "{row.id}",
                    class: if editing.as_ref() == Some(&row.id) { "settings-row profile-row on" } else { "settings-row profile-row" },
                    div { class: "settings-row-label",
                        button {
                            r#type: "button",
                            class: "settings-link profile-name",
                            aria_label: "Edit {row.name}",
                            onclick: {
                                let id = row.id.clone();
                                move |_| {
                                    told.set(None);
                                    chosen.set(Some(id.clone()));
                                }
                            },
                            "{row.name}"
                        }
                        if row.default {
                            span { class: "binding-mark", "Default" }
                        }
                        div { class: "settings-hint", "{row.agent.label()} · {row.connection}" }
                    }
                    div { class: "settings-row-control",
                        if !row.default {
                            button {
                                r#type: "button",
                                onclick: {
                                    let (id, act) = (row.id.clone(), act);
                                    move |_| {
                                        let id = id.clone();
                                        act(Rc::new(move |s| profiles::make_default(s, &id)));
                                    }
                                },
                                "Make default"
                            }
                        }
                        button {
                            r#type: "button",
                            title: "Duplicate: a copy in the same connection",
                            onclick: {
                                let (id, mut duplicate) = (row.id.clone(), duplicate);
                                move |_| duplicate(id.clone())
                            },
                            IconView { icon: Icon::Copy, size: 13 }
                            " Duplicate"
                        }
                        button {
                            r#type: "button",
                            onclick: {
                                let (id, act) = (row.id.clone(), act);
                                move |_| {
                                    let id = id.clone();
                                    act(Rc::new(move |s| profiles::archive_profile(s, &id)));
                                }
                            },
                            "Archive"
                        }
                    }
                }
            }
            div { class: "settings-row",
                div { class: "settings-row-label",
                    div { "New profile" }
                    div { class: "settings-hint", "Its id is made from the name and stays when it is renamed." }
                }
                div { class: "settings-row-control",
                    input {
                        r#type: "text",
                        aria_label: "New profile's name",
                        placeholder: "Name",
                        value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()),
                    }
                    select {
                        aria_label: "New profile's agent",
                        onchange: move |e| {
                            if let Some(a) = Agent::parse(&e.value()) {
                                new_agent.set(a);
                            }
                        },
                        for a in Agent::ALL {
                            option { key: "{a.key()}", value: "{a.key()}", selected: new_agent() == a, "{a.label()}" }
                        }
                    }
                    button { r#type: "button", class: "primary", onclick: add,
                        IconView { icon: Icon::Plus, size: 13 }
                        " Add"
                    }
                }
            }
        }
        if let Some(id) = editing.clone() {
            ProfileEditor {
                key: "{id}",
                id,
                now: now.clone(),
                agents,
                change,
                refused,
                told,
            }
        }
        if !archived.is_empty() {
            SettingGroup { title: "Archived profiles",
                for row in archived {
                    SettingRow {
                        key: "{row.id}",
                        label: row.name.clone(),
                        hint: "{row.agent.label()} · {row.connection} · kept for the sessions that ran from it",
                        button {
                            r#type: "button",
                            onclick: {
                                let (id, act) = (row.id.clone(), act);
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
    }
}

/// The profile `id`: its name, agent and connection, then its sessions'
/// defaults or its launch.
#[component]
fn ProfileEditor(
    id: ProfileId,
    now: Settings,
    agents: AgentsWork,
    change: Callback<Change>,
    refused: Signal<Option<String>>,
    told: Signal<Option<String>>,
) -> Element {
    let mut section = use_signal(|| AgentSection::Sessions);
    let Some(profile) = now.profiles.iter().find(|p| p.id == id).cloned() else {
        return rsx! {};
    };
    let agent = profile.agent();
    let program = agents
        .of(agent)
        .map(|f| f.program.clone())
        .unwrap_or_default();
    let choices = profiles::connection_choices(&now, agent, &profile.connection);
    let act = move |op: Attempt<()>| {
        attempt(change, refused, op);
    };
    let mut told_of = told;
    let mut switch_agent = move |to: Agent| {
        let id = id.clone();
        let op: Attempt<Option<String>> = Rc::new(move |s| profiles::change_agent(s, &id, to));
        if let Some(said) = attempt(change, refused, op) {
            told_of.set(said);
        }
    };
    rsx! {
        SettingGroup { title: "{profile.label()}",
            SettingRow { label: "Name", hint: "Renaming keeps its id, `{profile.id}`, and the sessions that ran from it.",
                TextField {
                    value: "{profile.name}",
                    aria_label: "Profile name",
                    onchange: {
                        let (id, act) = (profile.id.clone(), act);
                        move |name: String| {
                            let id = id.clone();
                            act(Rc::new(move |s| profiles::rename_profile(s, &id, &name)));
                        }
                    },
                }
            }
            SettingRow { label: "Agent", hint: "Changing it resets the options that belong to the agent it started.",
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
            SettingRow { label: "Connection", hint: "Where its agent's CLI keeps the login, history and settings; profiles in one connection share them.",
                select {
                    aria_label: "Profile connection",
                    onchange: {
                        let (id, act) = (profile.id.clone(), act);
                        move |e: Event<FormData>| {
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
                            selected: c.id == profile.connection,
                            if c.archived { "{c.name} (archived)" } else { "{c.name}" }
                        }
                    }
                }
            }
        }
        div { class: "settings-agent-tabs", role: "group", aria_label: "Profile settings",
            for item in AgentSection::ALL {
                button {
                    r#type: "button",
                    key: "{item.title()}",
                    class: if section() == item { "on" } else { "" },
                    aria_pressed: section() == item,
                    onclick: move |_| section.set(item),
                    "{item.title()}"
                }
            }
        }
        p { class: "settings-agent-note",
            match section() {
                AgentSection::Launch => "How this profile starts its agent.",
                AgentSection::Sessions => "Defaults for new sessions. Running sessions keep their settings.",
            }
        }
        if section() == AgentSection::Launch {
            LaunchGroup {
                key: "launch-{profile.id}",
                title: "Launch",
                hint: "The program and its arguments; empty for the agent's own command.",
                placeholder: program,
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
        } else {
            match profile.options.clone() {
                AgentOptions::Claude(options) => rsx! {
                    SessionGroup {
                        key: "claude-{profile.id}",
                        title: section().title(),
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
                        title: section().title(),
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
        }
    }
}

/// What is known of a connection's login while the Settings window shows it.
#[derive(Debug, Clone, PartialEq)]
enum Probed {
    Asking,
    /// Its login terminal is open; it is asked again once that ends.
    LoggingIn,
    /// As the CLI the profile it names starts there said.
    Is(LoginStanding, ProfileId),
    Failed(String),
}

/// Ask the work host whether the connection `id` is logged in, once the
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

/// Open a terminal that runs the CLI's login in the connection `id`, and ask
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

#[component]
pub(super) fn ConnectionsPane(now: Settings, change: Callback<Change>) -> Element {
    let refused = use_signal(|| None::<String>);
    let probed = use_signal(HashMap::<ConnectionId, Probed>::new);
    let mut new_name = use_signal(String::new);
    let mut new_agent = use_signal(|| now.default_agent());
    let mut new_home = use_signal(String::new);
    let (open, archived): (Vec<AgentConnection>, Vec<AgentConnection>) =
        now.connections.iter().cloned().partition(|c| !c.archived);
    let act = move |op: Attempt<()>| {
        attempt(change, refused, op);
    };
    let data = adroit_model::paths::data_dir();
    let own_home = tilde(
        &data.join("adroit").join("connections").join("<id>"),
        &home(),
    );
    let create = {
        move |_| {
            let (name, agent, chosen) = (new_name(), new_agent(), new_home());
            let data = data.clone();
            let op: Attempt<ConnectionId> = Rc::new(move |s| {
                let home = match chosen.trim() {
                    "" => NewHome::Own { data: data.clone() },
                    path => NewHome::Chosen {
                        path: path.to_string(),
                        home: home(),
                    },
                };
                profiles::add_connection(s, &name, agent, home, |p| std::fs::canonicalize(p).ok())
            });
            if attempt(change, refused, op).is_some() {
                new_name.set(String::new());
                new_home.set(String::new());
            }
        }
    };
    rsx! {
        if let Some(why) = refused() {
            SettingsError { "{why}" }
        }
        p { class: "settings-agent-note",
            "A connection is a folder an agent's CLI keeps its login, history and settings in, apart from every other. The CLI logs in itself; Adroit keeps no credentials, and archiving a connection logs nothing out and leaves its folder as it is."
        }
        SettingGroup { title: "Agents' own environments",
            for agent in Agent::ALL {
                LoginRow {
                    key: "{agent.key()}",
                    id: ConnectionId::implicit(agent),
                    label: profiles::connection_name(&now, &ConnectionId::implicit(agent)),
                    hint: "The CLI as it runs outside Adroit. Logging in or out of it in a terminal of your own changes it here too.".to_string(),
                    agent,
                    now: now.clone(),
                    probed,
                }
            }
        }
        for c in open {
            ConnectionGroup {
                key: "{c.id}",
                connection: c.clone(),
                now: now.clone(),
                used_by: profiles::users(&now, &c.id).iter().map(|p| p.label().to_string()).collect::<Vec<_>>(),
                probed,
                rename: {
                    let (id, act) = (c.id.clone(), act);
                    move |name: String| {
                        let id = id.clone();
                        act(Rc::new(move |s| profiles::rename_connection(s, &id, &name)));
                    }
                },
                archive: {
                    let (id, act) = (c.id.clone(), act);
                    move |_| {
                        let id = id.clone();
                        act(Rc::new(move |s| profiles::archive_connection(s, &id)));
                    }
                },
            }
        }
        SettingGroup { title: "New connection",
            SettingRow { label: "Name", hint: "Its id is made from the name; it can be renamed later.",
                input {
                    class: "settings-command",
                    r#type: "text",
                    aria_label: "New connection's name",
                    placeholder: "Work",
                    value: "{new_name}",
                    oninput: move |e| new_name.set(e.value()),
                }
            }
            SettingRow { label: "Agent", hint: "The agent whose CLI keeps its state there.",
                select {
                    aria_label: "New connection's agent",
                    onchange: move |e| {
                        if let Some(a) = Agent::parse(&e.value()) {
                            new_agent.set(a);
                        }
                    },
                    for a in Agent::ALL {
                        option { key: "{a.key()}", value: "{a.key()}", selected: new_agent() == a, "{a.label()}" }
                    }
                }
            }
            SettingRow { label: "Folder", hint: "Empty for a folder of its own, {own_home}. It is written once, as an absolute path, and never changes.",
                input {
                    class: "settings-command",
                    r#type: "text",
                    aria_label: "New connection's folder",
                    placeholder: "{own_home}",
                    value: "{new_home}",
                    oninput: move |e| new_home.set(e.value()),
                }
            }
            div { class: "settings-row",
                div { class: "settings-row-label" }
                div { class: "settings-row-control",
                    button { r#type: "button", class: "primary", onclick: create,
                        IconView { icon: Icon::Plus, size: 13 }
                        " Create connection"
                    }
                }
            }
        }
        if !archived.is_empty() {
            SettingGroup { title: "Archived connections",
                for c in archived {
                    SettingRow {
                        key: "{c.id}",
                        label: if c.name.trim().is_empty() { c.id.to_string() } else { c.name.clone() },
                        hint: "{c.agent.label()} · {tilde(&c.home, &home())} · its folder and login are kept",
                        button {
                            r#type: "button",
                            onclick: {
                                let (id, act) = (c.id.clone(), act);
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

/// One connection of its own: its name, folder, the profiles that run in
/// it, its login, and archiving it.
#[component]
fn ConnectionGroup(
    connection: AgentConnection,
    now: Settings,
    used_by: Vec<String>,
    probed: Signal<HashMap<ConnectionId, Probed>>,
    rename: Callback<String>,
    archive: Callback<()>,
) -> Element {
    let folder: PathBuf = connection.home.clone();
    let users = match used_by.is_empty() {
        true => "No profile runs in it.".to_string(),
        false => used_by.join(", "),
    };
    rsx! {
        SettingGroup { title: "{connection.agent.label()} · {profiles::connection_name(&now, &connection.id)}",
            SettingRow { label: "Name", hint: "Renaming changes neither its id nor its folder.",
                TextField {
                    value: "{connection.name}",
                    aria_label: "Connection name",
                    onchange: move |name: String| rename(name),
                }
            }
            SettingRow { label: "Folder", hint: tilde(&folder, &home()),
                button {
                    r#type: "button",
                    onclick: move |_| super::open_with("-R", &folder, ""),
                    IconView { icon: Icon::Folder, size: 13 }
                    " Show in Finder"
                }
            }
            SettingRow { label: "Used by", hint: users,
                button { r#type: "button", onclick: move |_| archive(()), "Archive" }
            }
            LoginRow {
                id: connection.id.clone(),
                label: "Login".to_string(),
                hint: String::new(),
                agent: connection.agent,
                now: now.clone(),
                probed,
            }
        }
    }
}

/// Whether the connection `id` is logged in, asked as the row is shown
/// unless it was already, with what to do next: open its login terminal, or
/// ask again. `now` names the profile whose CLI answered.
#[component]
fn LoginRow(
    id: ConnectionId,
    label: String,
    hint: String,
    agent: Agent,
    now: Settings,
    probed: Signal<HashMap<ConnectionId, Probed>>,
) -> Element {
    let link = use_context::<Link>();
    {
        let (link, id) = (link.clone(), id.clone());
        use_hook(move || {
            if !probed.peek().contains_key(&id) {
                probe(&link, probed, id, None);
            }
        });
    }
    let settings = now;
    let now = probed.read().get(&id).cloned();
    let (chip, class) = match &now {
        None | Some(Probed::Asking) => ("Checking…".to_string(), "login-standing"),
        Some(Probed::LoggingIn) => (
            "Waiting for the login terminal…".to_string(),
            "login-standing",
        ),
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
    let next = match &now {
        Some(Probed::Is(LoginStanding::LoggedOut, _)) => {
            "Open the login terminal and follow the CLI's own login there.".to_string()
        }
        Some(Probed::Is(LoginStanding::Missing, _)) => format!(
            "{}'s CLI cannot be started. Install it, or set the profile's launch command to where it is.",
            agent.label()
        ),
        Some(Probed::Is(LoginStanding::Unknown, _)) => {
            "The CLI did not say whether it is logged in.".to_string()
        }
        Some(Probed::Failed(why)) => why.clone(),
        _ => hint.clone(),
    };
    let hint = match (hint.is_empty(), next == hint) {
        (false, false) => format!("{hint} {next}"),
        _ => next,
    };
    let hint = match &now {
        Some(Probed::Is(_, profile)) => {
            let name = settings
                .find_profile(profile)
                .map_or_else(|| profile.to_string(), |p| p.label().to_string());
            format!("{hint} Asked of the CLI profile {name} starts.")
        }
        _ => hint,
    };
    let logged_out = matches!(now, Some(Probed::Is(LoginStanding::LoggedOut, _)));
    let busy = matches!(now, None | Some(Probed::Asking) | Some(Probed::LoggingIn));
    let (a, b) = (link.clone(), link);
    let (again, login) = (id.clone(), id);
    rsx! {
        SettingRow { label, hint,
            span { class, role: "status", "{chip}" }
            button {
                r#type: "button",
                title: "Ask the CLI again",
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
