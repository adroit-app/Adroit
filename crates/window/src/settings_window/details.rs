//! What the details panel is made of, arranged on a picture of the panel
//! drawn as the panel draws itself: its tabs, each with the components it
//! holds, and its foot, filled by dragging the components the subject can be
//! given onto it.

use adroit_core::details::{
    Component, DetailsLayout, Kind, Part, Placed, Slot, Subject, Surface, Tone, runs,
};
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;

use super::Change;
use super::specimen::Specimen;
use crate::components::{NumberField, TextField, Toggle};
use crate::icons::{Icon, IconView};
use crate::settings::Settings;

/// An edit of the surface of the subject the pane is on.
type Edit = Box<dyn FnOnce(&mut Surface)>;

/// The things the panel describes, as the pane offers them: what each is
/// called, and when the panel describes it.
const SUBJECTS: [(Subject, &str, &str); 3] = [
    (
        Subject::Session,
        "Session or file",
        "Shown for the window in focus: a session, a shell, a draft or a file.",
    ),
    (
        Subject::Environment,
        "Environment",
        "Shown while an environment's name is selected in the tree.",
    ),
    (
        Subject::Github,
        "Pull request or issue",
        "Shown while a pull request or an issue is selected in the tree.",
    ),
];

/// What is being dragged: a component taken from the list, or one placed.
#[derive(Clone, Copy, PartialEq)]
enum Drag {
    New(Component),
    Placed(Slot, usize),
}

impl Drag {
    fn component(self, surface: &Surface) -> Option<Component> {
        match self {
            Drag::New(c) => Some(c),
            Drag::Placed(slot, at) => surface
                .components(slot)
                .and_then(|l| l.get(at))
                .map(|p| p.component),
        }
    }
}

/// A drag in progress, by the pointer: HTML's own dragging is not relied on,
/// as the webview hands a drop to the page only when told synchronously that
/// it is wanted, which the window's event handlers are not.
#[derive(Clone, Copy, PartialEq)]
struct Dnd {
    /// What was pressed on and where, until the pointer moves far enough
    /// to drag it: a click stays a click.
    pressed: Signal<Option<(Drag, f64, f64)>>,
    grab: Signal<Option<Drag>>,
    /// Where it would be put: before what is at the place in the slot, and
    /// whether beside the one before it.
    over: Signal<Option<(Slot, usize, bool)>>,
    pointer: Signal<(f64, f64)>,
}

impl Dnd {
    fn active(self) -> bool {
        (self.grab)().is_some()
    }

    fn start(self, drag: Drag, e: &MouseEvent) {
        if e.trigger_button() == Some(MouseButton::Primary) {
            let mut pressed = self.pressed;
            let p = e.client_coordinates();
            pressed.set(Some((drag, p.x, p.y)));
        }
    }

    /// The pointer moved to `(x, y)`: what was pressed on is dragged once it
    /// is a few pixels away.
    fn moved(self, x: f64, y: f64) {
        let (mut grab, mut pointer) = (self.grab, self.pointer);
        if let Some((drag, x0, y0)) = (self.pressed)()
            && !self.active()
            && (x - x0).hypot(y - y0) > 4.0
        {
            grab.set(Some(drag));
        }
        if self.active() {
            pointer.set((x, y));
        }
    }

    fn hover(self, slot: Slot, at: usize, beside: bool) {
        let mut over = self.over;
        if self.active() && over() != Some((slot, at, beside)) {
            over.set(Some((slot, at, beside)));
        }
    }

    fn end(self) {
        let (mut pressed, mut grab, mut over) = (self.pressed, self.grab, self.over);
        pressed.set(None);
        grab.set(None);
        over.set(None);
    }
}

#[component]
pub(super) fn DetailsPane(now: Settings, change: Callback<Change>) -> Element {
    let mut subject = use_signal(|| Subject::Session);
    let tab = use_signal(|| 0usize);
    let chosen = use_signal(|| None::<(Slot, usize)>);
    let dnd = Dnd {
        pressed: use_signal(|| None),
        grab: use_signal(|| None),
        over: use_signal(|| None),
        pointer: use_signal(|| (0.0, 0.0)),
    };
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
    let pick_tab = move |at: usize| {
        let (mut tab, mut chosen) = (tab, chosen);
        tab.set(at);
        chosen.set(None);
    };
    let ghost = (dnd.grab)().and_then(|d| d.component(&surface));
    let (x, y) = (dnd.pointer)();
    let option = chosen().and_then(|(slot, at)| {
        let p = surface.components(slot)?.get(at).cloned()?;
        // A choice of the shown tab's, or of the foot; another tab's is gone.
        (slot == Slot::Foot || slot == Slot::Tab(shown)).then_some((slot, at, p))
    });
    let when = SUBJECTS
        .iter()
        .find(|(s, ..)| *s == subject())
        .map_or("", |(.., when)| when);
    rsx! {
        div {
            class: if dnd.active() { "dl-pane dragging" } else { "dl-pane" },
            onmousemove: move |e| {
                if (dnd.pressed)().is_some() {
                    let p = e.client_coordinates();
                    dnd.moved(p.x, p.y);
                }
            },
            onmouseup: move |_| {
                if let (Some(drag), Some((slot, at, beside))) = ((dnd.grab)(), (dnd.over)()) {
                    let put = match drag {
                        Drag::Placed(from, i) if from == slot && at > i => at - 1,
                        _ => at,
                    };
                    edit(Box::new(move |s: &mut Surface| {
                        match drag {
                            Drag::New(c) => s.insert(slot, at, c),
                            Drag::Placed(from, i) => s.carry(from, i, slot, at),
                        }
                        s.set_beside(slot, put, beside);
                    }));
                    let mut chosen = chosen;
                    chosen.set(Some((slot, put)));
                }
                dnd.end();
            },
            onmouseleave: move |_| dnd.end(),
            div { class: "dl-top",
                div { class: "dl-subjects", role: "tablist", aria_label: "Which panel to arrange",
                    for (s , name , _) in SUBJECTS {
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
                    title: "Put back the tabs and the foot Adroit comes with for this panel",
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
                "{when} Drag components from the list onto a tab or onto the foot, which shows under every tab; drag one placed to move it, or onto the right of another to put it beside that one. Click one to set how it shows."
            }
            div { class: "dl-editor",
                div { class: "dl-preview",
                    div { class: "app-panel details",
                        if tabs > 1 {
                            div { class: "panel-tabs",
                                for (at , t) in surface.tabs.iter().cloned().enumerate() {
                                    button {
                                        key: "{at}",
                                        class: match (at == shown, t.icon().is_some()) {
                                            (true, true) => "panel-tab active",
                                            (true, false) => "panel-tab active no-icon",
                                            (false, true) => "panel-tab",
                                            (false, false) => "panel-tab no-icon",
                                        },
                                        r#type: "button",
                                        title: "{t.title}",
                                        onclick: move |_| pick_tab(at),
                                        onmouseenter: move |_| {
                                            if dnd.active() {
                                                pick_tab(at);
                                            }
                                        },
                                        if let Some(icon) = t.icon() {
                                            IconView { icon, size: 13 }
                                        }
                                        span { class: "panel-tab-label", "{t.title}" }
                                    }
                                }
                            }
                        }
                        Area { slot: Slot::Tab(shown), placed: current.components.clone(), dnd, edit, chosen }
                        Area { slot: Slot::Foot, placed: surface.foot.clone(), dnd, edit, chosen }
                    }
                }
                div { class: "dl-side",
                    div { class: "dl-card-box",
                        div { class: "dl-box-head",
                            span { class: "dl-box-title", "Tab {shown + 1} of {tabs}" }
                            span { class: "dl-box-tools",
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
                                button { class: "dl-tool", r#type: "button", title: "Add a tab after the others",
                                    onclick: move |_| {
                                        edit(Box::new(|s: &mut Surface| s.add_tab("New tab")));
                                        pick_tab(tabs);
                                    },
                                    IconView { icon: Icon::Plus, size: 13 }
                                }
                            }
                        }
                        div { class: "dl-tab-fields",
                            IconPicker {
                                current: current.icon(),
                                title: "Choose the tab's icon",
                                none: "No icon: the tab shows its name",
                                on_pick: move |icon: Option<Icon>| {
                                    edit(Box::new(move |s: &mut Surface| {
                                        if let Some(t) = s.tabs.get_mut(shown) {
                                            t.icon = icon.map(|i| i.name().to_string());
                                        }
                                    }));
                                },
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
                        }
                    }
                    if let Some((slot, at, p)) = option {
                        Options { key: "{slot:?}-{at}", slot, at, placed: p, edit, chosen }
                    } else {
                        div { class: "dl-card-box dl-placeholder",
                            IconView { icon: Icon::AdjustmentsHorizontal, size: 14 }
                            span { "Click a component in the panel to set how it shows: folded or open, how many rows, beside another." }
                        }
                    }
                    div { class: "dl-palette",
                        div { class: "dl-box-title", "Components" }
                        div { class: "dl-palette-list",
                            for kind in Kind::ALL.iter().copied() {
                                {
                                    let offered: Vec<Component> = Component::placeable(subject()).filter(|c| c.kind() == kind).collect();
                                    rsx! {
                                        if !offered.is_empty() {
                                            div { key: "{kind.title()}", class: "dl-kind",
                                                div { class: "dl-kind-title", "{kind.title()}" }
                                                for c in offered {
                                                    Card { key: "{c.name()}", component: c, placed: placed.contains(&c), dnd }
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
            if let Some(c) = ghost {
                div { class: "dl-ghost", style: "left:{x + 14.0}px;top:{y + 10.0}px",
                    IconView { icon: c.icon(), size: 13 }
                    "{c.title()}"
                }
            }
        }
    }
}

/// How the component chosen on the picture shows where it is placed.
#[component]
fn Options(
    slot: Slot,
    at: usize,
    placed: Placed,
    edit: Callback<Edit>,
    chosen: Signal<Option<(Slot, usize)>>,
) -> Element {
    let c = placed.component;
    let change = move |f: Box<dyn FnOnce(&mut Placed)>| {
        edit(Box::new(move |s: &mut Surface| {
            if let Some(p) = s.components_mut(slot).and_then(|l| l.get_mut(at)) {
                f(p);
            }
        }));
    };
    rsx! {
        div { class: "dl-card-box",
            div { class: "dl-box-head",
                IconView { icon: c.icon(), size: 14 }
                span { class: "dl-box-title", "{c.title()}" }
                span { class: "dl-box-tools",
                    button { class: "dl-tool danger", r#type: "button", title: "Take {c.title()} out",
                        onclick: move |_| {
                            edit(Box::new(move |s: &mut Surface| s.remove(slot, at)));
                            let mut chosen = chosen;
                            chosen.set(None);
                        },
                        IconView { icon: Icon::X, size: 13 }
                    }
                }
            }
            div { class: "settings-hint dl-about", "{c.about()}" }
            if c.is_button() {
                div { class: "dl-option",
                    span { "Colour" }
                    div { class: "dl-tones",
                        for tone in Tone::ALL {
                            button {
                                key: "{tone.name()}",
                                class: if placed.tone() == tone { "dl-tone on" } else { "dl-tone" },
                                r#type: "button",
                                onclick: move |_| change(Box::new(move |p: &mut Placed| p.tone = Some(tone))),
                                "{tone.title()}"
                            }
                        }
                    }
                }
                div { class: "dl-option",
                    span { "Label" }
                    TextField {
                        key: "{slot:?}-{at}-label",
                        value: placed.title.clone().unwrap_or_default(),
                        placeholder: c.label().to_string(),
                        aria_label: "The button's label",
                        onchange: move |text: String| {
                            let label = (!text.trim().is_empty()).then(|| text.trim().to_string());
                            change(Box::new(move |p: &mut Placed| p.title = label));
                        },
                    }
                }
                div { class: "dl-option",
                    span { "Icon" }
                    IconPicker {
                        current: placed.button_icon(),
                        title: "Choose the button's icon",
                        none: "No icon",
                        on_pick: move |icon: Option<Icon>| {
                            let name = icon.map_or_else(String::new, |i| i.name().to_string());
                            change(Box::new(move |p: &mut Placed| p.icon = Some(name)));
                        },
                    }
                }
                div { class: "dl-option",
                    span { "Icon only" }
                    Toggle {
                        checked: placed.icon_only,
                        onchange: move |on: bool| change(Box::new(move |p: &mut Placed| p.icon_only = on)),
                    }
                }
            }
            if c == Component::Section {
                div { class: "dl-option",
                    span { "Says" }
                    TextField {
                        key: "{slot:?}-{at}-title",
                        value: placed.title.clone().unwrap_or_default(),
                        aria_label: "What the heading says",
                        onchange: move |text: String| {
                            let title = (!text.trim().is_empty()).then(|| text.trim().to_string());
                            change(Box::new(move |p: &mut Placed| p.title = title));
                        },
                    }
                }
            }
            if c.folds() {
                div { class: "dl-option",
                    span { "Starts folded" }
                    Toggle {
                        checked: placed.folded == Some(true),
                        onchange: move |on: bool| change(Box::new(move |p: &mut Placed| p.folded = on.then_some(true))),
                    }
                }
            }
            if c.pages() {
                div { class: "dl-option",
                    span { if c == Component::Timeline { "Trees before Show all" } else { "Rows before Show all" } }
                    NumberField {
                        min: "1",
                        max: "99",
                        value: placed.shown.map(|n| n.to_string()).unwrap_or_default(),
                        onchange: move |text: String| {
                            let n = text.trim().parse::<usize>().ok().filter(|n| *n > 0);
                            change(Box::new(move |p: &mut Placed| p.shown = n));
                        },
                    }
                }
            }
            if at > 0 {
                div { class: "dl-option",
                    span { "Beside the one before it" }
                    Toggle {
                        checked: placed.beside,
                        onchange: move |on: bool| edit(Box::new(move |s: &mut Surface| s.set_beside(slot, at, on))),
                    }
                }
            }
        }
    }
}

/// A tab's body or the foot, as the panel draws it: the components `slot`
/// places, in lines and in the blocks the panel draws them in, each to grab
/// and move, with a line where a dragged one would go.
#[component]
fn Area(
    slot: Slot,
    placed: Vec<Placed>,
    dnd: Dnd,
    edit: Callback<Edit>,
    chosen: Signal<Option<(Slot, usize)>>,
) -> Element {
    let last = placed.len();
    let foot = matches!(slot, Slot::Foot);
    let parts: Vec<Part> = placed
        .into_iter()
        .enumerate()
        .map(|(at, placed)| Part {
            placed,
            place: at,
            at,
            rows: Vec::new(),
            foot,
            said: None,
        })
        .collect();
    let lines = move |run: Vec<Vec<Part>>, kind: &'static str| {
        rsx! {
            for line in run {
                div {
                    key: "{line[0].at}",
                    class: "{kind}",
                    style: "--beside:{line.len()}",
                    for part in line.iter().cloned() {
                        SlotView { key: "{part.at}-{part.placed.component.name()}", slot, part, dnd, edit, chosen }
                    }
                }
            }
        }
    };
    rsx! {
        div {
            class: if foot { "details-foot dl-area" } else { "details-scroll dl-area" },
            if foot {
                div { class: "dl-foot-mark", "Foot · under every tab" }
            }
            for run in runs(parts) {
                {
                    let key = format!("run-{}", run[0][0].at);
                    let all = |f: fn(Component) -> bool| run.iter().flatten().all(|p| f(p.placed.component));
                    if all(|c| c.meter().is_some()) {
                        rsx! {
                            div { key: "{key}", class: "details-system-usage", {lines(run, "system-usage-grid")} }
                        }
                    } else if all(|c| c.gauge().is_some()) {
                        rsx! {
                            div { key: "{key}", class: "details-usage",
                                {lines(run, "usage-gauge-line")}
                            }
                        }
                    } else if run[0].len() > 1 {
                        rsx! { div { key: "{key}", {lines(run, "details-line beside")} } }
                    } else {
                        let part = run[0][0].clone();
                        rsx! { SlotView { key: "{key}", slot, part, dnd, edit, chosen } }
                    }
                }
            }
            if (dnd.over)() == Some((slot, last, false)) {
                div { class: "dl-drop-line" }
            }
            div {
                class: if last == 0 { "dl-rest empty" } else { "dl-rest" },
                onmouseenter: move |_| dnd.hover(slot, last, false),
                if last == 0 {
                    if foot { "Nothing under the tabs. Drop components here." } else { "This tab is empty. Drop components here." }
                }
            }
        }
    }
}

/// A component placed, drawn as the panel draws it, to grab and move, or to
/// click and set how it shows.
#[component]
fn SlotView(
    slot: Slot,
    part: Part,
    dnd: Dnd,
    edit: Callback<Edit>,
    chosen: Signal<Option<(Slot, usize)>>,
) -> Element {
    let at = part.at;
    let c = part.placed.component;
    let over = (dnd.over)();
    let mut class = String::from("dl-slot");
    if (dnd.grab)() == Some(Drag::Placed(slot, at)) {
        class.push_str(" lifted");
    }
    if chosen() == Some((slot, at)) {
        class.push_str(" chosen");
    }
    if over == Some((slot, at, false)) {
        class.push_str(" drop-above");
    }
    if over == Some((slot, at + 1, true)) {
        class.push_str(" drop-beside");
    }
    rsx! {
        div {
            class: "{class}",
            title: "{c.title()}: {c.about()}",
            onmousedown: move |e| dnd.start(Drag::Placed(slot, at), &e),
            onclick: move |_| {
                let mut chosen = chosen;
                chosen.set(Some((slot, at)));
            },
            Specimen { placed: part.placed.clone() }
            div { class: "dl-slot-bar",
                IconView { icon: c.icon(), size: 11 }
                span { class: "dl-slot-name", "{c.title()}" }
                button { class: "dl-tool", r#type: "button", title: "Set how {c.title()} shows",
                    onmousedown: move |e| e.stop_propagation(),
                    onclick: move |e| {
                        e.stop_propagation();
                        let mut chosen = chosen;
                        chosen.set(Some((slot, at)));
                    },
                    IconView { icon: Icon::AdjustmentsHorizontal, size: 11 }
                }
                button { class: "dl-tool danger", r#type: "button", title: "Take {c.title()} out",
                    onmousedown: move |e| e.stop_propagation(),
                    onclick: move |e| {
                        e.stop_propagation();
                        edit(Box::new(move |s: &mut Surface| s.remove(slot, at)));
                        let mut chosen = chosen;
                        chosen.set(None);
                    },
                    IconView { icon: Icon::X, size: 11 }
                }
            }
            if dnd.active() {
                div { class: "dl-zone above", onmouseenter: move |_| dnd.hover(slot, at, false) }
                div { class: "dl-zone below", onmouseenter: move |_| dnd.hover(slot, at + 1, false) }
                div { class: "dl-zone beside", onmouseenter: move |_| dnd.hover(slot, at + 1, true) }
            }
        }
    }
}

/// A component in the list: what it is and what it shows, to drag onto the
/// picture.
#[component]
fn Card(component: Component, placed: bool, dnd: Dnd) -> Element {
    rsx! {
        div {
            class: if placed { "dl-card placed" } else { "dl-card" },
            title: "Drag onto the panel",
            onmousedown: move |e| dnd.start(Drag::New(component), &e),
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
        }
    }
}

/// A button showing the icon `current` that opens, over what is below it,
/// the icons to pick from, filtered by name; `none` is the tip of the choice
/// of no icon.
#[component]
fn IconPicker(
    current: Option<Icon>,
    title: String,
    none: String,
    on_pick: EventHandler<Option<Icon>>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut filter = use_signal(String::new);
    let mut pick = move |icon: Option<Icon>| {
        on_pick.call(icon);
        open.set(false);
    };
    rsx! {
        span { class: "dl-icon-picker",
            button {
                class: if open() { "dl-icon-pick on" } else { "dl-icon-pick" },
                r#type: "button",
                title: "{title}",
                "aria-expanded": open(),
                onclick: move |_| {
                    filter.set(String::new());
                    open.toggle();
                },
                match current {
                    Some(icon) => rsx! { IconView { icon, size: 15 } },
                    None => rsx! { span { class: "dl-no-icon", "–" } },
                }
                IconView { icon: Icon::ChevronDown, size: 11 }
            }
            if open() {
                div { class: "dl-scrim", onclick: move |_| open.set(false) }
                div { class: "dl-icon-pop",
                    input {
                        class: "dl-icon-filter",
                        r#type: "search",
                        placeholder: "Filter icons by name",
                        aria_label: "Filter icons by name",
                        autofocus: true,
                        value: "{filter}",
                        oninput: move |e| filter.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Escape {
                                open.set(false);
                            }
                        },
                    }
                    div { class: "dl-icons", role: "listbox", aria_label: "Icons",
                        button {
                            class: if current.is_none() { "dl-icon on" } else { "dl-icon" },
                            r#type: "button",
                            title: "{none}",
                            onclick: move |_| pick(None),
                            span { class: "dl-no-icon", "–" }
                        }
                        for icon in Icon::ALL.iter().copied().filter(|i| i.name().contains(filter().trim().to_lowercase().as_str())) {
                            button {
                                key: "{icon.name()}",
                                class: if current == Some(icon) { "dl-icon on" } else { "dl-icon" },
                                r#type: "button",
                                title: "{icon.name()}",
                                onclick: move |_| pick(Some(icon)),
                                IconView { icon, size: 15 }
                            }
                        }
                    }
                }
            }
        }
    }
}
