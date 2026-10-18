//! The Settings window's key bindings: every binding in force, area by area,
//! each changed, taken away or put back in place, written into the user's
//! file as it is changed.

use std::time::Duration;

use dioxus::prelude::*;

use crate::keymap::{Action, Chord, Context, Keymap};
use crate::keys_edit;
use crate::paths::keys_path;
use crate::view::Keys;

/// How long recording keys waits for another before it ends.
const RECORD_WAIT: Duration = Duration::from_secs(2);

const STARTER: &str = "# Key bindings over the defaults, area by area.\n";

fn read_file() -> String {
    std::fs::read_to_string(keys_path()).unwrap_or_default()
}

fn write_file(text: &str) -> Result<(), String> {
    let path = keys_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = if text.trim().is_empty() && !path.exists() {
        STARTER.to_string()
    } else {
        text.to_string()
    };
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn area_hint(ctx: Context) -> &'static str {
    match ctx {
        Context::Global => "Everywhere, after the area's own bindings.",
        Context::Sidebar => "The list of workspaces and sessions.",
        Context::Details => "The panel on the right.",
        Context::Console => "The console while it shows a session.",
        Context::Prompt => "The prompt, while Neovim is not writing.",
    }
}

fn area_title(ctx: Context) -> &'static str {
    match ctx {
        Context::Global => "Global",
        Context::Sidebar => "Sidebar",
        Context::Details => "Details",
        Context::Console => "Console",
        Context::Prompt => "Prompt",
    }
}

/// Which binding is open for editing: its area, and its keys unless it is
/// a new one.
#[derive(Debug, Clone, PartialEq)]
struct Open {
    area: &'static str,
    keys: Option<String>,
}

#[component]
pub fn KeysPane() -> Element {
    let mut text = use_signal(read_file);
    let mut filter = use_signal(String::new);
    let mut open = use_signal(|| None::<Open>);
    let mut failed = use_signal(|| None::<String>);
    let mut confirm_reset = use_signal(|| false);
    // The file changed by hand shows here as well, unless a binding is open.
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let now = read_file();
            if open.peek().is_none() && *text.peek() != now {
                text.set(now);
            }
        }
    });
    let mut apply = move |next: Result<String, keys_edit::Error>| match next {
        Ok(t) => match write_file(&t) {
            Ok(()) => {
                text.set(t);
                failed.set(None);
                open.set(None);
            }
            Err(e) => failed.set(Some(format!("cannot write {}: {e}", keys_path().display()))),
        },
        Err(e) => failed.set(Some(e.to_string())),
    };
    let map = Keymap::with_user(&text());
    let removed = keys_edit::removed(&text());
    let defaults = Keymap::defaults().listed();
    let f = filter().to_lowercase();
    let shows = move |keys: &str, action: &str| {
        f.is_empty() || keys.to_lowercase().contains(&f) || action.contains(&f)
    };
    let path = keys_path();
    rsx! {
        div { class: "keys-bar",
            input {
                class: "keys-bar-search",
                placeholder: "Filter by key or action",
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
                        apply(keys_edit::reset_all(&t));
                    },
                    "Reset every binding"
                }
                button { onclick: move |_| confirm_reset.set(false), "Keep them" }
            } else {
                button { onclick: move |_| confirm_reset.set(true), "Reset all…" }
            }
        }
        if let Some(e) = failed() {
            div { class: "settings-error", "{e}" }
        }
        match map {
            Err(e) => rsx! {
                div { class: "settings-error", "keys.toml cannot be read, so it cannot be changed here until it is fixed: {e}" }
            },
            Ok(map) => rsx! {
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
                            .filter(|(a, k)| *a == area && shows(k, ""))
                            .map(|(_, k)| k.clone())
                            .collect();
                        let adding = open() == Some(Open { area, keys: None });
                        rsx! {
                            section { key: "{area}", class: "settings-group keys-area",
                                div { class: "keys-area-head",
                                    h2 { "{area_title(ctx)}" }
                                    div { class: "settings-hint", "{area_hint(ctx)}" }
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
                                                    text: text(),
                                                    on_apply: apply,
                                                    on_close: move |_| open.set(None),
                                                }
                                            }
                                        } else {
                                            rsx! {
                                                div {
                                                    key: "{keys}",
                                                    class: "binding",
                                                    title: "Click to change",
                                                    onclick: move |_| open.set(Some(Open { area, keys: Some(k.clone()) })),
                                                    span { class: "binding-keys", Keys { keys: keys.clone() } }
                                                    span { class: "binding-action settings-mono", "{action}" }
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
                                                                let keys = keys.clone();
                                                                move |e: Event<MouseData>| {
                                                                    e.stop_propagation();
                                                                    let t = text.peek().clone();
                                                                    apply(keys_edit::reset(&t, area, &keys));
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
                                        span { class: "binding-action settings-mono",
                                            "{keys_edit::default_of(area, &keys).unwrap_or_default()}"
                                        }
                                        span { class: "binding-mark", "taken away" }
                                        button {
                                            class: "settings-link",
                                            onclick: {
                                                let keys = keys.clone();
                                                move |_| {
                                                    let t = text.peek().clone();
                                                    apply(keys_edit::reset(&t, area, &keys))
                                                }
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
                                        text: text(),
                                        on_apply: apply,
                                        on_close: move |_| open.set(None),
                                    }
                                } else {
                                    div { class: "binding-add",
                                        button { class: "settings-link", onclick: move |_| open.set(Some(Open { area, keys: None })), "+ Add binding" }
                                    }
                                }
                            }
                        }
                    }
                }
            },
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
    on_apply: EventHandler<Result<String, keys_edit::Error>>,
    on_close: EventHandler<()>,
) -> Element {
    let area = ctx.name();
    let was_keys = keys.clone();
    let was_action = action.clone();
    let mut typed = use_signal(|| keys.clone().unwrap_or_default());
    let mut chosen = use_signal(|| {
        if action.is_empty() {
            "cursor.down".to_string()
        } else {
            action.clone()
        }
    });
    let mut recording = use_signal(|| keys.is_none());
    let mut before = use_signal(String::new);
    let mut recorded = use_signal(Vec::<String>::new);
    let mut epoch = use_signal(|| 0u64);
    use_effect(move || {
        if recording() {
            let _ = document::eval(
                "setTimeout(() => document.querySelector('.binding-form .binding-recorder')?.focus(), 0)",
            );
        }
    });
    let mut start = move || {
        before.set(typed.peek().clone());
        recorded.set(Vec::new());
        typed.set(String::new());
        recording.set(true);
    };
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
            on_apply.call(next);
        }
    };
    let groups: Vec<(&str, Vec<&str>)> = {
        let mut gs: Vec<(&str, Vec<&str>)> = Vec::new();
        for n in Action::NAMES {
            let g = n.split('.').next().unwrap_or(n);
            match gs.iter_mut().find(|(name, _)| *name == g) {
                Some((_, v)) => v.push(n),
                None => gs.push((g, vec![n])),
            }
        }
        gs
    };
    rsx! {
        div {
            class: "binding-form",
            onkeydown: move |e| {
                if !recording() && e.key() == Key::Escape {
                    on_close.call(());
                }
            },
            div { class: "binding-fields",
                input {
                    class: if recording() { "binding-recorder recording settings-mono" } else { "binding-recorder settings-mono" },
                    readonly: recording(),
                    placeholder: if recording() { "Press the keys…" } else { "Ctrl+w h, g g" },
                    value: "{typed}",
                    oninput: move |e| typed.set(e.value()),
                    onkeydown: {
                    let save = save.clone();
                    move |e: Event<KeyboardData>| {
                        if !recording() {
                            if e.key() == Key::Enter && valid && !same {
                                save();
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
                    }},
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
                    button { onclick: move |_| start(), "Record" }
                }
                select {
                    class: "binding-action-select",
                    onchange: move |e| chosen.set(e.value()),
                    // An action with its argument, as `console.send Ctrl+w`,
                    // is kept as it was written.
                    if !Action::NAMES.contains(&was_action.as_str()) && !was_action.is_empty() {
                        option { value: "{was_action}", selected: chosen() == was_action, "{was_action}" }
                    }
                    for (g , names) in groups {
                        optgroup { key: "{g}", label: "{g}",
                            for n in names {
                                option { key: "{n}", value: "{n}", selected: chosen() == n, "{n}" }
                            }
                        }
                    }
                }
            }
            if recording() {
                div { class: "binding-note", "Press each key of the sequence; Backspace takes the last one back. It ends by itself after a pause." }
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
                        onclick: move |_| on_apply.call(keys_edit::unbind(&text, area, &old)),
                        "Delete"
                    }
                }
            }
        }
    }
}
