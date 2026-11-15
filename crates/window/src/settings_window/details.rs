//! What the details panel is made of, arranged on a picture of the panel:
//! its tabs, each with the components it holds, and its foot, filled from
//! the components the subject can be given.

use adroit_core::details::{Component, DetailsLayout, Kind, Slot, Subject, Surface};
use dioxus::prelude::*;

use super::Change;
use crate::components::TextField;
use crate::icons::{Icon, IconView};
use crate::settings::Settings;

/// An edit of the surface of the subject the pane is on.
type Edit = Box<dyn FnOnce(&mut Surface)>;

/// The things the panel describes, as the pane offers them.
const SUBJECTS: [(Subject, &str); 3] = [
    (Subject::Session, "A window"),
    (Subject::Environment, "An environment"),
    (Subject::Github, "A pull request or issue"),
];

/// What is being dragged: a component taken from the list, or one placed.
#[derive(Clone, Copy, PartialEq)]
enum Drag {
    New(Component),
    Placed(Slot, usize),
}

#[component]
pub(super) fn DetailsPane(now: Settings, change: Callback<Change>) -> Element {
    let mut subject = use_signal(|| Subject::Session);
    let tab = use_signal(|| 0usize);
    let mut target = use_signal(|| Slot::Tab(0));
    let dragging = use_signal(|| None::<Drag>);
    let mut picking = use_signal(|| false);
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
    let shown = tab().min(tabs.saturating_sub(1));
    let current = surface.tabs[shown].clone();
    let placed: Vec<Component> = surface.placed().map(|p| p.component).collect();
    let target_now = match target() {
        Slot::Tab(_) => Slot::Tab(shown),
        Slot::Foot => Slot::Foot,
    };
    let target_name = match target_now {
        Slot::Tab(_) => format!("the tab “{}”", current.title),
        Slot::Foot => "the foot".to_string(),
    };
    let pick_tab = move |at: usize| {
        let (mut tab, mut target, mut picking) = (tab, target, picking);
        tab.set(at);
        target.set(Slot::Tab(at));
        picking.set(false);
    };
    rsx! {
        div { class: "dl-top",
            div { class: "dl-subjects", role: "tablist", aria_label: "What the panel describes",
                for (s , name) in SUBJECTS {
                    button {
                        key: "{name}",
                        class: if s == subject() { "dl-subject on" } else { "dl-subject" },
                        role: "tab",
                        "aria-selected": s == subject(),
                        onclick: move |_| {
                            subject.set(s);
                            pick_tab(0);
                        },
                        "{name}"
                    }
                }
            }
            button {
                class: "dl-restore",
                r#type: "button",
                title: "Put back the tabs and the foot Adroit comes with for this",
                onclick: move |_| {
                    let shipped = DetailsLayout::default().surface(subject());
                    edit(Box::new(move |s: &mut Surface| *s = shipped));
                    pick_tab(0);
                },
                IconView { icon: Icon::Restore, size: 13 }
                "Restore"
            }
        }
        p { class: "settings-hint dl-lead",
            "Pick a tab, then drag components from the list onto it or onto the foot, which shows under every tab. Drag a placed component to move it."
        }
        div { class: "dl-editor",
            div { class: "dl-mock",
                div { class: "dl-tabs",
                    for (at , t) in surface.tabs.iter().cloned().enumerate() {
                        button {
                            key: "{at}",
                            class: if at == shown { "dl-tab on" } else { "dl-tab" },
                            r#type: "button",
                            title: "{t.title}",
                            onclick: move |_| pick_tab(at),
                            ondragover: move |e: Event<DragData>| e.prevent_default(),
                            ondrop: move |e: Event<DragData>| {
                                e.prevent_default();
                                dropped(dragging, edit, Slot::Tab(at), usize::MAX);
                                pick_tab(at);
                            },
                            match t.icon() {
                                Some(icon) => rsx! { IconView { icon, size: 14 } },
                                None => rsx! { span { class: "dl-tab-letter", "{t.title.chars().next().unwrap_or('?')}" } },
                            }
                            if at == shown {
                                span { class: "dl-tab-label", "{t.title}" }
                            }
                        }
                    }
                    button {
                        class: "dl-tab dl-tab-add",
                        r#type: "button",
                        title: "Add a tab",
                        onclick: move |_| {
                            edit(Box::new(|s: &mut Surface| s.add_tab("New tab")));
                            pick_tab(tabs);
                        },
                        IconView { icon: Icon::Plus, size: 14 }
                    }
                }
                div { class: "dl-tab-head",
                    button {
                        class: if picking() { "dl-icon-pick on" } else { "dl-icon-pick" },
                        r#type: "button",
                        title: "Choose the tab's icon",
                        "aria-expanded": picking(),
                        onclick: move |_| picking.toggle(),
                        match current.icon() {
                            Some(icon) => rsx! { IconView { icon, size: 16 } },
                            None => rsx! { span { class: "dl-tab-letter", "–" } },
                        }
                        IconView { icon: Icon::ChevronDown, size: 11 }
                    }
                    div { class: "dl-tab-name",
                        TextField {
                            key: "{subject():?}-{shown}",
                            value: "{current.title}",
                            aria_label: "Name of the tab",
                            onchange: move |title: String| {
                                edit(Box::new(move |s: &mut Surface| {
                                    if let Some(t) = s.tabs.get_mut(shown) {
                                        t.title = title;
                                    }
                                }));
                            },
                        }
                    }
                    button { class: "dl-tool", r#type: "button", disabled: shown == 0, title: "Move this tab left",
                        onclick: move |_| {
                            edit(Box::new(move |s: &mut Surface| s.move_tab(shown, -1)));
                            pick_tab(shown.saturating_sub(1));
                        },
                        IconView { icon: Icon::ChevronLeft, size: 13 }
                    }
                    button { class: "dl-tool", r#type: "button", disabled: shown + 1 == tabs, title: "Move this tab right",
                        onclick: move |_| {
                            edit(Box::new(move |s: &mut Surface| s.move_tab(shown, 1)));
                            pick_tab(shown + 1);
                        },
                        IconView { icon: Icon::ChevronRight, size: 13 }
                    }
                    button { class: "dl-tool danger", r#type: "button", disabled: tabs == 1, title: "Remove this tab",
                        onclick: move |_| {
                            edit(Box::new(move |s: &mut Surface| s.remove_tab(shown)));
                            pick_tab(shown.saturating_sub(1));
                        },
                        IconView { icon: Icon::X, size: 13 }
                    }
                }
                if picking() {
                    div { class: "dl-icons", role: "listbox", aria_label: "Icons",
                        button {
                            class: if current.icon.is_none() { "dl-icon on" } else { "dl-icon" },
                            r#type: "button",
                            title: "No icon",
                            onclick: move |_| {
                                edit(Box::new(move |s: &mut Surface| {
                                    if let Some(t) = s.tabs.get_mut(shown) {
                                        t.icon = None;
                                    }
                                }));
                                picking.set(false);
                            },
                            span { class: "dl-tab-letter", "–" }
                        }
                        for icon in Icon::ALL.iter().copied() {
                            button {
                                key: "{icon.name()}",
                                class: if current.icon() == Some(icon) { "dl-icon on" } else { "dl-icon" },
                                r#type: "button",
                                title: "{icon.name()}",
                                onclick: move |_| {
                                    edit(Box::new(move |s: &mut Surface| {
                                        if let Some(t) = s.tabs.get_mut(shown) {
                                            t.icon = Some(icon.name().to_string());
                                        }
                                    }));
                                    picking.set(false);
                                },
                                IconView { icon, size: 16 }
                            }
                        }
                    }
                }
                Area {
                    slot: Slot::Tab(shown),
                    placed: current.components.iter().map(|p| p.component).collect::<Vec<_>>(),
                    targeted: target_now == Slot::Tab(shown),
                    dragging,
                    edit,
                    on_target: move |_| target.set(Slot::Tab(shown)),
                }
                div { class: "dl-foot-label",
                    span { "Foot" }
                    span { class: "settings-hint", "under every tab" }
                }
                Area {
                    slot: Slot::Foot,
                    placed: surface.foot.iter().map(|p| p.component).collect::<Vec<_>>(),
                    targeted: target_now == Slot::Foot,
                    dragging,
                    edit,
                    on_target: move |_| target.set(Slot::Foot),
                }
            }
            div { class: "dl-palette",
                div { class: "dl-palette-head",
                    span { class: "dl-palette-title", "Components" }
                    span { class: "settings-hint", "+ adds to {target_name}" }
                }
                for kind in Kind::ALL.iter().copied() {
                    {
                        let offered: Vec<Component> = Component::placeable(subject()).filter(|c| c.kind() == kind).collect();
                        rsx! {
                            if !offered.is_empty() {
                                div { key: "{kind.title()}", class: "dl-kind",
                                    div { class: "dl-kind-title", "{kind.title()}" }
                                    for c in offered {
                                        Card { key: "{c.name()}", component: c, placed: placed.contains(&c), dragging,
                                            on_add: move |_| edit(Box::new(move |s: &mut Surface| s.add(target_now, c))),
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What is dragged put into `slot` before what is at `at` there.
fn dropped(mut dragging: Signal<Option<Drag>>, edit: Callback<Edit>, slot: Slot, at: usize) {
    match dragging.take() {
        Some(Drag::New(c)) => edit(Box::new(move |s: &mut Surface| s.insert(slot, at, c))),
        Some(Drag::Placed(from, i)) => {
            edit(Box::new(move |s: &mut Surface| s.carry(from, i, slot, at)));
        }
        None => {}
    }
}

/// A tab's body or the foot on the picture: the components `slot` places,
/// each drawn as a sketch of what it shows, to drag, move or take out.
#[component]
fn Area(
    slot: Slot,
    placed: Vec<Component>,
    targeted: bool,
    dragging: Signal<Option<Drag>>,
    edit: Callback<Edit>,
    on_target: EventHandler<()>,
) -> Element {
    let mut over = use_signal(|| None::<usize>);
    let last = placed.len();
    let mut class = String::from("dl-area");
    if targeted {
        class.push_str(" target");
    }
    if matches!(slot, Slot::Foot) {
        class.push_str(" foot");
    }
    if dragging().is_some() && over() == Some(last) {
        class.push_str(" over");
    }
    rsx! {
        div {
            class: "{class}",
            onclick: move |_| on_target.call(()),
            ondragover: move |e: Event<DragData>| {
                e.prevent_default();
                if over() != Some(last) {
                    over.set(Some(last));
                }
            },
            ondragleave: move |_| over.set(None),
            ondrop: move |e: Event<DragData>| {
                e.prevent_default();
                over.set(None);
                dropped(dragging, edit, slot, usize::MAX);
                on_target.call(());
            },
            for (at , c) in placed.into_iter().enumerate() {
                div {
                    key: "{at}-{c.name()}",
                    class: if dragging().is_some() && over() == Some(at) { "dl-block over" } else { "dl-block" },
                    draggable: true,
                    title: "{c.about()}",
                    ondragstart: move |_| {
                        let mut dragging = dragging;
                        dragging.set(Some(Drag::Placed(slot, at)));
                    },
                    ondragend: move |_| {
                        let mut dragging = dragging;
                        dragging.set(None);
                        over.set(None);
                    },
                    ondragover: move |e: Event<DragData>| {
                        e.prevent_default();
                        e.stop_propagation();
                        if over() != Some(at) {
                            over.set(Some(at));
                        }
                    },
                    ondrop: move |e: Event<DragData>| {
                        e.prevent_default();
                        e.stop_propagation();
                        over.set(None);
                        dropped(dragging, edit, slot, at);
                        on_target.call(());
                    },
                    div { class: "dl-block-head",
                        IconView { icon: c.icon(), size: 13 }
                        span { class: "dl-block-title", "{c.title()}" }
                        span { class: "dl-block-tools",
                            button { class: "dl-tool flip", r#type: "button", disabled: at == 0, title: "Move {c.title()} up",
                                onclick: move |e| {
                                    e.stop_propagation();
                                    edit(Box::new(move |s: &mut Surface| s.shift(slot, at, -1)));
                                },
                                IconView { icon: Icon::ChevronDown, size: 11 }
                            }
                            button { class: "dl-tool", r#type: "button", disabled: at + 1 == last, title: "Move {c.title()} down",
                                onclick: move |e| {
                                    e.stop_propagation();
                                    edit(Box::new(move |s: &mut Surface| s.shift(slot, at, 1)));
                                },
                                IconView { icon: Icon::ChevronDown, size: 11 }
                            }
                            button { class: "dl-tool danger", r#type: "button", title: "Take {c.title()} out",
                                onclick: move |e| {
                                    e.stop_propagation();
                                    edit(Box::new(move |s: &mut Surface| s.remove(slot, at)));
                                },
                                IconView { icon: Icon::X, size: 11 }
                            }
                        }
                    }
                    Sketch { component: c }
                }
            }
            if last == 0 {
                div { class: "dl-empty",
                    if matches!(slot, Slot::Foot) { "Nothing under the tabs. Drop components here." } else { "This tab is empty. Drop components here." }
                }
            }
        }
    }
}

/// A picture of what `component` shows, in grey.
#[component]
fn Sketch(component: Component) -> Element {
    if component.meter().is_some() {
        return rsx! {
            div { class: "dl-sketch meter {component.name()}",
                div { class: "dl-bar", div { class: "dl-fill" } }
            }
        };
    }
    if component.gauge().is_some() {
        return rsx! {
            div { class: "dl-sketch gauge {component.name()}",
                div { class: "dl-bar", div { class: "dl-fill" } }
                div { class: "dl-line short" }
            }
        };
    }
    let lines = match component {
        Component::Session | Component::Change | Component::Progress => 1,
        Component::Environment | Component::Github | Component::Changes => 3,
        Component::Log | Component::Timeline => 4,
        _ => 2,
    };
    rsx! {
        div { class: "dl-sketch rows",
            for i in 0..lines {
                div { key: "{i}", class: if i % 2 == 1 { "dl-line short" } else { "dl-line" } }
            }
        }
    }
}

/// A component in the list: what it is and what it shows, to drag onto the
/// picture or add where the picture is pointed at.
#[component]
fn Card(
    component: Component,
    placed: bool,
    dragging: Signal<Option<Drag>>,
    on_add: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            class: if placed { "dl-card placed" } else { "dl-card" },
            draggable: true,
            ondragstart: move |_| {
                let mut dragging = dragging;
                dragging.set(Some(Drag::New(component)));
            },
            ondragend: move |_| {
                let mut dragging = dragging;
                dragging.set(None);
            },
            span { class: "dl-card-icon", IconView { icon: component.icon(), size: 15 } }
            div { class: "dl-card-text",
                div { class: "dl-card-title",
                    "{component.title()}"
                    if placed {
                        span { class: "dl-card-mark", title: "Already placed", IconView { icon: Icon::Check, size: 11 } }
                    }
                }
                div { class: "dl-card-about", "{component.about()}" }
            }
            button { class: "dl-tool dl-card-add", r#type: "button", title: "Add {component.title()}",
                onclick: move |_| on_add.call(()),
                IconView { icon: Icon::Plus, size: 13 }
            }
        }
    }
}
