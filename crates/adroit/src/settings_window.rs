//! The Settings window: the settings by category, each applied as it is
//! changed, with the key bindings beside them.

use std::cell::RefCell;
use std::path::Path;
use std::rc::{Rc, Weak};

use dioxus::desktop::{
    Config as Desktop, DesktopService, LogicalSize, WindowBuilder, WindowCloseBehaviour, window,
};
use dioxus::prelude::*;

use crate::icons::{Icon, IconView};
use crate::keys_pane::KeysPane;
use crate::paths::{home, keys_path};
use crate::programs::{Launch, format_command, format_env, parse_command, parse_env};
use crate::settings::{self, Settings};
use crate::style::SETTINGS_CSS;
use crate::zoom::LEVELS;

thread_local! {
    static OPEN: RefCell<Weak<DesktopService>> = const { RefCell::new(Weak::new()) };
}

/// Bring the Settings window forward, opening it when it is not open.
pub fn open() {
    if let Some(w) = OPEN.with(|o| o.borrow().upgrade()) {
        w.window.set_visible(true);
        w.window.set_focus();
        return;
    }
    let cfg = Desktop::new()
        .with_menu(None)
        .with_close_behaviour(WindowCloseBehaviour::WindowCloses)
        .with_background_color((15, 17, 20, 255))
        .with_custom_head(format!("<style>{SETTINGS_CSS}</style>"))
        .with_window(
            WindowBuilder::new()
                .with_title("Settings")
                .with_maximizable(false)
                .with_inner_size(LogicalSize::new(820.0, 580.0))
                .with_min_inner_size(LogicalSize::new(640.0, 420.0)),
        );
    let _ = window().new_window(VirtualDom::new(SettingsWindow), cfg);
}

/// Close the Settings window, as the main window goes.
pub fn close() {
    if let Some(w) = OPEN.with(|o| o.borrow().upgrade()) {
        w.close();
    }
}

/// Close the Settings window when it is the one in front; whether it was.
pub fn close_in_front() -> bool {
    let Some(w) = OPEN.with(|o| o.borrow().upgrade()) else {
        return false;
    };
    if !w.window.is_focused() {
        return false;
    }
    w.close();
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    General,
    Console,
    Prompt,
    Programs,
    Keys,
}

impl Pane {
    const ALL: [Pane; 5] = [
        Pane::General,
        Pane::Console,
        Pane::Prompt,
        Pane::Programs,
        Pane::Keys,
    ];

    fn title(self) -> &'static str {
        match self {
            Pane::General => "General",
            Pane::Console => "Console",
            Pane::Prompt => "Prompt",
            Pane::Programs => "Programs",
            Pane::Keys => "Keys",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Pane::General => Icon::AdjustmentsHorizontal,
            Pane::Console => Icon::Terminal,
            Pane::Prompt => Icon::Pencil,
            Pane::Programs => Icon::Terminal2,
            Pane::Keys => Icon::Keyboard,
        }
    }
}

fn short(path: &Path) -> String {
    crate::picker::short(path, &home())
}

/// Hand `path` to `open` with `how`: `-R` shows it in the Finder, `-t` opens
/// it in the text editor. It is made from `starter` first when it does not
/// exist yet, so there is something to show.
pub(crate) fn open_with(how: &str, path: &Path, starter: &str) {
    if !path.exists() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, starter);
    }
    let _ = std::process::Command::new("open")
        .arg(how)
        .arg(path)
        .spawn();
}

const KEYS_STARTER: &str = "# Key bindings over the defaults, area by area, as in\n# [sidebar]\n# \"J\" = \"cursor.down\"\n";

#[component]
fn SettingsWindow() -> Element {
    use_hook(|| {
        let me = window();
        OPEN.with(|o| *o.borrow_mut() = Rc::downgrade(&me));
    });
    let mut pane = use_signal(|| Pane::General);
    let mut now = use_signal(settings::current);
    let mut failed = use_signal(|| None::<String>);
    // Changes made elsewhere, such as in the file, show here as well.
    use_hook(move || {
        spawn(async move {
            let mut changed = settings::CHANGED.subscribe();
            while changed.recv().await.is_ok() {
                now.set(settings::current());
            }
        });
    });
    let change = move |f: Change| {
        let mut next = now.peek().clone();
        f(&mut next);
        failed.set(settings::set(next).err());
    };
    // The window in the theme's colours, as the main one is.
    let root_style = {
        let theme = now.read().theme.clone();
        let palette = crate::themes::palette(&theme, &Settings::themes_dir()).unwrap_or_default();
        format!(
            ":root{{{}}}",
            crate::themes::css(&crate::themes::window(&palette))
        )
    };
    rsx! {
        style { "{root_style}" }
        div { class: "settings",
            nav { class: "settings-nav",
                for p in Pane::ALL {
                    div {
                        key: "{p.title()}",
                        class: if pane() == p { "settings-nav-item on" } else { "settings-nav-item" },
                        onclick: move |_| pane.set(p),
                        IconView { icon: p.icon(), size: 16 }
                        "{p.title()}"
                    }
                }
            }
            main { class: "settings-pane",
                h1 { "{pane().title()}" }
                if let Some(e) = failed() {
                    div { class: "settings-error", "Could not keep the settings: {e}" }
                }
                match pane() {
                    Pane::General => rsx! { General { now: now(), change } },
                    Pane::Console => rsx! { ConsolePane { now: now(), change } },
                    Pane::Prompt => rsx! { PromptPane { now: now(), change } },
                    Pane::Programs => rsx! { ProgramsPane { now: now(), change } },
                    Pane::Keys => rsx! { KeysPane {} },
                }
            }
        }
    }
}

type Change = Box<dyn FnOnce(&mut Settings)>;

/// One setting: what it is called and says on the left, its control on the
/// right.
#[component]
fn Row(label: String, hint: String, children: Element) -> Element {
    rsx! {
        div { class: "settings-row",
            div { class: "settings-row-label",
                div { "{label}" }
                if !hint.is_empty() {
                    div { class: "settings-hint", "{hint}" }
                }
            }
            div { class: "settings-row-control", {children} }
        }
    }
}

#[component]
fn General(now: Settings, change: Callback<Change>) -> Element {
    let files = [
        ("Settings", settings::Settings::path(), ""),
        ("Key bindings", keys_path(), KEYS_STARTER),
    ];
    rsx! {
        section { class: "settings-group",
            Row { label: "Zoom", hint: "How large the window draws everything when it opens; ⌘0 comes back to it.",
                select {
                    onchange: move |e| {
                        if let Ok(z) = e.value().parse::<f64>() {
                            change(Box::new(move |s: &mut Settings| s.zoom = z));
                        }
                    },
                    for l in LEVELS {
                        option { key: "{l}", value: "{l}", selected: (l - now.zoom).abs() < 1e-9, "{(l * 100.0).round()}%" }
                    }
                }
            }
            Row { label: "Keep the Mac awake", hint: "While a session or a shell runs, even with the window closed; the display still sleeps.",
                input {
                    r#type: "checkbox",
                    checked: now.keep_awake,
                    onchange: move |e| {
                        let on = e.checked();
                        change(Box::new(move |s: &mut Settings| s.keep_awake = on));
                    },
                }
            }
        }
        section { class: "settings-group",
            h2 { "Files" }
            for (name, path, starter) in files {
                Row { key: "{name}", label: name, hint: short(&path),
                    button { onclick: move |_| open_with("-R", &path, starter), "Show in Finder" }
                }
            }
        }
    }
}

/// The weights a font is offered in.
const WEIGHTS: [(u16, &str); 9] = [
    (100, "Thin"),
    (200, "Extra light"),
    (300, "Light"),
    (400, "Regular"),
    (500, "Medium"),
    (600, "Semibold"),
    (700, "Bold"),
    (800, "Extra bold"),
    (900, "Black"),
];

#[component]
fn ConsolePane(now: Settings, change: Callback<Change>) -> Element {
    let themes_dir = Settings::themes_dir();
    let themes = crate::themes::names(&themes_dir);
    let known = themes.contains(&now.theme);
    rsx! {
        section { class: "settings-group",
            Row { label: "Theme", hint: "The console's colours. A theme of your own goes in the themes folder, a file in Ghostty's theme format named as the theme.",
                select {
                    onchange: move |e| {
                        let theme = e.value();
                        change(Box::new(move |s: &mut Settings| s.theme = theme));
                    },
                    for t in themes {
                        option { key: "{t}", value: "{t}", selected: now.theme == t, "{t}" }
                    }
                    if !known {
                        option { value: "{now.theme}", selected: true, "{now.theme}" }
                    }
                }
            }
            Row { label: "Themes folder", hint: short(&themes_dir),
                button {
                    onclick: move |_| {
                        let _ = std::fs::create_dir_all(&themes_dir);
                        let _ = std::process::Command::new("open").arg(&themes_dir).spawn();
                    },
                    "Open"
                }
            }
            Row { label: "Font", hint: "The family the console and the prompt are drawn in, as the system names it; empty for the built-in ones.",
                input {
                    r#type: "text",
                    value: "{now.font_family}",
                    placeholder: "SF Mono",
                    onchange: move |e| {
                        let family = e.value();
                        change(Box::new(move |s: &mut Settings| s.font_family = family));
                    },
                }
            }
            Row { label: "Size", hint: "In points; a line is as tall as the size needs.",
                input {
                    r#type: "number",
                    min: "8",
                    max: "32",
                    step: "0.5",
                    value: "{now.font_size}",
                    onchange: move |e| {
                        if let Ok(n) = e.value().parse::<f64>() {
                            change(Box::new(move |s: &mut Settings| s.font_size = n));
                        }
                    },
                }
            }
            Row { label: "Weight", hint: "Bold text is drawn heavier than this.",
                select {
                    onchange: move |e| {
                        if let Ok(w) = e.value().parse::<u16>() {
                            change(Box::new(move |s: &mut Settings| s.font_weight = w));
                        }
                    },
                    for (w , name) in WEIGHTS {
                        option { key: "{w}", value: "{w}", selected: now.font_weight == w, "{name}" }
                    }
                }
            }
        }
    }
}

#[component]
fn PromptPane(now: Settings, change: Callback<Change>) -> Element {
    rsx! {
        section { class: "settings-group",
            Row { label: "Fewest lines", hint: "The prompt is never shorter than this.",
                input {
                    r#type: "number",
                    min: "1",
                    max: "40",
                    value: "{now.prompt_min_rows}",
                    onchange: move |e| {
                        if let Ok(n) = e.value().parse::<usize>() {
                            change(Box::new(move |s: &mut Settings| s.prompt_min_rows = n));
                        }
                    },
                }
            }
            Row { label: "Most lines", hint: "Past this the prompt scrolls inside.",
                input {
                    r#type: "number",
                    min: "1",
                    max: "60",
                    value: "{now.prompt_max_rows}",
                    onchange: move |e| {
                        if let Ok(n) = e.value().parse::<usize>() {
                            change(Box::new(move |s: &mut Settings| s.prompt_max_rows = n));
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn ProgramsPane(now: Settings, change: Callback<Change>) -> Element {
    let login = crate::programs::login_shell();
    rsx! {
        section { class: "settings-group",
            h2 { "Shell" }
            Row { label: "Command", hint: "The shell a shell tab runs, with its arguments; empty for your login shell, started as a login shell.",
                input {
                    class: "settings-command",
                    r#type: "text",
                    value: "{format_command(&now.shell.command)}",
                    placeholder: "{login} -l",
                    onchange: move |e| {
                        if let Some(c) = parse_command(&e.value()) {
                            change(Box::new(move |s: &mut Settings| s.shell.command = c));
                        }
                    },
                }
            }
            Row { label: "Environment", hint: "Variables set for it, one a line as NAME=value.",
                textarea {
                    class: "settings-env",
                    value: "{format_env(&now.shell.env)}",
                    placeholder: "LANG=en_US.UTF-8",
                    onchange: move |e| {
                        let env = parse_env(&e.value());
                        change(Box::new(move |s: &mut Settings| s.shell.env = env));
                    },
                }
            }
        }
        LaunchGroup {
            title: "Claude Code",
            hint: "The program every session runs, with the arguments it always gets ahead of adroit's own; empty for claude.",
            placeholder: "claude",
            launch: now.claude.clone(),
            set: move |l: Launch| change(Box::new(move |s: &mut Settings| s.claude = l)),
        }
    }
}

/// How one program other than the shell is started: its command, whether
/// through the shell, and its variables.
#[component]
fn LaunchGroup(
    title: String,
    hint: String,
    placeholder: String,
    launch: Launch,
    set: Callback<Launch>,
) -> Element {
    let (a, b, c) = (launch.clone(), launch.clone(), launch.clone());
    rsx! {
        section { class: "settings-group",
            h2 { "{title}" }
            Row { label: "Command", hint,
                input {
                    class: "settings-command",
                    r#type: "text",
                    value: "{format_command(&launch.command)}",
                    placeholder,
                    onchange: move |e| {
                        if let Some(command) = parse_command(&e.value()) {
                            set(Launch { command, ..a.clone() });
                        }
                    },
                }
            }
            Row { label: "Start through the shell", hint: "Run by your shell as an interactive login shell, so what its startup files set reaches it.",
                input {
                    r#type: "checkbox",
                    checked: launch.via_shell,
                    onchange: move |e| set(Launch { via_shell: e.checked(), ..b.clone() }),
                }
            }
            Row { label: "Environment", hint: "Variables set for it, one a line as NAME=value.",
                textarea {
                    class: "settings-env",
                    value: "{format_env(&launch.env)}",
                    onchange: move |e| set(Launch { env: parse_env(&e.value()), ..c.clone() }),
                }
            }
        }
    }
}
