//! A window showing a session not started yet, and the window's one body
//! while nothing is open: where it starts, its agent, model and
//! permissions, and the keys that get it going.

use std::path::PathBuf;

use adroit_core::msg::Msg;
use adroit_model::profile::{ConnectionId, LoginStanding, ProfileId};
use adroit_model::{Agent, AgentPermissions};
use std::time::Instant;

use dioxus::prelude::*;

use super::super::short;
use crate::actions::Action;
use crate::bridge::AppCtx;
use crate::components::{Button, EmptyState, Keys, Legend, LegendRow};
use crate::content::DraftOptions;
use crate::icons::{Icon, IconView};
use crate::keymap::{Context, Keymap};
use crate::logins::Login;

/// A draft in a window that is not focused: what it is and where.
#[component]
pub(crate) fn DraftHead(worktree: PathBuf) -> Element {
    rsx! {
        EmptyState { icon: Icon::MessagePlus, title: "New session",
            div { class: "console-empty-path", "{short(&worktree)}" }
        }
    }
}

/// The focused window's draft, in `worktree`: its agent, `agent` now, and
/// the options chosen for it.
#[component]
pub(crate) fn DraftForm(
    worktree: PathBuf,
    agent: Agent,
    profile: ProfileId,
    profiles: Vec<ProfileChoice>,
    options: DraftOptions,
    keymap: Keymap,
    onagent: EventHandler<Agent>,
    onprofile: EventHandler<ProfileId>,
    onoptions: EventHandler<DraftOptions>,
) -> Element {
    let k_agent = keymap.keys_for(Context::Draft, &Action::SessionAgentNext);
    let k_profile = keymap.keys_for(Context::Draft, &Action::SessionProfileNext);
    let k_send = keymap.keys_for(Context::Prompt, &Action::PromptSend);
    rsx! {
        EmptyState {
            div { class: "console-empty-head",
                IconView { icon: Icon::MessagePlus, size: 28 }
                div { class: "console-empty-title", "New session" }
                div { class: "console-empty-path", "{short(&worktree)}" }
            }
            div { class: "launch-form",
                AgentChoices { current: agent, keys: k_agent, onchoose: onagent }
                ProfileChoices { current: profile, profiles, keys: k_profile, onchoose: onprofile }
                DraftLaunchOptions { options, onchange: onoptions }
            }
            Legend {
                if let Some(k) = k_send {
                    LegendRow { does: "Send the prompt written below", Keys { keys: k } }
                }
                LegendRow { does: "As its first line, names the session", kbd { "name: …" } }
            }
        }
    }
}

/// The window with nothing open: the button that picks where a session
/// starts, the agent and options a scratch session starts with, and the
/// tree's keys.
#[component]
pub(crate) fn NothingOpen(
    agent: Agent,
    profile: ProfileId,
    profiles: Vec<ProfileChoice>,
    options: DraftOptions,
    keymap: Keymap,
    onpick: EventHandler<()>,
    onagent: EventHandler<Agent>,
    onprofile: EventHandler<ProfileId>,
    onoptions: EventHandler<DraftOptions>,
) -> Element {
    let km = keymap;
    let k_agent = km.keys_for(Context::Draft, &Action::SessionAgentNext);
    let k_profile = km.keys_for(Context::Draft, &Action::SessionProfileNext);
    // The button is pressed from wherever the focus is.
    let k_pick = km
        .keys_for(Context::Global, &Action::SessionPick)
        .or_else(|| km.keys_for(Context::Tree, &Action::SessionPick));
    let k_sidebar = km.keys_for(Context::Global, &Action::WindowLeft);
    let k_down = km.keys_for(Context::Tree, &Action::CursorDown);
    let k_up = km.keys_for(Context::Tree, &Action::CursorUp);
    let k_open = km.keys_for(Context::Tree, &Action::CursorGo);
    let k_open_beside = km.keys_for_all(
        Context::Tree,
        &[Action::CursorSplit, Action::CursorVsplit, Action::CursorTab],
    );
    let k_new = km.keys_for(Context::Tree, &Action::SessionNew);
    let k_shell = km.keys_for(Context::Tree, &Action::ShellOpen);
    rsx! {
        EmptyState { icon: Icon::LayoutDashboard, title: "Nothing open",
            div { class: "console-empty-start",
                Button {
                    kind: "primary",
                    onclick: move |_| onpick.call(()),
                    if let Some(k) = k_pick { Keys { keys: k } }
                    "Start a session"
                }
                div { class: "console-empty-hint", "Pick the directory it works in: a workspace, a Claude Code project, a ghq repository, or any folder." }
            }
            div { class: "launch-form",
                div { class: "launch-form-label", "Starts in" }
                div { class: "launch-form-where", "A scratch session" }
                AgentChoices { current: agent, keys: k_agent, onchoose: onagent }
                ProfileChoices { current: profile, profiles, keys: k_profile, onchoose: onprofile }
                DraftLaunchOptions { options, onchange: onoptions }
            }
            Legend {
                div { class: "console-legend-head",
                    "In the tree"
                    if let Some(k) = k_sidebar { span { class: "console-legend-reach", Keys { keys: k } " goes there" } }
                }
                if k_down.is_some() || k_up.is_some() {
                    LegendRow { does: "Move through the list",
                        if let Some(k) = k_down { Keys { keys: k } }
                        if let Some(k) = k_up { Keys { keys: k } }
                    }
                }
                if let Some(k) = k_open {
                    LegendRow { does: "Open the selected row", Keys { keys: k } }
                }
                if let Some(k) = k_open_beside {
                    LegendRow { does: "Open it in a split below, beside, or a new tab", Keys { keys: k } }
                }
                if let Some(k) = k_new {
                    LegendRow { does: "Start a session on a workspace row", Keys { keys: k } }
                }
                if let Some(k) = k_shell {
                    LegendRow { does: "Open a shell on a workspace row", Keys { keys: k } }
                }
            }
        }
    }
}

#[component]
fn AgentChoices(current: Agent, keys: Option<String>, onchoose: EventHandler<Agent>) -> Element {
    rsx! {
        div { class: "launch-form-label", "Agent" }
        div { class: "launch-form-agent",
            div { class: "segmented",
                for agent in Agent::ALL {
                    button {
                        key: "{agent.key()}",
                        class: if agent == current { "on" },
                        onclick: move |e| {
                            e.stop_propagation();
                            onchoose.call(agent);
                        },
                        "{agent.label()}"
                    }
                }
            }
            if let Some(keys) = keys {
                span { class: "launch-form-keys", title: "Switches the agent", Keys { keys } }
            }
        }
    }
}

/// A profile a new session can start from, as the draft offers it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProfileChoice {
    pub id: ProfileId,
    pub agent: Agent,
    pub connection: ConnectionId,
    /// What it is offered as; see [`adroit_core::profiles::offered_as`].
    pub label: String,
}

#[component]
fn ProfileChoices(
    current: ProfileId,
    profiles: Vec<ProfileChoice>,
    keys: Option<String>,
    onchoose: EventHandler<ProfileId>,
) -> Element {
    let chosen = profiles.iter().find(|p| p.id == current).cloned();
    rsx! {
        label { class: "launch-form-label", r#for: "launch-profile", "Profile" }
        div {
            class: "launch-form-agent",
            onmousedown: move |event| event.stop_propagation(),
            onkeydown: move |event| event.stop_propagation(),
            select {
                id: "launch-profile",
                aria_label: "Session profile",
                onchange: move |event| {
                    if let Ok(id) = event.value().parse::<ProfileId>() {
                        onchoose.call(id);
                    }
                },
                for p in profiles {
                    option { key: "{p.id}", value: "{p.id}", selected: p.id == current, "{p.label}" }
                }
            }
            if let Some(keys) = keys {
                span { class: "launch-form-keys", title: "Switches the profile", Keys { keys } }
            }
        }
        if let Some(chosen) = chosen {
            LoginNotice {
                key: "{chosen.id}",
                profile: chosen.id.clone(),
                connection: chosen.connection.clone(),
                agent: chosen.agent,
            }
        }
    }
}

/// What to do before the session can start when the CLI `profile` starts
/// in its connection says it is not logged in, or cannot be found; nothing
/// otherwise. Asked as the draft shows the profile, unless the window asked
/// a short while ago, and again once a login terminal opened here ends.
#[component]
fn LoginNotice(profile: ProfileId, connection: ConnectionId, agent: Agent) -> Element {
    let app = use_context::<AppCtx>();
    let ask = {
        let (profile, connection) = (profile.clone(), connection.clone());
        move || crate::logins::ask(app, profile.clone(), connection.clone(), None)
    };
    {
        let (profile, ask) = (profile.clone(), ask.clone());
        use_hook(move || {
            if crate::logins::stale(app.logins.peek().get(&profile), Instant::now()) {
                ask();
            }
        });
    }
    let standing = match app.logins.read().get(&profile) {
        Some(Login::Is(standing, _)) => Some(*standing),
        _ => None,
    };
    rsx! {
        match standing {
            Some(LoginStanding::LoggedOut) => rsx! {
                div { class: "launch-form-label", "Login" }
                div { class: "launch-form-agent", role: "status",
                    span { class: "launch-form-where", "Not logged in" }
                    Button {
                        kind: "primary",
                        onclick: move |_| {
                            app.dispatch(Msg::OpenLogin {
                                connection: connection.clone(),
                                profile: profile.clone(),
                            });
                        },
                        "Open login terminal"
                    }
                }
            },
            Some(LoginStanding::Missing) => rsx! {
                div { class: "launch-form-label", "Login" }
                div { class: "launch-form-agent", role: "status",
                    span { class: "launch-form-where",
                        "{agent.label()} isn't installed. Install it, or set this profile's launch command in Settings → Agents."
                    }
                    Button { onclick: move |_| ask.clone()(), "Check again" }
                }
            },
            _ => rsx! {},
        }
    }
}

#[component]
fn DraftLaunchOptions(options: DraftOptions, onchange: EventHandler<DraftOptions>) -> Element {
    let model_options = options.clone();
    let permission_options = options.clone();
    rsx! {
        div {
            class: "launch-form-options",
            onmousedown: move |event| event.stop_propagation(),
            onkeydown: move |event| event.stop_propagation(),
            label { class: "launch-form-label", r#for: "launch-model", "Model" }
            input {
                id: "launch-model",
                r#type: "text",
                aria_label: "Session model",
                placeholder: "Agent default",
                value: options.model.clone().unwrap_or_default(),
                oninput: move |event| {
                    let mut next = model_options.clone();
                    let value = event.value();
                    next.model = (!value.trim().is_empty()).then(|| value.trim().to_string());
                    onchange.call(next);
                },
            }
            label { class: "launch-form-label", r#for: "launch-permissions", "Permissions" }
            select {
                id: "launch-permissions",
                aria_label: "Session permissions",
                value: if options.permissions == AgentPermissions::ReadOnly { "read_only" } else { "default" },
                oninput: move |event| {
                    let mut next = permission_options.clone();
                    next.permissions = match event.value().as_str() {
                        "read_only" => AgentPermissions::ReadOnly,
                        _ => AgentPermissions::Default,
                    };
                    onchange.call(next);
                },
                option { value: "default", "Default" }
                option { value: "read_only", "Read only" }
            }
        }
    }
}
