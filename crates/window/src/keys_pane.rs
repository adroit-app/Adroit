//! The Settings window's key bindings, listed action by action, each with the
//! keys that run it wherever it works, or area by area as the file writes
//! them; either way changed, taken away or put back in place, and written
//! into the user's file as it is changed.

use std::rc::Rc;
use std::time::Duration;

use dioxus::prelude::*;

use adroit_core::config::Config;
use adroit_protocol::{Ask, FileText};

use crate::actions::Action;
use crate::components::{Keys, SettingsError};
use crate::feed::use_config;
use crate::icons::{Icon, IconView};
use crate::keymap::{Chord, Context, Keymap};
use crate::keys_by_action::{self, ActionKeys, Bound};
use crate::keys_edit;
use crate::link::{Link, failure};

/// How long recording keys waits for another before it ends.
const RECORD_WAIT: Duration = Duration::from_secs(2);

const STARTER: &str = "# Key bindings over the defaults, area by area.\n";

/// What the keys file held when the work host last read it.
fn read_file(config: &Config) -> String {
    match config.files().keys.as_ref().map(|f| &f.text) {
        Some(FileText::Read { text }) => text.clone(),
        _ => String::new(),
    }
}

/// How the bindings are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    /// Action by action, for changing what a key does without knowing the
    /// areas of the window.
    ByAction,
    /// Area by area, as the file writes them, for setting an area apart.
    ByArea,
}

impl View {
    const ALL: [View; 2] = [View::ByAction, View::ByArea];

    fn title(self) -> &'static str {
        match self {
            View::ByAction => "By action",
            View::ByArea => "By area",
        }
    }

    fn lead(self) -> &'static str {
        match self {
            View::ByAction => {
                "Each action with the keys that run it, in every area it works in. Click a key to change it, × to take it away, + to add one. A key marked with an area runs the action only there."
            }
            View::ByArea => {
                "Every binding, area by area, as keys.toml writes them. An area's binding wins there over a Global one; click one to change it."
            }
        }
    }
}

#[component]
pub fn KeysPane() -> Element {
    let config = use_config();
    let link = use_context::<Link>();
    let link = use_hook(move || CopyValue::new(link));
    let mut text = use_signal(|| read_file(&config.peek()));
    let mut filter = use_signal(String::new);
    let mut view = use_signal(|| View::ByAction);
    let mut failed = use_signal(|| None::<String>);
    let mut confirm_reset = use_signal(|| false);
    let mut editing = use_signal(|| false);
    // The file changed by hand shows here as well, unless a binding is open.
    use_effect(move || {
        let now = read_file(&config.read());
        if !*editing.peek() && *text.peek() != now {
            text.set(now);
        }
    });
    let mut apply = move |next: Result<String, String>| match next {
        Ok(t) => {
            text.set(t.clone());
            let saved = link.read().ask(Ask::SaveKeys { text: t });
            spawn(async move {
                failed.set(failure(saved.await));
            });
        }
        Err(e) => failed.set(Some(e)),
    };
    let path = config
        .read()
        .files()
        .keys
        .as_ref()
        .map(|f| f.path.clone())
        .unwrap_or_default();
    rsx! {
        div { class: "keys-bar",
            div { class: "segmented", role: "tablist",
                for v in View::ALL {
                    button {
                        key: "{v.title()}",
                        r#type: "button",
                        role: "tab",
                        aria_selected: view() == v,
                        class: if view() == v { "on" },
                        onclick: move |_| view.set(v),
                        "{v.title()}"
                    }
                }
            }
            input {
                class: "keys-bar-search",
                placeholder: "Filter by key, action or what it does",
                autocomplete: "off",
                spellcheck: "false",
                "autocorrect": "off",
                "autocapitalize": "off",
                value: "{filter}",
                oninput: move |e| filter.set(e.value()),
            }
            button { onclick: move |_| crate::settings_window::open_with("-t", &path, STARTER), "Open keys.toml" }
            if confirm_reset() {
                button {
                    class: "danger",
                    onclick: move |_| {
                        confirm_reset.set(false);
                        let t = text.peek().clone();
                        apply(keys_edit::reset_all(&t).map_err(|e| e.to_string()));
                    },
                    "Reset every binding"
                }
                button { onclick: move |_| confirm_reset.set(false), "Keep them" }
            } else {
                button { onclick: move |_| confirm_reset.set(true), "Reset all…" }
            }
        }
        p { class: "keys-lead", "{view().lead()}" }
        if let Some(e) = failed() {
            SettingsError { "{e}" }
        }
        match Keymap::with_user(&text()) {
            Err(e) => rsx! {
                SettingsError { "keys.toml cannot be read, so it cannot be changed here until it is fixed: {e}" }
            },
            Ok(map) => match view() {
                View::ByAction => rsx! {
                    ByAction {
                        text: text(),
                        filter: filter(),
                        map,
                        on_apply: apply,
                        on_editing: move |on| editing.set(on),
                    }
                },
                View::ByArea => rsx! {
                    ByArea {
                        text: text(),
                        filter: filter(),
                        map,
                        on_apply: apply,
                        on_editing: move |on| editing.set(on),
                    }
                },
            },
        }
    }
}

/// Whether `filter`, lower-cased, finds something in `haystacks`.
fn finds(filter: &str, haystacks: &[&str]) -> bool {
    filter.is_empty() || haystacks.iter().any(|h| h.to_lowercase().contains(filter))
}

/// Every action, heading by heading.
#[component]
fn ByAction(
    text: String,
    filter: String,
    map: Keymap,
    on_apply: EventHandler<Result<String, String>>,
    on_editing: EventHandler<bool>,
) -> Element {
    let sections = match keys_by_action::sections(&text) {
        Ok(sections) => sections,
        Err(e) => return rsx! { SettingsError { "{e}" } },
    };
    let f = filter.to_lowercase();
    rsx! {
        for section in sections {
            {
                let shown: Vec<ActionKeys> = section
                    .actions
                    .into_iter()
                    .filter(|a| {
                        finds(&f, &[&a.label, &a.action, &a.does]) || a.keys.iter().any(|b| finds(&f, &[&b.keys]))
                    })
                    .collect();
                let hidden = shown.is_empty();
                rsx! {
                    section { key: "{section.title}", class: "settings-group keys-area", hidden,
                        h2 { "{section.title}" }
                        for item in shown {
                            ActionRow {
                                key: "{item.action}",
                                item,
                                map: map.clone(),
                                text: text.clone(),
                                on_apply,
                                on_editing,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What the form under an action does.
#[derive(Debug, Clone, PartialEq)]
enum Editing {
    Adding,
    /// Puts other keys, or other areas, in place of these.
    Changing(Bound),
}

/// The areas `areas` names, as the Settings window titles them.
fn titles(areas: &[Context]) -> String {
    areas
        .iter()
        .map(|a| a.title())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// One action: its keys, each to change or take away, a way to add one, and
/// the way back to adroit's own.
#[component]
fn ActionRow(
    item: ActionKeys,
    map: Keymap,
    text: String,
    on_apply: EventHandler<Result<String, String>>,
    on_editing: EventHandler<bool>,
) -> Element {
    let mut editing = use_signal(|| None::<Editing>);
    let mut open = move |what: Option<Editing>| {
        on_editing.call(what.is_some());
        editing.set(what);
    };
    let action = item.action.clone();
    rsx! {
        div { class: if editing().is_some() { "action-row open" } else { "action-row" },
            div { class: "action-row-main",
                div { class: "action-row-text",
                    div { class: "action-row-label",
                        "{item.label}"
                        if item.changed {
                            span { class: "binding-mark", "changed" }
                            button {
                                class: "settings-link",
                                title: "Back to adroit's keys for it",
                                onclick: {
                                    let (text, action) = (text.clone(), action.clone());
                                    move |_| on_apply.call(keys_by_action::reset(&text, &action).map_err(|e| e.to_string()))
                                },
                                "Reset"
                            }
                        }
                    }
                    div { class: "settings-hint", title: "{item.does}", "{item.does}" }
                }
                div { class: "action-keys",
                    for bound in item.keys.clone() {
                        {
                            let partial = !item.everywhere(&bound);
                            let (b, r) = (bound.clone(), bound.clone());
                            let (text, action) = (text.clone(), action.clone());
                            rsx! {
                                span { key: "{bound.keys}", class: if partial { "key-chip partial" } else { "key-chip" },
                                    button {
                                        class: "chip-key",
                                        title: if partial { format!("Change; only in {}", titles(&bound.areas)) } else { "Change".to_string() },
                                        onclick: move |_| open(Some(Editing::Changing(b.clone()))),
                                        Keys { keys: bound.keys.clone() }
                                        if partial {
                                            span { class: "chip-where", "{titles(&bound.areas)}" }
                                        }
                                    }
                                    button {
                                        class: "chip-remove",
                                        title: "Take it away",
                                        aria_label: "Take {bound.keys} away",
                                        onclick: move |_| on_apply.call(
                                            keys_by_action::remove(&text, &action, &r.keys, &r.areas).map_err(|e| e.to_string()),
                                        ),
                                        IconView { icon: Icon::X, size: 11 }
                                    }
                                }
                            }
                        }
                    }
                    if item.keys.is_empty() {
                        span { class: "action-none", "No keys" }
                    }
                    if editing().is_none() {
                        button {
                            class: "chip-add",
                            title: "Add keys",
                            aria_label: "Add keys",
                            onclick: move |_| open(Some(Editing::Adding)),
                            IconView { icon: Icon::Plus, size: 12 }
                        }
                    }
                }
            }
            if let Some(what) = editing() {
                ActionForm {
                    item: item.clone(),
                    what,
                    map,
                    text,
                    on_apply: move |next: Result<String, String>| {
                        let done = next.is_ok();
                        on_apply.call(next);
                        if done {
                            open(None);
                        }
                    },
                    on_close: move |_| open(None),
                }
            }
        }
    }
}

/// The form keys are added to an action in, or changed: the keys, typed or
/// recorded, and the areas they run it in.
#[component]
fn ActionForm(
    item: ActionKeys,
    what: Editing,
    map: Keymap,
    text: String,
    on_apply: EventHandler<Result<String, String>>,
    on_close: EventHandler<()>,
) -> Element {
    let old = match &what {
        Editing::Adding => None,
        Editing::Changing(b) => Some(b.clone()),
    };
    let typed = use_signal(|| old.as_ref().map(|b| b.keys.clone()).unwrap_or_default());
    let recording = use_signal(|| old.is_none());
    let mut areas = use_signal(|| match &old {
        Some(b) => b.areas.clone(),
        // Global covers every area already.
        None if item.areas.contains(&Context::Global) => vec![Context::Global],
        None => item.areas.clone(),
    });
    let keys = typed().trim().to_string();
    let chosen: Vec<Context> = Context::ALL
        .into_iter()
        .filter(|c| areas().contains(c))
        .collect();
    let action = item.action.clone();
    let next = if keys.is_empty() || chosen.is_empty() {
        None
    } else {
        Some(match &old {
            None => keys_by_action::add(&text, &action, &keys, &chosen),
            Some(b) => {
                keys_by_action::replace(&text, &action, (&b.keys, &b.areas), (&keys, &chosen))
            }
        })
    };
    let valid = matches!(next, Some(Ok(_)));
    let same = old
        .as_ref()
        .is_some_and(|b| b.keys == keys && b.areas == chosen);
    let mine = format!(" is {action}");
    let clashes: Vec<String> = if valid {
        chosen
            .iter()
            .flat_map(|&area| {
                let except = old
                    .as_ref()
                    .filter(|b| b.areas.contains(&area))
                    .map(|b| b.keys.as_str());
                let place = if chosen.len() > 1 {
                    format!("{}: ", area.title())
                } else {
                    String::new()
                };
                map.clashes(area, &keys, except)
                    .into_iter()
                    .filter(|c| !c.ends_with(&mine))
                    .map(move |c| format!("{place}{c}"))
                    .collect::<Vec<_>>()
            })
            .collect()
    } else {
        Vec::new()
    };
    let everywhere_typing =
        valid && chosen.contains(&Context::Global) && keys_by_action::types(&keys);
    let save = {
        let next = next.clone();
        move || {
            if let Some(next) = next.clone() {
                on_apply.call(next.map_err(|e| e.to_string()));
            }
        }
    };
    rsx! {
        div {
            class: "binding-form",
            onkeydown: move |e| {
                if !recording() && e.key() == Key::Escape {
                    on_close.call(());
                }
            },
            KeyField {
                typed,
                recording,
                on_enter: {
                    let save = save.clone();
                    move |_| if valid && !same { save() }
                },
            }
            if item.areas.len() > 1 {
                div { class: "binding-areas",
                    span { class: "binding-areas-label", "Runs it in" }
                    for area in item.areas.clone() {
                        label { key: "{area.name()}", class: "binding-area", title: "{area.hint()}",
                            input {
                                r#type: "checkbox",
                                checked: chosen.contains(&area),
                                onchange: move |e| {
                                    let on = e.checked();
                                    areas.with_mut(|a| {
                                        a.retain(|c| *c != area);
                                        if on {
                                            a.push(area);
                                        }
                                    });
                                },
                            }
                            "{area.title()}"
                        }
                    }
                }
            }
            if !keys.is_empty() && chosen.is_empty() {
                div { class: "binding-note bad", "Choose an area for the keys to run it in." }
            }
            if let Some(Err(e)) = &next {
                div { class: "binding-note bad", "{keys} cannot be bound: {e}" }
            }
            for c in clashes {
                div { class: "binding-note warn", "{c}" }
            }
            if everywhere_typing {
                div { class: "binding-note warn",
                    "Bound in Global, {keys} is taken from the tree and the console, where it moves and types. Bind it in the areas it is for instead, or hold Ctrl or Command with it."
                }
            }
            div { class: "binding-actions",
                button {
                    class: "primary",
                    disabled: !valid || same || recording(),
                    onclick: move |_| save.clone()(),
                    if old.is_some() { "Save" } else { "Add" }
                }
                button { onclick: move |_| on_close.call(()), "Cancel" }
            }
        }
    }
}

/// Keys typed as the file writes them, or recorded as they are pressed.
#[component]
fn KeyField(typed: Signal<String>, recording: Signal<bool>, on_enter: EventHandler<()>) -> Element {
    let mut typed = typed;
    let mut recording = recording;
    let mut before = use_signal(String::new);
    let mut recorded = use_signal(Vec::<String>::new);
    let mut epoch = use_signal(|| 0u64);
    let mut field = use_signal(|| None::<Rc<MountedData>>);
    use_effect(move || {
        if recording()
            && let Some(f) = field()
        {
            spawn(async move {
                let _ = f.set_focus(true).await;
            });
        }
    });
    let mut start = move || {
        before.set(typed.peek().clone());
        recorded.set(Vec::new());
        typed.set(String::new());
        recording.set(true);
    };
    rsx! {
        div { class: "binding-fields",
            input {
                class: if recording() { "binding-recorder recording settings-mono" } else { "binding-recorder settings-mono" },
                readonly: recording(),
                placeholder: if recording() { "Press the keys…" } else { "Ctrl+q h, g g" },
                value: "{typed}",
                onmounted: move |e: MountedEvent| field.set(Some(e.data())),
                oninput: move |e| typed.set(e.value()),
                onkeydown: move |e: Event<KeyboardData>| {
                    if !recording() {
                        if e.key() == Key::Enter {
                            on_enter.call(());
                        }
                        return;
                    }
                    e.prevent_default();
                    e.stop_propagation();
                    let m = e.modifiers();
                    let name = e.key().to_string();
                    if name == "Backspace" && !m.ctrl() && !m.alt() {
                        recorded.write().pop();
                    } else if let Some(chord) = Chord::pressed(&name, m.ctrl(), m.alt(), m.shift(), m.meta()) {
                        recorded.write().push(chord);
                    } else {
                        return;
                    }
                    typed.set(recorded.peek().join(" "));
                    let this = *epoch.peek() + 1;
                    epoch.set(this);
                    spawn(async move {
                        tokio::time::sleep(RECORD_WAIT).await;
                        if *epoch.peek() == this {
                            recording.set(false);
                        }
                    });
                },
            }
            if recording() {
                button { onclick: move |_| recording.set(false), "Done" }
                button {
                    onclick: move |_| {
                        typed.set(before.peek().clone());
                        recording.set(false);
                    },
                    "Cancel"
                }
            } else {
                button { title: "Press the keys to bind", onclick: move |_| start(),
                    IconView { icon: Icon::Keyboard, size: 14 }
                    "Record"
                }
            }
        }
        if recording() {
            div { class: "binding-note", "Press each key of the sequence; Backspace takes the last one back. It ends by itself after a pause." }
        }
    }
}

/// Which binding is open for editing: its area, and its keys unless it is
/// a new one.
#[derive(Debug, Clone, PartialEq)]
struct Open {
    area: &'static str,
    keys: Option<String>,
}

/// Every binding, area by area, as the file writes them.
#[component]
fn ByArea(
    text: String,
    filter: String,
    map: Keymap,
    on_apply: EventHandler<Result<String, String>>,
    on_editing: EventHandler<bool>,
) -> Element {
    let mut open = use_signal(|| None::<Open>);
    let mut set_open = move |o: Option<Open>| {
        on_editing.call(o.is_some());
        open.set(o);
    };
    let removed = keys_edit::removed(&text);
    let defaults = Keymap::defaults().listed();
    let f = filter.to_lowercase();
    let label = |action: &str| {
        action
            .parse::<Action>()
            .map(|a| a.label())
            .unwrap_or_default()
    };
    let shows = |keys: &str, action: &str| finds(&f, &[keys, action, &label(action)]);
    let close = move |next: Result<String, String>| {
        let done = next.is_ok();
        on_apply.call(next);
        if done {
            set_open(None);
        }
    };
    rsx! {
        for ctx in Context::ALL {
            {
                let area = ctx.name();
                let rows: Vec<(String, String)> = map
                    .listed()
                    .into_iter()
                    .filter(|(a, k, act)| *a == area && shows(k, act))
                    .map(|(_, k, act)| (k, act))
                    .collect();
                let gone: Vec<String> = removed
                    .iter()
                    .filter(|(a, k)| *a == area && shows(k, &keys_edit::default_of(area, k).unwrap_or_default()))
                    .map(|(_, k)| k.clone())
                    .collect();
                let adding = open() == Some(Open { area, keys: None });
                // Filtering, an area with nothing that matches is noise.
                let hidden = !f.is_empty() && rows.is_empty() && gone.is_empty() && !adding;
                rsx! {
                    section { key: "{area}", class: "settings-group keys-area", hidden,
                        div { class: "keys-area-head",
                            h2 { "{ctx.title()}" }
                            div { class: "settings-hint", "{ctx.hint()}" }
                        }
                        for (keys , action) in rows {
                            {
                                let is_open = open() == Some(Open { area, keys: Some(keys.clone()) });
                                let default = defaults.iter().find(|(a, k, _)| *a == area && *k == keys).map(|(_, _, act)| act.clone());
                                let changed = default.as_deref() != Some(action.as_str());
                                let k = keys.clone();
                                if is_open {
                                    rsx! {
                                        BindingForm {
                                            key: "{keys}",
                                            ctx,
                                            keys: Some(keys.clone()),
                                            action: action.clone(),
                                            map: map.clone(),
                                            text: text.clone(),
                                            on_apply: close,
                                            on_close: move |_| set_open(None),
                                        }
                                    }
                                } else {
                                    rsx! {
                                        div {
                                            key: "{keys}",
                                            class: "binding",
                                            title: "Click to change",
                                            onclick: move |_| set_open(Some(Open { area, keys: Some(k.clone()) })),
                                            span { class: "binding-keys", Keys { keys: keys.clone() } }
                                            span { class: "binding-action",
                                                "{label(&action)}"
                                                span { class: "binding-name settings-mono", "{action}" }
                                            }
                                            if changed {
                                                span { class: "binding-mark",
                                                    if default.is_some() { "changed" } else { "added" }
                                                }
                                                button {
                                                    class: "settings-link",
                                                    title: match &default {
                                                        Some(d) => format!("Back to {d}"),
                                                        None => "Take it away".to_string(),
                                                    },
                                                    onclick: {
                                                        let (keys, text) = (keys.clone(), text.clone());
                                                        move |e: Event<MouseData>| {
                                                            e.stop_propagation();
                                                            on_apply.call(keys_edit::reset(&text, area, &keys).map_err(|e| e.to_string()));
                                                        }
                                                    },
                                                    if default.is_some() { "Reset" } else { "Remove" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        for keys in gone {
                            div { key: "gone-{keys}", class: "binding gone",
                                span { class: "binding-keys", Keys { keys: keys.clone() } }
                                span { class: "binding-action",
                                    "{label(&keys_edit::default_of(area, &keys).unwrap_or_default())}"
                                    span { class: "binding-name settings-mono", "{keys_edit::default_of(area, &keys).unwrap_or_default()}" }
                                }
                                span { class: "binding-mark", "taken away" }
                                button {
                                    class: "settings-link",
                                    onclick: {
                                        let (keys, text) = (keys.clone(), text.clone());
                                        move |_| on_apply.call(keys_edit::reset(&text, area, &keys).map_err(|e| e.to_string()))
                                    },
                                    "Restore"
                                }
                            }
                        }
                        if adding {
                            BindingForm {
                                ctx,
                                keys: None,
                                action: String::new(),
                                map: map.clone(),
                                text: text.clone(),
                                on_apply: close,
                                on_close: move |_| set_open(None),
                            }
                        } else {
                            div { class: "binding-add",
                                button { class: "settings-link", onclick: move |_| set_open(Some(Open { area, keys: None })), "+ Add binding" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The form a binding is changed or added in: its keys, typed or recorded
/// as they are pressed, and its action.
#[component]
fn BindingForm(
    ctx: Context,
    keys: Option<String>,
    action: String,
    map: Keymap,
    text: String,
    on_apply: EventHandler<Result<String, String>>,
    on_close: EventHandler<()>,
) -> Element {
    let area = ctx.name();
    let was_keys = keys.clone();
    let was_action = action.clone();
    let typed = use_signal(|| keys.clone().unwrap_or_default());
    let mut chosen = use_signal(|| {
        if action.is_empty() {
            "cursor.down".to_string()
        } else {
            action.clone()
        }
    });
    let recording = use_signal(|| keys.is_none());
    let new_keys = typed().trim().to_string();
    let valid = !new_keys.is_empty()
        && keys_edit::bind("", area, &new_keys, &chosen())
            .ok()
            .is_some_and(|t| Keymap::with_user(&t).is_ok());
    let same = was_keys.as_deref() == Some(new_keys.as_str()) && was_action == chosen();
    let clashes = if valid {
        map.clashes(ctx, &new_keys, was_keys.as_deref())
    } else {
        Vec::new()
    };
    let save = {
        let text = text.clone();
        let was_keys = was_keys.clone();
        move || {
            let keys = typed.peek().trim().to_string();
            let action = chosen.peek().clone();
            let cleared = match &was_keys {
                Some(old) if *old != keys => keys_edit::unbind(&text, area, old),
                _ => Ok(text.clone()),
            };
            let next = cleared.and_then(|t| {
                if keys_edit::default_of(area, &keys).as_deref() == Some(action.as_str()) {
                    keys_edit::reset(&t, area, &keys)
                } else {
                    keys_edit::bind(&t, area, &keys, &action)
                }
            });
            on_apply.call(next.map_err(|e| e.to_string()));
        }
    };
    let groups = keys_by_action::sections("").unwrap_or_default();
    rsx! {
        div {
            class: "binding-form",
            onkeydown: move |e| {
                if !recording() && e.key() == Key::Escape {
                    on_close.call(());
                }
            },
            KeyField {
                typed,
                recording,
                on_enter: {
                    let save = save.clone();
                    move |_| if valid && !same { save() }
                },
            }
            select {
                class: "binding-action-select",
                onchange: move |e| chosen.set(e.value()),
                // An action with an argument no row offers, as
                // `console.send Ctrl+q`, is kept as it was written.
                if !groups.iter().any(|g| g.actions.iter().any(|a| a.action == was_action)) && !was_action.is_empty() {
                    option { value: "{was_action}", selected: chosen() == was_action, "{was_action}" }
                }
                for g in groups {
                    optgroup { key: "{g.title}", label: "{g.title}",
                        for a in g.actions {
                            option { key: "{a.action}", value: "{a.action}", selected: chosen() == a.action, "{a.label} — {a.action}" }
                        }
                    }
                }
            }
            if !new_keys.is_empty() && !valid {
                div { class: "binding-note bad", "{new_keys} is not a key adroit knows." }
            }
            for c in clashes {
                div { class: "binding-note warn", "{c}" }
            }
            div { class: "binding-actions",
                button {
                    class: "primary",
                    disabled: !valid || same || recording(),
                    onclick: move |_| save.clone()(),
                    if was_keys.is_some() { "Save" } else { "Add" }
                }
                button { onclick: move |_| on_close.call(()), "Cancel" }
                div { class: "binding-actions-spacer" }
                if let Some(old) = was_keys.clone() {
                    button {
                        class: "danger",
                        onclick: move |_| on_apply.call(keys_edit::unbind(&text, area, &old).map_err(|e| e.to_string())),
                        "Delete"
                    }
                }
            }
        }
    }
}
