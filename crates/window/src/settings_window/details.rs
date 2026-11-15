//! What the details panel is made of: for each thing it describes, its tabs
//! with the components of each, and its foot.

use adroit_core::details::{Component, Slot, Subject, Surface};
use dioxus::prelude::*;

use super::Change;
use crate::components::{SettingGroup, SettingRow, TextField};
use crate::icons::{Icon, IconView};
use crate::settings::Settings;

/// An edit of the surface of the subject the pane is on.
type Edit = Box<dyn FnOnce(&mut Surface)>;

/// The things the panel describes, as the pane offers them.
const SUBJECTS: [(Subject, &str); 3] = [
    (Subject::Session, "What a window shows"),
    (Subject::Environment, "An environment's heading"),
    (Subject::Github, "A pull request or issue"),
];

#[component]
pub(super) fn DetailsPane(now: Settings, change: Callback<Change>) -> Element {
    let mut subject = use_signal(|| Subject::Session);
    let surface = now.panel.surface(subject());
    let edit = use_callback(move |f: Edit| {
        let at = subject();
        change(Box::new(move |s: &mut Settings| {
            let mut surface = s.panel.surface(at);
            f(&mut surface);
            s.panel.set(at, surface);
        }));
    });
    let tabs = surface.tabs.len();
    rsx! {
        SettingGroup {
            SettingRow { label: "Arrange", hint: "Each thing the panel describes has tabs of its own, and a foot shown under every tab.",
                select {
                    aria_label: "What the panel describes",
                    onchange: move |e| {
                        if let Some((s, _)) = SUBJECTS.iter().find(|(_, name)| *name == e.value()) {
                            subject.set(*s);
                        }
                    },
                    for (s , name) in SUBJECTS {
                        option { key: "{name}", value: "{name}", selected: s == subject(), "{name}" }
                    }
                }
            }
            SettingRow { label: "Adroit's arrangement", hint: "Put back the tabs and the foot Adroit comes with for this.",
                button {
                    class: "button",
                    r#type: "button",
                    onclick: move |_| {
                        let shipped = adroit_core::details::DetailsLayout::default().surface(subject());
                        edit(Box::new(move |s: &mut Surface| *s = shipped));
                    },
                    "Restore"
                }
            }
        }
        for (at , tab) in surface.tabs.into_iter().enumerate() {
            SettingGroup { key: "{at}", title: "Tab {at + 1}",
                SettingRow { label: "Name", hint: "",
                    TextField {
                        value: "{tab.title}",
                        aria_label: "Name of tab {at + 1}",
                        onchange: move |title: String| {
                            edit(Box::new(move |s: &mut Surface| {
                                if let Some(t) = s.tabs.get_mut(at) {
                                    t.title = title;
                                }
                            }));
                        },
                    }
                }
                SettingRow { label: "Icon", hint: "",
                    select {
                        aria_label: "Icon of tab {at + 1}",
                        onchange: move |e| {
                            let name = e.value();
                            edit(Box::new(move |s: &mut Surface| {
                                if let Some(t) = s.tabs.get_mut(at) {
                                    t.icon = (!name.is_empty()).then_some(name);
                                }
                            }));
                        },
                        option { value: "", selected: tab.icon.is_none(), "None" }
                        for icon in Icon::ALL.iter() {
                            option { key: "{icon.name()}", value: "{icon.name()}", selected: tab.icon.as_deref() == Some(icon.name()), "{icon.name()}" }
                        }
                    }
                }
                Placements { subject: subject(), slot: Slot::Tab(at), placed: tab.components.iter().map(|p| p.component).collect::<Vec<_>>(), edit }
                SettingRow { label: "This tab", hint: "",
                    div { class: "settings-buttons",
                        button { class: "button", r#type: "button", disabled: at == 0, title: "Move this tab left",
                            onclick: move |_| edit(Box::new(move |s: &mut Surface| s.move_tab(at, -1))),
                            "Move left"
                        }
                        button { class: "button", r#type: "button", disabled: at + 1 == tabs, title: "Move this tab right",
                            onclick: move |_| edit(Box::new(move |s: &mut Surface| s.move_tab(at, 1))),
                            "Move right"
                        }
                        button { class: "button", r#type: "button", disabled: tabs == 1, title: "Remove this tab",
                            onclick: move |_| edit(Box::new(move |s: &mut Surface| s.remove_tab(at))),
                            "Remove"
                        }
                    }
                }
            }
        }
        SettingGroup {
            SettingRow { label: "New tab", hint: "A tab of its own, holding nothing until components are added.",
                button {
                    class: "button",
                    r#type: "button",
                    onclick: move |_| edit(Box::new(|s: &mut Surface| s.add_tab("New tab"))),
                    IconView { icon: Icon::Plus, size: 13 }
                    "Add a tab"
                }
            }
        }
        SettingGroup { title: "Foot",
            Placements { subject: subject(), slot: Slot::Foot, placed: surface.foot.iter().map(|p| p.component).collect::<Vec<_>>(), edit }
        }
    }
}

/// The components `slot` places, each to move or take out, and a choice of
/// those `subject` can be given to add.
#[component]
fn Placements(
    subject: Subject,
    slot: Slot,
    placed: Vec<Component>,
    edit: Callback<Edit>,
) -> Element {
    let last = placed.len();
    rsx! {
        for (at , c) in placed.into_iter().enumerate() {
            SettingRow { key: "{at}-{c.name()}", label: "{c.title()}", hint: "",
                div { class: "settings-buttons",
                    button { class: "button", r#type: "button", disabled: at == 0, title: "Move {c.title()} up",
                        onclick: move |_| edit(Box::new(move |s: &mut Surface| s.shift(slot, at, -1))),
                        "Up"
                    }
                    button { class: "button", r#type: "button", disabled: at + 1 == last, title: "Move {c.title()} down",
                        onclick: move |_| edit(Box::new(move |s: &mut Surface| s.shift(slot, at, 1))),
                        "Down"
                    }
                    button { class: "button", r#type: "button", title: "Take {c.title()} out",
                        onclick: move |_| edit(Box::new(move |s: &mut Surface| s.remove(slot, at))),
                        IconView { icon: Icon::X, size: 12 }
                    }
                }
            }
        }
        SettingRow { label: "Add", hint: "",
            select {
                aria_label: "Add a component",
                onchange: move |e| {
                    if let Some(c) = Component::ALL.iter().copied().find(|c| c.name() == e.value()) {
                        edit(Box::new(move |s: &mut Surface| s.add(slot, c)));
                    }
                },
                option { value: "", selected: true, "Add a component…" }
                for c in Component::placeable(subject) {
                    option { key: "{c.name()}", value: "{c.name()}", "{c.title()}" }
                }
            }
        }
    }
}
