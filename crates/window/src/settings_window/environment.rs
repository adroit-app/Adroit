//! The environment the Settings window configures, as this computer keeps
//! it: its name, its connection and its removal; and the environments
//! removed, to restore.

use adroit_connect::Target;
use adroit_connect::environments::{Catalog, Environment};
use adroit_model::EnvironmentId;
use dioxus::prelude::*;

use super::Edited;
use crate::components::{SettingGroup, SettingRow, SettingsError, TextField};
use crate::environments::{self, fresh_name};

/// What `target` is called where a connection is chosen.
fn connection_name(target: &Target) -> String {
    match target {
        Target::Local => "This computer".into(),
        Target::Native { address, .. } => format!("Work machine at {address}"),
    }
}

/// The connections an environment on `target` can be moved to, its own
/// first: a work machine is reached only once paired, so only this
/// computer is offered beside it.
fn connections(target: &Target) -> Vec<Target> {
    match target {
        Target::Local => vec![Target::Local],
        native => vec![native.clone(), Target::Local],
    }
}

/// The environment the window was started in, which it cannot remove.
fn started_in() -> Option<EnvironmentId> {
    adroit_connect::environments::active()
        .ok()
        .flatten()
        .map(|binding| EnvironmentId::new(binding.environment))
}

#[component]
pub(super) fn EnvironmentPane() -> Element {
    let edited = use_context::<Edited>();
    let mut catalog = use_signal(|| Catalog::load(&Catalog::path()));
    let mut error = use_signal(|| None::<String>);
    let mut notice = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut removing = use_signal(|| false);
    // The connection chosen in place of the environment's own, if any.
    let mut chosen = use_signal(|| None::<Target>);
    let kept: Option<Environment> = catalog.read().as_ref().ok().and_then(|c| {
        c.environments
            .iter()
            .find(|e| EnvironmentId::new(&e.id) == edited.environment)
            .cloned()
    });
    let Some(kept) = kept else {
        return rsx! {
            if let Err(why) = &*catalog.read() { SettingsError { "{why}" } }
            p { class: "settings-agent-note",
                "{edited.name} is not kept among this computer's environments, so its name and connection cannot be changed here."
            }
        };
    };
    let shown = chosen.read().clone().unwrap_or_else(|| kept.target.clone());
    let moving = shown != kept.target;
    let removable = started_in().as_ref() != Some(&edited.environment);
    let id = edited.environment.clone();
    rsx! {
        SettingGroup {
            SettingRow { label: "Name", hint: "Every window calls the environment by it.",
                TextField {
                    aria_label: "Environment name",
                    value: kept.name.clone(),
                    onchange: {
                        let id = id.clone();
                        move |name: String| {
                            notice.set(None);
                            error.set(environments::rename(&Catalog::path(), &id, &name).err());
                            catalog.set(Catalog::load(&Catalog::path()));
                        }
                    },
                }
            }
            SettingRow { label: "Connection", hint: "Agents and working files stay on this computer or on the work machine.",
                select {
                    aria_label: "Environment connection",
                    disabled: busy(),
                    onchange: {
                        let choices = connections(&kept.target);
                        move |e: Event<FormData>| {
                            notice.set(None);
                            if let Some(target) = e.value().parse::<usize>().ok().and_then(|i| choices.get(i)) {
                                chosen.set(Some(target.clone()));
                            }
                        }
                    },
                    for (i , target) in connections(&kept.target).into_iter().enumerate() {
                        option { value: "{i}", selected: shown == target, "{connection_name(&target)}" }
                    }
                }
            }
            if matches!(kept.target, Target::Local) {
                SettingRow { label: "Work on a work machine", hint: "Add an environment for it below the main window's tree, which pairs with it.", }
            }
        }
        if moving {
            p { class: "settings-agent-note",
                "Changing the connection adds another environment, on {connection_name(&shown)}, which starts with its own agents and work settings. This one, and the sessions running in it, stay as they are."
            }
        }
        div { class: "settings-page-actions",
            button {
                r#type: "button",
                disabled: busy(),
                onclick: {
                    let target = shown.clone();
                    move |_| {
                    notice.set(None);
                    error.set(None);
                    busy.set(true);
                    let target = target.clone();
                    spawn(async move {
                        let result = adroit_connect::check(&target).await;
                        if chosen.peek().as_ref().is_none_or(|now| *now == target) {
                            match result {
                                Ok(_) => notice.set(Some(format!("Connected to {}.", connection_name(&target)))),
                                Err(why) => error.set(Some(why)),
                            }
                        }
                        busy.set(false);
                    });
                    }
                },
                if busy() { "Checking…" } else { "Test connection" }
            }
            if moving {
                button {
                    r#type: "button",
                    class: "primary",
                    disabled: busy(),
                    onclick: {
                        let name = kept.name.clone();
                        move |_| {
                            let Some(target) = chosen.peek().clone() else {
                                return;
                            };
                            let named = catalog
                                .peek()
                                .as_ref()
                                .map_or_else(|_| name.clone(), |c| fresh_name(c, &name));
                            match environments::add(&Catalog::path(), &named, target) {
                                Ok(added) => {
                                    let _ = environments::show(&added);
                                    chosen.set(None);
                                    catalog.set(Catalog::load(&Catalog::path()));
                                    notice.set(Some(format!("Added {}; every window lists it.", added.name)));
                                }
                                Err(why) => error.set(Some(why)),
                            }
                        }
                    },
                    "Add as new environment"
                }
            }
        }
        if let Some(why) = error() { SettingsError { "{why}" } }
        if let Some(text) = notice() { p { class: "settings-agent-note", role: "status", "{text}" } }
        if removable {
            SettingGroup { title: "Remove",
                SettingRow { label: "Remove {kept.name}", hint: "Every window stops listing it. Its sessions keep running on its host, and it can be restored from Settings.",
                    button { r#type: "button", class: "danger", onclick: move |_| removing.set(true), "Remove…" }
                }
            }
        }
        if removing() {
            div { class: "settings-confirm", role: "alertdialog", aria_label: "Remove {kept.name}",
                p { class: "settings-confirm-head", "Remove {kept.name}?" }
                p { class: "settings-agent-note",
                    "Every window stops listing it. Its sessions keep running on its host, and it can be restored from Settings."
                }
                div { class: "settings-page-actions",
                    button {
                        r#type: "button",
                        class: "danger",
                        onclick: move |_| match environments::archive(&Catalog::path(), &id) {
                            // Nothing is left here to configure.
                            Ok(()) => super::close(),
                            Err(why) => {
                                removing.set(false);
                                error.set(Some(why));
                            }
                        },
                        "Remove"
                    }
                    button { r#type: "button", onclick: move |_| removing.set(false), "Keep" }
                }
            }
        }
    }
}

#[component]
pub(super) fn RemovedPane() -> Element {
    let mut catalog = use_signal(|| Catalog::load(&Catalog::path()));
    let mut error = use_signal(|| None::<String>);
    let removed: Vec<Environment> = catalog
        .read()
        .as_ref()
        .map(|c| {
            c.environments
                .iter()
                .filter(|e| e.archived)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    rsx! {
        if let Err(why) = &*catalog.read() { SettingsError { "{why}" } }
        if let Some(why) = error() { SettingsError { "{why}" } }
        SettingGroup {
            if removed.is_empty() {
                SettingRow { label: "None", hint: "An environment removed from its heading in the tree is kept here, to restore." }
            }
            for environment in removed {
                SettingRow { key: "{environment.id}", label: environment.name.clone(), hint: connection_name(&environment.target),
                    button {
                        r#type: "button",
                        onclick: {
                            let id = EnvironmentId::new(&environment.id);
                            move |_| {
                                error.set(environments::restore(&Catalog::path(), &id).err());
                                catalog.set(Catalog::load(&Catalog::path()));
                            }
                        },
                        "Restore"
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
    fn a_paired_environment_can_be_moved_to_this_computer_and_one_here_nowhere() {
        assert_eq!(connections(&paired()), [paired(), Target::Local]);
        assert_eq!(connections(&Target::Local), [Target::Local]);
    }

    #[test]
    fn a_connection_is_named_by_where_it_reaches() {
        assert_eq!(connection_name(&Target::Local), "This computer");
        assert_eq!(
            connection_name(&paired()),
            "Work machine at work-box.local:47130"
        );
    }
}
