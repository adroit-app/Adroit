//! Environment creation and connection setup, apart from running work.

use crate::components::{SettingGroup, SettingRow, SettingsError, TextField};
use crate::icons::{Icon, IconView};
use adroit_connect::{
    Target,
    environments::{Catalog, Environment},
};
use dioxus::prelude::*;

use dioxus::desktop::{
    Config as Desktop, DesktopService, LogicalSize, WindowBuilder, WindowCloseBehaviour, window,
};
use std::cell::RefCell;
use std::rc::{Rc, Weak};

thread_local! {
    static OPEN: RefCell<Weak<DesktopService>> = const { RefCell::new(Weak::new()) };
}

pub(crate) fn open() {
    if let Some(w) = OPEN.with(|o| o.borrow().upgrade()) {
        w.window.set_visible(true);
        w.window.set_focus();
        return;
    }
    let cfg = Desktop::new()
        .with_menu(None)
        .with_close_behaviour(WindowCloseBehaviour::WindowCloses)
        .with_background_color((15, 17, 20, 255))
        .with_custom_head(format!("<style>{}</style>", crate::style::SETTINGS_CSS))
        .with_window(
            WindowBuilder::new()
                .with_title("Environments")
                .with_maximizable(false)
                .with_inner_size(LogicalSize::new(900.0, 640.0))
                .with_min_inner_size(LogicalSize::new(680.0, 440.0)),
        );
    let _ = window().new_window(VirtualDom::new(EnvironmentManager), cfg);
}

pub(crate) fn close() {
    if let Some(w) = OPEN.with(|o| o.borrow().upgrade()) {
        w.close();
    }
}

pub(crate) fn close_in_front() -> bool {
    let Some(w) = OPEN.with(|o| o.borrow().upgrade()) else {
        return false;
    };
    if !w.window.is_focused() {
        return false;
    }
    w.close();
    true
}

#[component]
fn EnvironmentManager() -> Element {
    use_hook(|| {
        let me = window();
        OPEN.with(|o| *o.borrow_mut() = Rc::downgrade(&me));
    });
    let config = crate::feed::use_config();
    let style = crate::feed::root_style(&config.read());
    rsx! {
        style { "{style}" }
        div { class: "environment-manager",
            header { class: "environment-manager-head",
                h1 { "Environments" }
                p { "Choose where you work. Each environment keeps its own agents, settings and sessions." }
            }
            EnvironmentEditor {}
        }
    }
}

/// Which connection the editor shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Local,
    Paired,
    /// Pairing with a new work machine, which has no connection to save yet.
    Pairing,
}

impl Kind {
    fn value(self) -> &'static str {
        match self {
            Kind::Local => "local",
            Kind::Paired => "paired",
            Kind::Pairing => "pair",
        }
    }

    fn from_value(value: &str) -> Self {
        [Kind::Paired, Kind::Pairing]
            .into_iter()
            .find(|kind| kind.value() == value)
            .unwrap_or(Kind::Local)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Choice {
    kind: Kind,
    /// The work machine this environment was paired with, kept so it can be
    /// chosen again after another connection was tried.
    paired: Option<(String, String)>,
    /// The address being paired with.
    pairing: String,
}

impl From<Target> for Choice {
    fn from(target: Target) -> Self {
        let local = Self {
            kind: Kind::Local,
            paired: None,
            pairing: String::new(),
        };
        match target {
            Target::Local => local,
            Target::Native { address, server } => Self {
                kind: Kind::Paired,
                paired: Some((address, server)),
                ..local
            },
        }
    }
}

impl Choice {
    fn target(&self) -> Result<Target, String> {
        let target = match (self.kind, &self.paired) {
            (Kind::Local, _) => Target::Local,
            (Kind::Paired, Some((address, server))) => Target::Native {
                address: address.clone(),
                server: server.clone(),
            },
            (Kind::Paired | Kind::Pairing, _) => {
                return Err("Pair with the work machine first".into());
            }
        };
        target.validate()?;
        Ok(target)
    }

    /// Shows `kind`; pairing starts from the machine paired before, which is
    /// the one to pair again after it forgot this computer.
    fn choose(&mut self, kind: Kind) {
        self.kind = kind;
        if kind == Kind::Pairing
            && self.pairing.is_empty()
            && let Some((address, _)) = &self.paired
        {
            self.pairing = address.clone();
        }
    }

    /// Takes the work machine pairing answered with as this environment's.
    fn paired_with(&mut self, target: Target) {
        if let Target::Native { address, server } = target {
            self.paired = Some((address, server));
            self.kind = Kind::Paired;
        }
    }
}

/// `catalog` with `name` and `target` saved to `path`: in place for the
/// environment `selected` when its connection is `target`, as an environment of its own
/// otherwise; with the saved environment's id.
fn save(
    catalog: &Catalog,
    path: &std::path::Path,
    selected: Option<&str>,
    name: &str,
    target: Target,
) -> Result<(Catalog, String), String> {
    let mut next = catalog.clone();
    let revision = next.revision;
    let mut environment = next
        .environments
        .iter()
        .find(|p| Some(p.id.as_str()) == selected && p.target == target)
        .cloned()
        .unwrap_or_else(|| Environment::new(name, target));
    environment.name = name.trim().into();
    match next
        .environments
        .iter_mut()
        .find(|p| p.id == environment.id)
    {
        Some(old) => *old = environment.clone(),
        None => next.environments.push(environment.clone()),
    }
    next.save(path, revision)?;
    Ok((next, environment.id))
}

/// A name for the work machine at `address`: its host, without the port, or
/// what a loopback address means.
fn machine_name(address: &str) -> String {
    let host = address.rsplit_once(':').map_or(address, |(host, _)| host);
    let host = host.trim_matches(['[', ']']);
    let loopback = host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    match loopback {
        true => "This computer's gateway".into(),
        false => host.to_string(),
    }
}

#[component]
fn EnvironmentEditor() -> Element {
    let mut catalog = use_signal(|| Catalog::load(&Catalog::path()));
    let active = adroit_connect::environments::active().ok().flatten();
    let active_id = active.as_ref().map(|p| p.environment.clone());
    let initial = catalog.read().as_ref().ok().and_then(|c| {
        c.environments
            .iter()
            .find(|p| Some(&p.id) == active_id.as_ref())
            .cloned()
    });
    let mut selected = use_signal(|| initial.as_ref().map(|p| p.id.clone()));
    let mut name = use_signal(|| {
        initial
            .as_ref()
            .map_or_else(String::new, |p| p.name.clone())
    });
    let mut choice = use_signal(|| Choice::from(initial.map_or(Target::Local, |p| p.target)));
    let mut error = use_signal(|| None::<String>);
    let mut notice = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut removing = use_signal(|| false);
    let mut heard = use_signal(Vec::<adroit_connect::Heard>::new);
    let mut pair_step = use_signal(|| None::<adroit_connect::PairingStep>);
    // Kept past the next code, so the person knows why the code changed.
    let mut mismatched = use_signal(|| false);
    let mut pairing = use_signal(|| None::<dioxus::core::Task>);
    let mut serving = use_signal(|| false);
    let mut looking = use_signal(|| false);
    let mut look = move || {
        if *looking.peek() {
            return;
        }
        looking.set(true);
        spawn(async move {
            match adroit_connect::discover().await {
                Ok(found) => heard.set(found),
                Err(why) => error.set(Some(why)),
            }
            looking.set(false);
        });
    };
    // A memo, so typing the address does not look again on every key.
    let choosing_machine = use_memo(move || choice.read().kind == Kind::Pairing);
    use_effect(move || {
        if choosing_machine() {
            look();
        }
    });
    let archived = catalog
        .read()
        .as_ref()
        .ok()
        .and_then(|c| {
            c.environments
                .iter()
                .find(|p| Some(&p.id) == selected.read().as_ref())
        })
        .is_some_and(|p| p.archived);
    let target_changed = catalog
        .read()
        .as_ref()
        .ok()
        .and_then(|c| {
            c.environments
                .iter()
                .find(|p| Some(&p.id) == selected.read().as_ref())
        })
        .is_some_and(|p| choice.read().target().is_ok_and(|t| p.target != t));
    let name_changed = catalog
        .read()
        .as_ref()
        .ok()
        .and_then(|c| {
            c.environments
                .iter()
                .find(|p| Some(&p.id) == selected.read().as_ref())
        })
        .is_none_or(|p| p.name != name().trim());
    rsx! {
        if let Err(why) = &*catalog.read() { SettingsError { "{why}" } }
        div { class: "environment-manager-body",
            nav { class: "environment-list", aria_label: "Environments",
                if let Ok(catalog) = &*catalog.read() {
                    for environment in &catalog.environments {
                        button { r#type: "button", aria_pressed: !serving() && Some(&environment.id) == selected.read().as_ref(), class: if !serving() && Some(&environment.id) == selected.read().as_ref() { "environment-entry on" } else { "environment-entry" },
                            onclick: {
                                let environment = environment.clone();
                                move |_| { serving.set(false); selected.set(Some(environment.id.clone())); name.set(environment.name.clone()); choice.set(Choice::from(environment.target.clone())); error.set(None); notice.set(None); removing.set(false); }
                            },
                            span { "{environment.name}" }
                            small { if environment.archived { "Removed · select to restore" } else if matches!(environment.target, Target::Local) { "This computer" } else if let Target::Native { address, .. } = &environment.target { "{address}" } }
                        }
                    }
                }
                button { r#type: "button", class: "environment-entry new", onclick: move |_| { serving.set(false); selected.set(None); name.set(String::new()); choice.set(Choice::from(Target::Local)); error.set(None); notice.set(None); removing.set(false); }, "+ New environment" }
                div { class: "environment-list-foot",
                    h2 { class: "settings-nav-heading", "This computer" }
                    button { r#type: "button", aria_pressed: serving(), class: if serving() { "environment-entry on" } else { "environment-entry" }, onclick: move |_| serving.set(true),
                        span { "Accept connections" }
                        small { "Let others work here" }
                    }
                }
            }
            if serving() {
                main { class: "environment-editor",
                    // Pairing with this computer's own gateway needs the code
                    // while its request is answered here.
                    if let Some(adroit_connect::PairingStep::Shown { code }) = pair_step.read().clone() {
                        p { class: "pairing-code", aria_live: "polite", "This computer is pairing with {choice.read().pairing}. Its code: " strong { "{code}" } }
                    }
                    crate::serving_pane::ServingPane {}
                }
            } else {
            main { class: "environment-editor",
                SettingGroup { title: if selected().is_none() { "New environment" } else { "Environment" },
                    SettingRow { label: "Name", hint: "",
                        TextField { aria_label: "Environment name", value: name(), placeholder: "Work", onchange: move |value| name.set(value) }
                    }
                    SettingRow { label: "Connection", hint: "Agents and working files stay on this computer or the work machine you choose.",
                        select { aria_label: "Environment connection", disabled: busy() || pairing.read().is_some(), onchange: move |e| { choice.write().choose(Kind::from_value(&e.value())); notice.set(None); },
                            option { value: Kind::Local.value(), selected: choice.read().kind == Kind::Local, "This computer" }
                            if let Some((address, _)) = choice.read().paired.clone() {
                                option { value: Kind::Paired.value(), selected: choice.read().kind == Kind::Paired, "Paired work machine at {address}" }
                            }
                            option { value: Kind::Pairing.value(), selected: choice.read().kind == Kind::Pairing, "Pair a work machine…" }
                        }
                    }
                }
                if choice.read().kind == Kind::Pairing {
                    SettingGroup { title: "Pair a work machine",
                        SettingRow { label: "Address", hint: "The work machine's address and port. On it, turn on Accept connections and allow pairing in Environments.",
                            // Kept on every key, so Pair is enabled as soon as an address is typed.
                            input { class: "settings-command", r#type: "text", aria_label: "Work machine address", placeholder: "work-box.local:47130", spellcheck: false, autocomplete: "off",
                                disabled: pairing.read().is_some(),
                                value: choice.read().pairing.clone(),
                                oninput: move |e| { choice.write().pairing = e.value(); },
                            }
                            if pairing.read().is_some() {
                                button { r#type: "button", onclick: move |_| {
                                    if let Some(task) = pairing.write().take() { task.cancel(); }
                                    pair_step.set(None);
                                    mismatched.set(false);
                                }, "Cancel" }
                            } else {
                                button { r#type: "button", class: "primary", disabled: busy() || choice.read().pairing.trim().is_empty(), onclick: move |_| {
                                    error.set(None); notice.set(None);
                                    let address = choice.peek().pairing.trim().to_string();
                                    let task = spawn(async move {
                                        let result = adroit_connect::pair_with(&address, None, |step| {
                                            if step == adroit_connect::PairingStep::Mismatched { mismatched.set(true); }
                                            pair_step.set(Some(step));
                                        }).await;
                                        pair_step.set(None);
                                        mismatched.set(false);
                                        pairing.set(None);
                                        match result {
                                            Ok(target) => {
                                                choice.write().paired_with(target.clone());
                                                // Pairing is what the operator came for, so                                                 // environment is kept without a second step.
                                                let named = match (selected.peek().is_none(), name.peek().trim()) {
                                                    (true, typed) if !typed.is_empty() => typed.to_string(),
                                                    _ => machine_name(&address),
                                                };
                                                let saved = catalog.peek().as_ref().map_err(Clone::clone).and_then(|catalog| {
                                                    // Pairing again keeps the machine's environment; pairing
                                                    // from another environment adds one beside it.
                                                    match catalog.environments.iter().find(|p| p.target == target) {
                                                        Some(known) => Ok((catalog.clone(), known.id.clone())),
                                                        None => save(catalog, &Catalog::path(), None, &named, target),
                                                    }
                                                });
                                                match saved {
                                                    Ok((next, id)) => {
                                                        selected.set(Some(id));
                                                        name.set(named.clone());
                                                        catalog.set(Ok(next));
                                                        notice.set(Some(format!("Paired with the work machine at {address} and saved it as {named}. Open it to work there.")));
                                                    }
                                                    Err(why) => error.set(Some(why)),
                                                }
                                            }
                                            Err(why) => error.set(Some(why)),
                                        }
                                    });
                                    pairing.set(Some(task));
                                }, "Pair" }
                            }
                        }
                        SettingRow { label: "On this network", hint: "Work machines that announce themselves here. Choose one to pair with it.",
                            button { r#type: "button", class: if looking() { "icon-button looking" } else { "icon-button" }, title: "Look again", aria_label: "Look for work machines again",
                                disabled: looking() || pairing.read().is_some(),
                                onclick: move |_| look(),
                                IconView { icon: Icon::Refresh, size: 14 }
                            }
                        }
                        div { class: "found-machines", role: "list", aria_busy: looking(),
                            for found in heard.read().clone() {
                                button { key: "{found.key}", r#type: "button", role: "listitem",
                                    class: if choice.read().pairing.trim() == found.address { "found-machine on" } else { "found-machine" },
                                    aria_pressed: choice.read().pairing.trim() == found.address,
                                    disabled: pairing.read().is_some(),
                                    onclick: { let address = found.address.clone(); move |_| choice.write().pairing = address.clone() },
                                    span { class: "found-machine-name", "{found.name}" }
                                    small { class: "found-machine-detail", "{found.address} · Key {found.key}" }
                                }
                            }
                            if heard.read().is_empty() {
                                p { class: "found-machines-empty",
                                    if looking() { "Looking for work machines…" } else { "No work machine announced itself on this network. Enter its address above instead." }
                                }
                            }
                        }
                        match pair_step.read().clone() {
                            Some(adroit_connect::PairingStep::NotAccepting) => rsx! {
                                p { class: "pairing-code", aria_live: "polite", "Waiting for the work machine to allow pairing. On it, open Environments, choose Accept connections and press Allow pairing." }
                            },
                            Some(adroit_connect::PairingStep::Shown { code }) => rsx! {
                                p { class: "pairing-code", aria_live: "polite",
                                    if mismatched() { "That was not the code, so this is a new one. " }
                                    "On the work machine, type this code beside this computer's request: "
                                    strong { "{code}" }
                                }
                            },
                            Some(adroit_connect::PairingStep::Mismatched) => rsx! {
                                p { class: "pairing-code", aria_live: "polite", "That was not the code. Asking again with a new one…" }
                            },
                            None => rsx! {},
                        }
                    }
                    // Where pairing is shown, so its outcome is seen without scrolling.
                    if let Some(why) = error() { SettingsError { "{why}" } }
                    if let Some(text) = notice() { p { class: "settings-agent-note", role: "status", "{text}" } }
                }
                // Nothing is saved or tested until pairing answered; it saves the environment itself.
                if choice.read().kind != Kind::Pairing {
                if target_changed { p { class: "settings-agent-note", "This connection will be saved as a new environment. The original environment, agents and running sessions stay where they are." } }
                div { class: "settings-page-actions environment-actions",
                    button { r#type: "button", disabled: busy(), onclick: move |_| {
                        notice.set(None); error.set(None);
                        match choice.read().target() {
                            Err(why) => error.set(Some(why)),
                            Ok(target) => { let testing = choice.peek().clone(); busy.set(true); spawn(async move {
                                let result = adroit_connect::check(&target).await;
                                if *choice.peek() != testing { busy.set(false); return; }
                                match result {
                                    Ok(_) => { notice.set(Some("Connected. Open this environment to configure the agents installed there.".into())); }
                                    Err(why) => { error.set(Some(why)); }
                                }
                                busy.set(false);
                            }); }
                        }
                    }, if busy() { "Checking…" } else { "Test connection" } }
                    if selected().is_none() || target_changed || name_changed {
                    button { r#type: "button", class: "primary", disabled: busy(), onclick: move |_| {
                        let result = (|| {
                            let target = choice.read().target()?;
                            let (next, id) = save(catalog.peek().as_ref().map_err(Clone::clone)?, &Catalog::path(), selected.peek().as_deref(), &name(), target)?;
                            selected.set(Some(id));
                            catalog.set(Ok(next));
                            Ok::<_, String>(())
                        })();
                        match result { Ok(()) => { error.set(None); notice.set(Some("Environment saved. Open it to use its connection and agents.".into())); }, Err(why) => error.set(Some(why)) }
                    }, if selected().is_none() || target_changed { "Create environment" } else { "Save changes" } }
                    }
                    if selected().is_some() && !target_changed {
                        button { r#type: "button", class: if name_changed { "" } else { "primary" }, disabled: busy(), onclick: move |_| {
                            if let Ok(catalog) = &*catalog.peek()
                                && let Some(environment) = catalog.environments.iter().find(|p| Some(&p.id) == selected.peek().as_ref()) { error.set(crate::environments::open(environment, false).err()); }
                        }, "Open environment" }
                    }
                }
                // Beside the buttons, so what they did is seen without scrolling.
                if let Some(why) = error() { SettingsError { "{why}" } }
                if let Some(text) = notice() { p { class: "settings-agent-note", role: "status", "{text}" } }
                }
                div { class: "environment-management",
                    SettingGroup { title: "Environment actions",
                        if selected().is_some() && !target_changed {
                        SettingRow { label: "Open another window", hint: "Use this environment in another window, with its own tabs and drafts.",
                        button { r#type: "button", disabled: busy(), onclick: move |_| {
                            if let Ok(catalog) = &*catalog.peek()
                                && let Some(environment) = catalog.environments.iter().find(|p| Some(&p.id) == selected.peek().as_ref()) { error.set(crate::environments::open(environment, true).err()); }
                        }, "Open window" }
                        }
                        }
                        SettingRow { label: "Refresh environments", hint: "Reload saved environments, including changes made in other windows.",
                            button { r#type: "button", onclick: move |_| { catalog.set(Catalog::load(&Catalog::path())); error.set(None); notice.set(None); }, "Refresh" }
                        }
                if selected().is_some() {
                        if archived {
                        SettingRow { label: "Restore environment", hint: "Show this environment in the environment menu again.",
                        button { r#type: "button", onclick: move |_| {
                            let result = (|| {
                                let mut next = catalog.peek().as_ref().map_err(Clone::clone)?.clone();
                                let revision = next.revision;
                                for environment in &mut next.environments { if Some(&environment.id) == selected.peek().as_ref() { environment.archived = false; } }
                                next.save(&Catalog::path(), revision)?; catalog.set(Ok(next));
                                Ok::<_, String>(())
                            })(); error.set(result.err());
                        }, "Restore" }
                        }
                        }
                        SettingRow { label: "New environment from this connection", hint: "Reuse the connection. Agent setups and work settings start fresh.",
                            button { r#type: "button", onclick: move |_| { selected.set(None); name.set(format!("{} copy", name())); removing.set(false); notice.set(Some("Only the connection is copied. The new environment starts with its own agents and work settings.".into())); }, "New environment…" }
                        }
                        if !archived {
                        SettingRow { label: "Remove from environment list", hint: "Running sessions, credentials and files are kept. You can restore the environment later.",
                            button { r#type: "button", class: "danger", onclick: move |_| removing.set(true), "Remove…" }
                        }
                        }
                    }
                }
                if removing() {
                    div { class: "settings-confirm",
                    p { class: "settings-agent-note", "Remove this environment from the list? Running sessions, credentials and files on the work machine are kept." }
                    button { r#type: "button", onclick: move |_| {
                        let result = (|| {
                            let mut next = catalog.peek().as_ref().map_err(Clone::clone)?.clone();
                            let revision = next.revision;
                            for environment in &mut next.environments { if Some(&environment.id) == selected.peek().as_ref() { environment.archived = true; } }
                            next.save(&Catalog::path(), revision)?;
                            catalog.set(Ok(next)); selected.set(None); removing.set(false);
                            Ok::<_, String>(())
                        })();
                        error.set(result.err());
                    }, "Remove registration" }
                    button { r#type: "button", onclick: move |_| removing.set(false), "Keep environment" }
                    }
                }
                }
                if choice.read().kind != Kind::Local {
                    details { class: "connection-help settings-advanced",
                        summary { "Remote feature availability" }
                        p { "Remote console typing, local Finder/editor actions on remote files and interactive login terminals are currently unavailable. Log in to your agents on the work machine and check their status under this environment's Agents." }
                    }
                }
            }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn paired() -> Target {
        Target::Native {
            address: "work-box.local:47130".into(),
            server: "ab".repeat(32),
        }
    }

    #[test]
    fn a_connection_being_paired_cannot_be_saved_until_the_machine_answers() {
        let mut choice = Choice::from(Target::Local);
        choice.kind = Kind::Pairing;
        assert!(choice.target().is_err());
        choice.paired_with(paired());
        assert_eq!(choice.kind, Kind::Paired);
        assert_eq!(choice.target().unwrap(), paired());
    }

    #[test]
    fn a_paired_machine_stays_choosable_while_another_connection_is_tried() {
        let mut choice = Choice::from(paired());
        choice.kind = Kind::Local;
        choice.kind = Kind::Pairing;
        choice.kind = Kind::Paired;
        assert_eq!(choice.target().unwrap(), paired());
    }

    #[test]
    fn pairing_again_starts_from_the_address_of_the_machine_paired_before() {
        let mut choice = Choice::from(paired());
        choice.choose(Kind::Pairing);
        assert_eq!(choice.pairing, "work-box.local:47130");
        choice.pairing = "other.local:47130".into();
        choice.choose(Kind::Local);
        choice.choose(Kind::Pairing);
        assert_eq!(choice.pairing, "other.local:47130", "what was typed stays");
        let mut fresh = Choice::from(Target::Local);
        fresh.choose(Kind::Pairing);
        assert_eq!(fresh.pairing, "");
    }

    #[test]
    fn a_work_machine_is_named_by_its_host() {
        assert_eq!(machine_name("work-box.local:47130"), "work-box.local");
        assert_eq!(machine_name("[fe80::1]:47130"), "fe80::1");
        assert_eq!(machine_name("work-box"), "work-box");
        assert_eq!(machine_name("127.0.0.1:47130"), "This computer's gateway");
        assert_eq!(machine_name("[::1]:47130"), "This computer's gateway");
        assert_eq!(machine_name("localhost:47130"), "This computer's gateway");
    }

    #[test]
    fn a_changed_connection_is_saved_as_an_environment_of_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("environments.toml");
        let (catalog, local) =
            save(&Catalog::default(), &path, None, "Personal", Target::Local).unwrap();
        let (catalog, renamed) =
            save(&catalog, &path, Some(&local), "Mine", Target::Local).unwrap();
        assert_eq!(renamed, local);
        let (catalog, machine) = save(&catalog, &path, Some(&local), "work-box", paired()).unwrap();
        assert_ne!(machine, local);
        let saved = Catalog::load(&path).unwrap();
        assert_eq!(saved.environments, catalog.environments);
        let names: Vec<_> = saved.environments.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Mine", "work-box"]);
    }
}
