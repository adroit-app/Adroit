//! The Settings window: the settings by category, each applied as it is
//! changed, with the key bindings beside them.

use std::cell::RefCell;
use std::path::Path;
use std::rc::{Rc, Weak};

use adroit_model::config::{
    Launch, SessionOptions, format_command, format_env, parse_command, parse_env,
};
use adroit_model::paths::tilde;
use adroit_protocol::{AgentsWork, Answer, Ask};
use dioxus::desktop::{
    Config as Desktop, DesktopService, LogicalSize, WindowBuilder, WindowCloseBehaviour, window,
};
use dioxus::prelude::*;

use crate::components::{
    NumberField, Select, SettingGroup, SettingRow, SettingsError, TextArea, TextField, Toggle,
};
use crate::feed::{root_style, use_config};
use crate::fonts;
use crate::icons::{Icon, IconView};
use crate::keys_pane::KeysPane;
use crate::link::Link;
use crate::paths::home;
use crate::settings::{OptionAsAlt, Settings};
use crate::style::SETTINGS_CSS;
use crate::zoom::LEVELS;

thread_local! {
    static OPEN: RefCell<Weak<DesktopService>> = const { RefCell::new(Weak::new()) };
}

/// Bring the Settings window forward, opening it when it is not open; what
/// it saves is asked of the work host through `link`.
pub(crate) fn open(agents: AgentsWork, link: Link) {
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
    let _ = window().new_window(
        VirtualDom::new_with_props(SettingsWindow, SettingsWindowProps { agents })
            .with_root_context(link),
        cfg,
    );
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
    Agents,
    Programs,
    Keys,
}

impl Pane {
    const ALL: [Pane; 6] = [
        Pane::General,
        Pane::Console,
        Pane::Prompt,
        Pane::Agents,
        Pane::Programs,
        Pane::Keys,
    ];

    fn title(self) -> &'static str {
        match self {
            Pane::General => "General",
            Pane::Console => "Console",
            Pane::Prompt => "Prompt",
            Pane::Agents => "Agents",
            Pane::Programs => "Programs",
            Pane::Keys => "Keys",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Pane::General => Icon::AdjustmentsHorizontal,
            Pane::Console => Icon::Terminal,
            Pane::Prompt => Icon::Pencil,
            Pane::Agents => Icon::Message,
            Pane::Programs => Icon::Terminal2,
            Pane::Keys => Icon::Keyboard,
        }
    }
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
    let mut cmd = tokio::process::Command::new("open");
    cmd.arg(how).arg(path);
    spawn(async move {
        let _ = cmd.status().await;
    });
}

const KEYS_STARTER: &str =
    "# Key bindings over the defaults, area by area, as in\n# [tree]\n# \"J\" = \"cursor.down\"\n";

/// Why the settings cannot be had, while they cannot, as the file reads now.
#[component]
fn SettingsFileError() -> Element {
    let config = use_config();
    rsx! {
        SettingsLoadError { error: config.read().settings_error.clone() }
    }
}

/// `error`, why the settings cannot be had, told over the window.
#[component]
pub fn SettingsLoadError(error: Option<String>) -> Element {
    rsx! {
        if let Some(error) = error {
            div {
                class: "settings-error",
                role: "alert",
                style: "padding: 12px; color: var(--fail, #f08080); white-space: pre-wrap; flex: none; max-height: 30vh; overflow: auto; user-select: text",
                "{error}. Fix the configuration file before saving settings."
            }
        }
    }
}

/// The settings, `agents` being what the work host tells of each agent.
#[component]
fn SettingsWindow(agents: AgentsWork) -> Element {
    use_hook(|| {
        let me = window();
        OPEN.with(|o| *o.borrow_mut() = Rc::downgrade(&me));
    });
    let config = use_config();
    let link = use_context::<Link>();
    let mut pane = use_signal(|| Pane::General);
    let mut agent = use_signal(|| config.peek().settings.agent);
    let mut now = use_signal(|| config.peek().settings.clone());
    let mut failed = use_signal(|| None::<String>);
    // Changes made elsewhere, such as in the file, show here as well.
    use_effect(move || now.set(config.read().settings.clone()));
    let change = move |f: Change| {
        let mut next = now.peek().clone();
        f(&mut next);
        // Shown at once; the host sends the settings as it kept them.
        now.set(next.clone());
        let saved = toml::to_string_pretty(&next)
            .map(|text| link.ask(Ask::SaveSettings { text }))
            .map_err(|e| e.to_string());
        spawn(async move {
            failed.set(match saved {
                Ok(saved) => match saved.await {
                    Answer::Done => None,
                    Answer::Failed { text } => Some(text),
                    answer => Some(format!("the work host answered {answer:?}")),
                },
                Err(e) => Some(e),
            });
        });
    };
    // The window in the theme's colours, as the main one is.
    let root_style = root_style(&config.read());
    rsx! {
        style { "{root_style}" }
        div { class: "settings",
            nav { class: "settings-nav",
                for p in Pane::ALL {
                    button {
                        key: "{p.title()}",
                        r#type: "button",
                        aria_current: if pane() == p { "page" } else { "false" },
                        class: if pane() == p { "settings-nav-item on" } else { "settings-nav-item" },
                        onclick: move |_| pane.set(p),
                        IconView { icon: p.icon(), size: 16 }
                        "{p.title()}"
                    }
                    if p == Pane::Agents && pane() == Pane::Agents {
                        for choice in adroit_model::Agent::ALL {
                            button {
                                key: "{choice.key()}",
                                r#type: "button",
                                class: if agent() == choice { "settings-nav-item settings-agent on" } else { "settings-nav-item settings-agent" },
                                aria_current: if agent() == choice { "page" } else { "false" },
                                onclick: move |_| agent.set(choice),
                                "{choice.label()}"
                            }
                        }
                    }
                }
            }
            main { class: "settings-pane",
                SettingsFileError {}
                if let Some(e) = failed() {
                    SettingsError { "Could not keep the settings: {e}" }
                }
                match pane() {
                    Pane::General => rsx! { General { now: now(), change } },
                    Pane::Console => rsx! { ConsolePane { now: now(), change } },
                    Pane::Prompt => rsx! { PromptPane { now: now(), change } },
                    Pane::Agents => rsx! {
                        AgentsPane {
                            agent: agent(),
                            program: agents.of(agent()).map(|f| f.program.clone()).unwrap_or_default(),
                            now: now(),
                            change,
                        }
                    },
                    Pane::Programs => rsx! { ProgramsPane { now: now(), change } },
                    Pane::Keys => rsx! { KeysPane {} },
                }
            }
        }
    }
}

type Change = Box<dyn FnOnce(&mut Settings)>;

#[component]
fn General(now: Settings, change: Callback<Change>) -> Element {
    let config = use_config();
    let files = {
        let files = config.read();
        let files = files.files();
        let path = |file: Option<&adroit_protocol::ConfigFile>| {
            file.map(|f| f.path.clone()).unwrap_or_default()
        };
        [
            ("Settings", path(files.settings.as_ref()), ""),
            ("Key bindings", path(files.keys.as_ref()), KEYS_STARTER),
        ]
    };
    rsx! {
        SettingGroup {
            SettingRow { label: "Zoom", hint: "How large the window draws everything when it opens; ⌘0 comes back to it.",
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
            SettingRow { label: "Keep the Mac awake", hint: "While a session or a shell runs, even with the window closed; the display still sleeps.",
                Toggle {
                    checked: now.keep_awake,
                    onchange: move |on| change(Box::new(move |s: &mut Settings| s.keep_awake = on)),
                }
            }
            SettingRow { label: "Notify when a session waits", hint: "A notification when a session starts waiting on you while the window is not in front.",
                Toggle {
                    checked: now.notify_waiting,
                    onchange: move |on| change(Box::new(move |s: &mut Settings| s.notify_waiting = on)),
                }
            }
        }
        SettingGroup { title: "Default agent",
            SettingRow { label: "Agent", hint: "The agent a new session starts with; each can be switched before its first prompt. Sessions already running keep their agent.",
                select {
                    onchange: move |e| {
                        if let Some(agent) = adroit_model::Agent::parse(&e.value()) {
                            change(Box::new(move |s: &mut Settings| s.agent = agent));
                        }
                    },
                    for c in agent_choices(now.agent) {
                        option { key: "{c.agent.key()}", value: "{c.agent.key()}", selected: c.selected, "{c.label}" }
                    }
                }
            }
        }
        SettingGroup { title: "Starting sessions",
            SettingRow { label: "Scratch folder", hint: "Where scratch sessions run, each in a folder of its own; empty for adroit's own.",
                TextField {
                    value: "{now.scratch_dir}",
                    placeholder: "~/.local/share/adroit/scratch",
                    onchange: move |value: String| {
                        let dir = value.trim().to_string();
                        change(Box::new(move |s: &mut Settings| s.scratch_dir = dir));
                    },
                }
            }
            SettingRow { label: "Offer ghq's repositories", hint: "The picker lists the repositories ghq keeps, and its file tree ghq's roots.",
                Toggle {
                    checked: now.picker.ghq,
                    onchange: move |on| change(Box::new(move |s: &mut Settings| s.picker.ghq = on)),
                }
            }
            SettingRow { label: "Folders to offer", hint: "One a line: the picker lists the folders in each, and its file tree each one.",
                TextArea {
                    value: "{now.picker.roots.join(\"\\n\")}",
                    placeholder: "~/src",
                    onchange: move |value: String| {
                        let roots = value.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
                        change(Box::new(move |s: &mut Settings| s.picker.roots = roots));
                    },
                }
            }
        }
        SettingGroup { title: "System usage",
            SettingRow { label: "CPU", hint: "Whole-machine CPU utilization.",
                Toggle {
                    checked: now.usage.cpu,
                    onchange: move |on| change(Box::new(move |s: &mut Settings| s.usage.cpu = on)),
                }
            }
            SettingRow { label: "GPU", hint: "GPU utilization, when the system reports it.",
                Toggle {
                    checked: now.usage.gpu,
                    onchange: move |on| change(Box::new(move |s: &mut Settings| s.usage.gpu = on)),
                }
            }
            SettingRow { label: "Memory", hint: "Used physical memory as a percentage of total memory.",
                Toggle {
                    checked: now.usage.memory,
                    onchange: move |on| change(Box::new(move |s: &mut Settings| s.usage.memory = on)),
                }
            }
            SettingRow { label: "Storage", hint: "Used capacity on the startup volume.",
                Toggle {
                    checked: now.usage.storage,
                    onchange: move |on| change(Box::new(move |s: &mut Settings| s.usage.storage = on)),
                }
            }
        }
        SettingGroup { title: "Details panel",
            SettingRow { label: "Changes in Log", hint: "How many changes the Log sheet lists.",
                NumberField {
                    min: "1", max: "1000",
                    value: "{now.details.log_commits}",
                    onchange: move |value: String| if let Ok(n) = value.parse::<usize>() {
                        change(Box::new(move |s: &mut Settings| s.details.log_commits = n));
                    },
                }
            }
            SettingRow { label: "Lines of a diff", hint: "How much of a diff is shown before it is cut.",
                NumberField {
                    min: "1", max: "100000",
                    value: "{now.details.diff_lines}",
                    onchange: move |value: String| if let Ok(n) = value.parse::<usize>() {
                        change(Box::new(move |s: &mut Settings| s.details.diff_lines = n));
                    },
                }
            }
            SettingRow { label: "Links shown", hint: "How many links of each kind the overview shows before a row that shows them all.",
                NumberField {
                    min: "1", max: "1000",
                    value: "{now.details.links_shown}",
                    onchange: move |value: String| if let Ok(n) = value.parse::<usize>() {
                        change(Box::new(move |s: &mut Settings| s.details.links_shown = n));
                    },
                }
            }
            SettingRow { label: "Links kept", hint: "How many links of each kind the overview shows, the newest.",
                NumberField {
                    min: "1", max: "1000",
                    value: "{now.details.links_kept}",
                    onchange: move |value: String| if let Ok(n) = value.parse::<usize>() {
                        change(Box::new(move |s: &mut Settings| s.details.links_kept = n));
                    },
                }
            }
        }
        SettingGroup { title: "Files",
            for (name, path, starter) in files {
                SettingRow { key: "{name}", label: name, hint: tilde(&path, &home()),
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
    let config = use_config();
    let (themes, themes_dir) = {
        let config = config.read();
        let made = config.files().configured.clone().unwrap_or_default();
        (made.themes, made.themes_dir)
    };
    let known = themes.contains(&now.theme);
    let installed = use_resource(|| async {
        tokio::task::spawn_blocking(crate::installed_fonts::installed)
            .await
            .ok()
    });
    rsx! {
        SettingGroup {
            SettingRow { label: "Theme", hint: "The console's colours. A theme of your own goes in the themes folder, a file in Ghostty's theme format named as the theme.",
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
            SettingRow { label: "Themes folder", hint: tilde(&themes_dir, &home()),
                button {
                    onclick: move |_| {
                        let _ = std::fs::create_dir_all(&themes_dir);
                        let mut cmd = tokio::process::Command::new("open");
                        cmd.arg(&themes_dir);
                        spawn(async move {
                            let _ = cmd.status().await;
                        });
                    },
                    "Open"
                }
            }
            SettingRow { label: "Font", hint: "The family the console and the prompt are drawn in, among the monospace ones installed and adroit's own, which stands behind any other for Nerd Font glyphs.",
                select {
                    onchange: move |e| {
                        let family = e.value();
                        change(Box::new(move |s: &mut Settings| s.font_family = family));
                    },
                    for c in fonts::choices(installed.read().as_ref().and_then(|i| i.as_deref()), &now.font_family) {
                        option { key: "{c.value}", value: "{c.value}", selected: now.font_family == c.value, "{c.label}" }
                    }
                }
            }
            SettingRow { label: "Size", hint: "In points; a line is as tall as the size needs.",
                NumberField {
                    min: "8",
                    max: "32",
                    step: "0.5",
                    value: "{now.font_size}",
                    onchange: move |value: String| {
                        if let Ok(n) = value.parse::<f64>() {
                            change(Box::new(move |s: &mut Settings| s.font_size = n));
                        }
                    },
                }
            }
            SettingRow { label: "Option as Alt", hint: "Which Option keys send Alt (Meta) to what runs in the console; the others type what macOS makes of the key.",
                select {
                    onchange: move |e| {
                        let as_alt = match e.value().as_str() {
                            "left" => OptionAsAlt::Left,
                            "right" => OptionAsAlt::Right,
                            "none" => OptionAsAlt::None,
                            _ => OptionAsAlt::Both,
                        };
                        change(Box::new(move |s: &mut Settings| s.console.option_as_alt = as_alt));
                    },
                    for (value , name) in [("both", "Both"), ("left", "Left only"), ("right", "Right only"), ("none", "Neither")] {
                        option { key: "{value}", value: "{value}", selected: now.console.option_as_alt.name() == value, "{name}" }
                    }
                }
            }
            SettingRow { label: "Blinking cursor", hint: "The console's cursor blinks when the program lets it.",
                Toggle {
                    checked: now.console.cursor_blink,
                    onchange: move |on| change(Box::new(move |s: &mut Settings| s.console.cursor_blink = on)),
                }
            }
            SettingRow { label: "Weight", hint: "Bold text is drawn heavier than this.",
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
        SettingGroup {
            SettingRow { label: "Fewest lines", hint: "The prompt is never shorter than this.",
                NumberField {
                    min: "1",
                    max: "40",
                    value: "{now.prompt_min_rows}",
                    onchange: move |value: String| {
                        if let Ok(n) = value.parse::<usize>() {
                            change(Box::new(move |s: &mut Settings| s.prompt_min_rows = n));
                        }
                    },
                }
            }
            SettingRow { label: "Most lines", hint: "Past this the prompt scrolls inside.",
                NumberField {
                    min: "1",
                    max: "60",
                    value: "{now.prompt_max_rows}",
                    onchange: move |value: String| {
                        if let Ok(n) = value.parse::<usize>() {
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
    let config = use_config();
    let login = config
        .read()
        .files()
        .configured
        .as_ref()
        .map(|c| c.login_shell.clone())
        .unwrap_or_default();
    rsx! {
        SettingGroup { title: "Shell",
            SettingRow { label: "Command", hint: "The shell a shell tab runs, with its arguments; empty for your login shell, started as a login shell.",
                TextField {
                    value: "{format_command(&now.shell.command)}",
                    placeholder: "{login} -l",
                    onchange: move |value: String| {
                        if let Some(c) = parse_command(&value) {
                            change(Box::new(move |s: &mut Settings| s.shell.command = c));
                        }
                    },
                }
            }
            SettingRow { label: "Environment", hint: "Variables set for it, one a line as NAME=value.",
                TextArea {
                    value: "{format_env(&now.shell.env)}",
                    placeholder: "LANG=en_US.UTF-8",
                    onchange: move |value: String| {
                        let env = parse_env(&value);
                        change(Box::new(move |s: &mut Settings| s.shell.env = env));
                    },
                }
            }
        }
        LaunchGroup {
            title: "Neovim",
            hint: "The Neovim a Neovim tab runs, with its arguments; empty for nvim. It reads your own configuration.",
            placeholder: "nvim",
            launch: now.neovim.clone(),
            set: move |l: Launch| change(Box::new(move |s: &mut Settings| s.neovim = l)),
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
        SettingGroup { title: "{title}",
            SettingRow { label: "Command", hint,
                TextField {
                    value: "{format_command(&launch.command)}",
                    placeholder,
                    onchange: move |value: String| {
                        if let Some(command) = parse_command(&value) {
                            set(Launch { command, ..a.clone() });
                        }
                    },
                }
            }
            SettingRow { label: "Start through the shell", hint: "Run by your shell as an interactive login shell, so what its startup files set reaches it.",
                Toggle {
                    checked: launch.via_shell,
                    onchange: move |on| set(Launch { via_shell: on, ..b.clone() }),
                }
            }
            SettingRow { label: "Environment", hint: "Variables set for it, one a line as NAME=value.",
                TextArea {
                    value: "{format_env(&launch.env)}",
                    onchange: move |value: String| set(Launch { env: parse_env(&value), ..c.clone() }),
                }
            }
        }
    }
}

/// The permission modes Claude Code takes, as it names them, with what the
/// Settings window calls each; empty leaves it to Claude Code.
const MODES: [(&str, &str); 5] = [
    ("", "Claude Code's own"),
    ("default", "Ask before acting"),
    ("acceptEdits", "Accept edits"),
    ("plan", "Plan only"),
    ("bypassPermissions", "Bypass permissions"),
];

struct AgentChoice {
    agent: adroit_model::Agent,
    label: &'static str,
    selected: bool,
}

/// Each agent new sessions can run, the one they run now marked: a select's
/// own `value` does not pick the option it names when the pane is drawn.
fn agent_choices(chosen: adroit_model::Agent) -> Vec<AgentChoice> {
    adroit_model::Agent::ALL
        .into_iter()
        .map(|agent| AgentChoice {
            agent,
            label: agent.label(),
            selected: agent == chosen,
        })
        .collect()
}

/// The models offered to choose from; another can be typed.
const MODELS: [&str; 3] = ["opus", "sonnet", "haiku"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum AgentSection {
    Sessions,
    Launch,
}

impl AgentSection {
    const ALL: [Self; 2] = [Self::Sessions, Self::Launch];

    fn title(self) -> &'static str {
        match self {
            Self::Sessions => "Sessions",
            Self::Launch => "Launch",
        }
    }
}

#[component]
fn AgentsPane(
    agent: adroit_model::Agent,
    /// The program the agent runs unless its launch names another.
    program: String,
    now: Settings,
    change: Callback<Change>,
) -> Element {
    use adroit_model::Agent;
    let mut section = use_signal(|| AgentSection::Sessions);
    rsx! {
        div { class: "settings-agent-tabs", role: "group", aria_label: "Agent settings",
            for item in AgentSection::ALL {
                button {
                    r#type: "button",
                    key: "{item.title()}",
                    class: if section() == item { "on" } else { "" },
                    aria_pressed: section() == item,
                    onclick: move |_| section.set(item),
                    "{item.title()}"
                }
            }
        }
        p { class: "settings-agent-note",
            match section() {
                AgentSection::Launch => "How this agent starts.",
                AgentSection::Sessions => "Defaults for new sessions. Running sessions keep their settings.",
            }
        }
        if section() == AgentSection::Launch {
            LaunchGroup {
                key: "launch-{agent.key()}",
                title: "Launch",
                hint: "The program and its arguments; empty for the agent's own command.",
                placeholder: program.clone(),
                launch: now.launch(agent).clone(),
                set: move |launch: Launch| change(Box::new(move |s: &mut Settings| *s.launch_mut(agent) = launch)),
            }
        } else {
            match agent {
                Agent::Claude => rsx! {
                    SessionGroup {
                        key: "claude-{agent.key()}",
                        title: section().title(),
                        options: now.sessions.clone(),
                        set: move |options: SessionOptions| change(Box::new(move |s: &mut Settings| {
                            s.sessions = options;
                        })),
                    }
                },
                Agent::Codex => rsx! {
                    CodexGroup {
                        key: "codex-{agent.key()}",
                        title: section().title(),
                        options: now.codex_sessions.clone(),
                        set: move |options: adroit_model::config::CodexOptions| change(Box::new(move |s: &mut Settings| {
                            s.codex_sessions = options;
                        })),
                    }
                },
            }
        }
    }
}

#[component]
fn CodexGroup(
    title: String,
    options: adroit_model::config::CodexOptions,
    set: Callback<adroit_model::config::CodexOptions>,
) -> Element {
    let (model, effort, approval, sandbox) = (
        options.clone(),
        options.clone(),
        options.clone(),
        options.clone(),
    );
    rsx! {
        SettingGroup { title: "{title}",
            SettingRow { label: "Model", hint: "Empty to use Codex's own model.",
                TextField {
                    value: options.model,
                    aria_label: "Model",
                    placeholder: "Codex's own",
                    onchange: move |value: String| set(adroit_model::config::CodexOptions { model: value.trim().into(), ..model.clone() }),
                }
            }
            SettingRow { label: "Reasoning effort", hint: "Use a level supported by the selected model, or leave empty for Codex's own.",
                TextField {
                    list: "codex-efforts",
                    aria_label: "Reasoning effort",
                    value: options.reasoning_effort,
                    placeholder: "Codex's own",
                    onchange: move |value: String| set(adroit_model::config::CodexOptions { reasoning_effort: value.trim().into(), ..effort.clone() }),
                }
                datalist { id: "codex-efforts",
                    for value in ["low", "medium", "high", "xhigh", "max", "ultra"] {
                        option { key: "{value}", value: value }
                    }
                }
            }
            SettingRow { label: "Approval policy", hint: "When Codex asks before running a command. Never ask keeps sandbox restrictions in place.",
                Select { label: "Approval policy", value: options.approval_policy,
                    choices: vec![("", "Codex's own"), ("on-request", "Ask when needed"), ("never", "Never ask")],
                    onchange: move |value| set(adroit_model::config::CodexOptions { approval_policy: value, ..approval.clone() }),
                }
            }
            SettingRow { label: "Sandbox", hint: "Where commands can write. Full access removes the sandbox.",
                Select { label: "Sandbox", value: options.sandbox_mode,
                    choices: vec![("", "Codex's own"), ("read-only", "Read only"), ("workspace-write", "Workspace write"), ("danger-full-access", "Full access")],
                    onchange: move |value| set(adroit_model::config::CodexOptions { sandbox_mode: value, ..sandbox.clone() }),
                }
            }
        }
    }
}

/// How sessions of one kind run: their model, permission mode, the tools
/// denied them, and whether Adroit's channel warning is confirmed for them.
#[component]
fn SessionGroup(title: String, options: SessionOptions, set: Callback<SessionOptions>) -> Element {
    let (a, b, c, d) = (
        options.clone(),
        options.clone(),
        options.clone(),
        options.clone(),
    );
    let list = format!("models-{title}");
    rsx! {
        SettingGroup { title: "{title}",
            SettingRow { label: "Model", hint: "The model they start with, as Claude Code's --model takes it; empty for Claude Code's own.",
                TextField {
                    list: "{list}",
                    value: "{options.model}",
                    placeholder: "Claude Code's own",
                    onchange: move |value: String| set(SessionOptions { model: value.trim().to_string(), ..a.clone() }),
                }
                datalist { id: "{list}",
                    for m in MODELS {
                        option { key: "{m}", value: "{m}" }
                    }
                }
            }
            SettingRow { label: "Permission mode", hint: "How they ask before they act.",
                select {
                    onchange: move |e| set(SessionOptions { permission_mode: e.value(), ..b.clone() }),
                    for (mode , name) in MODES {
                        option { key: "{mode}", value: "{mode}", selected: options.permission_mode == mode, "{name}" }
                    }
                }
            }
            SettingRow { label: "Tools denied", hint: "Tools they may not use, separated by commas.",
                TextField {
                    value: "{options.disallowed_tools.join(\", \")}",
                    placeholder: "Edit, Write",
                    onchange: move |value: String| {
                        let tools = value.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
                        set(SessionOptions { disallowed_tools: tools, ..c.clone() })
                    },
                }
            }
            SettingRow { label: "Confirm Adroit's channel warning", hint: "Confirm, once a launch, Claude Code's warning about loading development channels when it names Adroit's own channel alone; every other dialog stays yours.",
                Toggle {
                    checked: options.confirm_channel_warning,
                    onchange: move |on| set(SessionOptions { confirm_channel_warning: on, ..d.clone() }),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_model::Agent;

    #[test]
    fn the_agent_new_sessions_run_is_the_one_marked_chosen() {
        for chosen in Agent::ALL {
            let marked: Vec<Agent> = agent_choices(chosen)
                .into_iter()
                .filter(|c| c.selected)
                .map(|c| c.agent)
                .collect();
            assert_eq!(marked, [chosen]);
        }
        let offered: Vec<Agent> = agent_choices(Agent::Claude)
            .into_iter()
            .map(|c| c.agent)
            .collect();
        assert_eq!(offered, Agent::ALL);
    }
}
