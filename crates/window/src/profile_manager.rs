//! Profile creation and connection setup, apart from running work.

use crate::components::{SettingGroup, SettingRow, SettingsError, TextField};
use adroit_connect::{
    Target,
    profiles::{Catalog, Profile},
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
                .with_title("Profiles")
                .with_maximizable(false)
                .with_inner_size(LogicalSize::new(900.0, 640.0))
                .with_min_inner_size(LogicalSize::new(680.0, 440.0)),
        );
    let _ = window().new_window(VirtualDom::new(ProfileManager), cfg);
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
fn ProfileManager() -> Element {
    use_hook(|| {
        let me = window();
        OPEN.with(|o| *o.borrow_mut() = Rc::downgrade(&me));
    });
    let config = crate::feed::use_config();
    let style = crate::feed::root_style(&config.read());
    rsx! {
        style { "{style}" }
        div { class: "profile-manager",
            header { class: "profile-manager-head",
                h1 { "Profiles" }
                p { "Choose where you work. Each profile keeps its own agents, settings and sessions." }
            }
            ProfileEditor {}
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
    /// The work machine this profile was paired with, kept so it can be
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

    /// Takes the work machine pairing answered with as this profile's.
    fn paired_with(&mut self, target: Target) {
        if let Target::Native { address, server } = target {
            self.paired = Some((address, server));
            self.kind = Kind::Paired;
        }
    }
}

#[component]
fn ProfileEditor() -> Element {
    let mut catalog = use_signal(|| Catalog::load(&Catalog::path()));
    let active = adroit_connect::profiles::active().ok().flatten();
    let active_id = active.as_ref().map(|p| p.profile.clone());
    let initial = catalog.read().as_ref().ok().and_then(|c| {
        c.profiles
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
    let mut pair_code = use_signal(|| None::<String>);
    let mut pairing = use_signal(|| None::<dioxus::core::Task>);
    let archived = catalog
        .read()
        .as_ref()
        .ok()
        .and_then(|c| {
            c.profiles
                .iter()
                .find(|p| Some(&p.id) == selected.read().as_ref())
        })
        .is_some_and(|p| p.archived);
    let target_changed = catalog
        .read()
        .as_ref()
        .ok()
        .and_then(|c| {
            c.profiles
                .iter()
                .find(|p| Some(&p.id) == selected.read().as_ref())
        })
        .is_some_and(|p| choice.read().target().is_ok_and(|t| p.target != t));
    let name_changed = catalog
        .read()
        .as_ref()
        .ok()
        .and_then(|c| {
            c.profiles
                .iter()
                .find(|p| Some(&p.id) == selected.read().as_ref())
        })
        .is_none_or(|p| p.name != name().trim());
    rsx! {
        if let Err(why) = &*catalog.read() { SettingsError { "{why}" } }
        div { class: "profile-manager-body",
            nav { class: "environment-profile-list", aria_label: "Profiles",
                if let Ok(catalog) = &*catalog.read() {
                    for profile in &catalog.profiles {
                        button { r#type: "button", aria_pressed: Some(&profile.id) == selected.read().as_ref(), class: if Some(&profile.id) == selected.read().as_ref() { "environment-profile on" } else { "environment-profile" },
                            onclick: {
                                let profile = profile.clone();
                                move |_| { selected.set(Some(profile.id.clone())); name.set(profile.name.clone()); choice.set(Choice::from(profile.target.clone())); error.set(None); notice.set(None); removing.set(false); }
                            },
                            span { "{profile.name}" }
                            small { if profile.archived { "Removed · select to restore" } else if matches!(profile.target, Target::Local) { "This computer" } else if let Target::Native { address, .. } = &profile.target { "{address}" } }
                        }
                    }
                }
                button { r#type: "button", class: "environment-profile new", onclick: move |_| { selected.set(None); name.set(String::new()); choice.set(Choice::from(Target::Local)); error.set(None); notice.set(None); removing.set(false); }, "+ New profile" }
            }
            main { class: "profile-editor",
                SettingGroup { title: if selected().is_none() { "New profile" } else { "Profile" },
                    SettingRow { label: "Name", hint: "",
                        TextField { aria_label: "Profile name", value: name(), placeholder: "Work", onchange: move |value| name.set(value) }
                    }
                    SettingRow { label: "Connection", hint: "Agents and working files stay on this computer or the work machine you choose.",
                        select { aria_label: "Profile connection", disabled: busy() || pair_code.read().is_some(), onchange: move |e| { choice.write().kind = Kind::from_value(&e.value()); notice.set(None); },
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
                        SettingRow { label: "Address", hint: "The work machine's address and port. On it, run adroit remote enable once, then adroit remote pair.",
                            TextField { aria_label: "Work machine address", value: choice.read().pairing.clone(), placeholder: "work-box.local:47130", onchange: move |value| { choice.write().pairing = value; } }
                        }
                        SettingRow { label: "On this network", hint: "Work machines that announce themselves here.",
                            div { class: "paired-machines",
                                for found in heard.read().clone() {
                                    button { r#type: "button", disabled: pair_code.read().is_some(), title: "Key {found.key}", onclick: { let address = found.address.clone(); move |_| choice.write().pairing = address.clone() }, "{found.name} · {found.address}" }
                                }
                                button { r#type: "button", disabled: busy() || pair_code.read().is_some(), onclick: move |_| {
                                    busy.set(true); error.set(None);
                                    spawn(async move {
                                        match adroit_connect::discover().await {
                                            Ok(found) if found.is_empty() => notice.set(Some("No work machine announced itself on this network. Enter its address instead.".into())),
                                            Ok(found) => heard.set(found),
                                            Err(why) => error.set(Some(why)),
                                        }
                                        busy.set(false);
                                    });
                                }, if busy() { "Looking…" } else { "Look for work machines" } }
                            }
                        }
                        if let Some(code) = pair_code.read().clone() {
                            p { class: "pairing-code", aria_live: "polite", "Type this code on the work machine, where adroit remote pair shows this computer's request: " strong { "{code}" } }
                        }
                        div { class: "settings-page-actions",
                            if pairing.read().is_some() {
                                button { r#type: "button", onclick: move |_| {
                                    if let Some(task) = pairing.write().take() { task.cancel(); }
                                    pair_code.set(None);
                                }, "Cancel" }
                            } else {
                                button { r#type: "button", class: "primary", disabled: busy() || choice.read().pairing.trim().is_empty(), onclick: move |_| {
                                    error.set(None); notice.set(None);
                                    let address = choice.peek().pairing.trim().to_string();
                                    let task = spawn(async move {
                                        let result = adroit_connect::pair_with(&address, None, |code| pair_code.set(Some(code))).await;
                                        pair_code.set(None);
                                        pairing.set(None);
                                        match result {
                                            Ok(target) => {
                                                choice.write().paired_with(target);
                                                notice.set(Some(format!("Paired with the work machine at {address}. Save the profile to use it.")));
                                            }
                                            Err(why) => error.set(Some(why)),
                                        }
                                    });
                                    pairing.set(Some(task));
                                }, "Pair" }
                            }
                        }
                    }
                }
                if target_changed { p { class: "settings-agent-note", "This connection will be saved as a new profile. The original profile, agents and running sessions stay where they are." } }
                div { class: "settings-page-actions profile-actions",
                    button { r#type: "button", disabled: busy(), onclick: move |_| {
                        notice.set(None); error.set(None);
                        match choice.read().target() {
                            Err(why) => error.set(Some(why)),
                            Ok(target) => { let testing = choice.peek().clone(); busy.set(true); spawn(async move {
                                let result = adroit_connect::check(&target).await;
                                if *choice.peek() != testing { busy.set(false); return; }
                                match result {
                                    Ok(_) => { notice.set(Some("Connected. Open this profile to configure the agents installed there.".into())); }
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
                            let mut next = catalog.peek().as_ref().map_err(Clone::clone)?.clone();
                            let revision = next.revision;
                            let mut profile = next.profiles.iter().find(|p| Some(&p.id) == selected.peek().as_ref() && p.target == target).cloned().unwrap_or_else(|| Profile::new(&name(), target));
                            profile.name = name().trim().into();
                            if let Some(old) = next.profiles.iter_mut().find(|p| p.id == profile.id) { *old = profile.clone(); } else { next.profiles.push(profile.clone()); }
                            next.save(&Catalog::path(), revision)?;
                            selected.set(Some(profile.id));
                            catalog.set(Ok(next));
                            Ok::<_, String>(())
                        })();
                        match result { Ok(()) => { error.set(None); notice.set(Some("Profile saved. Open it to use its connection and agents.".into())); }, Err(why) => error.set(Some(why)) }
                    }, if selected().is_none() || target_changed { "Create profile" } else { "Save changes" } }
                    }
                    if selected().is_some() && !target_changed {
                        button { r#type: "button", class: if name_changed { "" } else { "primary" }, disabled: busy(), onclick: move |_| {
                            if let Ok(catalog) = &*catalog.peek()
                                && let Some(profile) = catalog.profiles.iter().find(|p| Some(&p.id) == selected.peek().as_ref()) { error.set(crate::environment_profiles::open(profile, false).err()); }
                        }, "Open profile" }
                    }
                }
                div { class: "profile-management",
                    SettingGroup { title: "Profile actions",
                        if selected().is_some() && !target_changed {
                        SettingRow { label: "Open another window", hint: "Use this profile in another window, with its own tabs and drafts.",
                        button { r#type: "button", disabled: busy(), onclick: move |_| {
                            if let Ok(catalog) = &*catalog.peek()
                                && let Some(profile) = catalog.profiles.iter().find(|p| Some(&p.id) == selected.peek().as_ref()) { error.set(crate::environment_profiles::open(profile, true).err()); }
                        }, "Open window" }
                        }
                        }
                        SettingRow { label: "Refresh profiles", hint: "Reload saved profiles, including changes made in other windows.",
                            button { r#type: "button", onclick: move |_| { catalog.set(Catalog::load(&Catalog::path())); error.set(None); notice.set(None); }, "Refresh" }
                        }
                if selected().is_some() {
                        if archived {
                        SettingRow { label: "Restore profile", hint: "Show this profile in the profile menu again.",
                        button { r#type: "button", onclick: move |_| {
                            let result = (|| {
                                let mut next = catalog.peek().as_ref().map_err(Clone::clone)?.clone();
                                let revision = next.revision;
                                for profile in &mut next.profiles { if Some(&profile.id) == selected.peek().as_ref() { profile.archived = false; } }
                                next.save(&Catalog::path(), revision)?; catalog.set(Ok(next));
                                Ok::<_, String>(())
                            })(); error.set(result.err());
                        }, "Restore" }
                        }
                        }
                        SettingRow { label: "New profile from this connection", hint: "Reuse the connection. Agent setups and work settings start fresh.",
                            button { r#type: "button", onclick: move |_| { selected.set(None); name.set(format!("{} copy", name())); removing.set(false); notice.set(Some("Only the connection is copied. The new profile starts with its own agents and work settings.".into())); }, "New profile…" }
                        }
                        if !archived {
                        SettingRow { label: "Remove from profile list", hint: "Running sessions, credentials and files are kept. You can restore the profile later.",
                            button { r#type: "button", class: "danger", onclick: move |_| removing.set(true), "Remove…" }
                        }
                        }
                    }
                }
                if removing() {
                    div { class: "settings-confirm",
                    p { class: "settings-agent-note", "Remove this profile from the list? Running sessions, credentials and files on the work machine are kept." }
                    button { r#type: "button", onclick: move |_| {
                        let result = (|| {
                            let mut next = catalog.peek().as_ref().map_err(Clone::clone)?.clone();
                            let revision = next.revision;
                            for profile in &mut next.profiles { if Some(&profile.id) == selected.peek().as_ref() { profile.archived = true; } }
                            next.save(&Catalog::path(), revision)?;
                            catalog.set(Ok(next)); selected.set(None); removing.set(false);
                            Ok::<_, String>(())
                        })();
                        error.set(result.err());
                    }, "Remove registration" }
                    button { r#type: "button", onclick: move |_| removing.set(false), "Keep profile" }
                    }
                }
                }
                if let Some(why) = error() { SettingsError { "{why}" } }
                if let Some(text) = notice() { p { class: "settings-agent-note", role: "status", "{text}" } }
                if choice.read().kind != Kind::Local {
                    details { class: "connection-help settings-advanced",
                        summary { "Remote feature availability" }
                        p { "Remote console typing, local Finder/editor actions on remote files and interactive login terminals are currently unavailable. Log in to your agents on the work machine and check their status under this profile's Agents." }
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
}
