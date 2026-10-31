//! The Settings window: the settings by category, each applied as it is
//! changed, with the key bindings beside them.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use adroit_core::agent_pages::{self, AgentPage};
use adroit_model::EnvironmentId;
use adroit_model::config::{Launch, format_command, format_env, parse_command, parse_env};
use adroit_model::paths::tilde;
use adroit_protocol::{AgentsWork, Ask};
use dioxus::desktop::{
    Config as Desktop, DesktopService, LogicalSize, WindowBuilder, WindowCloseBehaviour, window,
};
use dioxus::prelude::*;

use crate::bridge::run_detached;
use crate::components::{
    NumberField, SettingGroup, SettingRow, SettingsError, SettingsFileError, TextArea, TextField,
    Toggle,
};
use crate::feed::{root_style, use_config};
use crate::fonts;
use crate::icons::{Icon, IconView};
use crate::keys_pane::KeysPane;
use crate::link::{Link, failure};
use crate::paths::home;
use crate::settings::{Appearance, OptionAsAlt, Settings, ThemeKind};
use crate::style::SETTINGS_CSS;
use crate::zoom::LEVELS;

mod about;
mod agents;
mod environment;
mod neovim;
mod profiles;

pub(crate) use neovim::use_asks;

thread_local! {
    static OPEN: RefCell<Weak<DesktopService>> = const { RefCell::new(Weak::new()) };
    /// The environment the Settings window open edits the work settings of.
    static EDITING: RefCell<Option<EnvironmentId>> = const { RefCell::new(None) };
}

/// The environment whose work settings the Settings window edits.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Edited {
    pub(crate) environment: EnvironmentId,
    /// What it is called, which the window is titled by.
    pub(crate) name: String,
    /// Where its work keeps its data on this computer.
    pub(crate) data_dir: Result<PathBuf, String>,
}

/// Bring the Settings window forward on `edited`'s work settings, opening it
/// when it is not open, or opening it again when it is open on another
/// environment's; what it saves is asked of that environment's work host
/// through `link`.
pub(crate) fn open(agents: AgentsWork, link: Link, edited: Edited) {
    if let Some(w) = OPEN.with(|o| o.borrow().upgrade()) {
        if EDITING.with(|e| e.borrow().as_ref() == Some(&edited.environment)) {
            w.window.set_visible(true);
            w.window.set_focus();
            return;
        }
        w.close();
    }
    EDITING.with(|e| *e.borrow_mut() = Some(edited.environment.clone()));
    let cfg = Desktop::new()
        .with_menu(None)
        .with_close_behaviour(WindowCloseBehaviour::WindowCloses)
        .with_background_color((15, 17, 20, 255))
        .with_custom_head(format!("<style>{SETTINGS_CSS}</style>"))
        .with_window(
            WindowBuilder::new()
                .with_title(format!("Settings · {}", edited.name))
                .with_maximizable(false)
                .with_inner_size(LogicalSize::new(820.0, 580.0))
                .with_min_inner_size(LogicalSize::new(640.0, 420.0)),
        );
    let _ = window().new_window(
        VirtualDom::new_with_props(SettingsWindow, SettingsWindowProps { agents })
            .with_root_context(link)
            .with_root_context(edited),
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
    Environment,
    Window,
    General,
    Appearance,
    Console,
    Prompt,
    Agents,
    Programs,
    Keys,
    Serving,
    Removed,
    About,
}

impl Pane {
    /// The panes of settings, in the order the sidebar lists them; About is
    /// set apart below them.
    const ALL: [Pane; 11] = [
        Pane::Environment,
        Pane::General,
        Pane::Appearance,
        Pane::Agents,
        Pane::Programs,
        Pane::Keys,
        Pane::Window,
        Pane::Console,
        Pane::Prompt,
        Pane::Serving,
        Pane::Removed,
    ];

    fn available_offline(self) -> bool {
        matches!(
            self,
            Pane::Environment
                | Pane::Window
                | Pane::Console
                | Pane::Prompt
                | Pane::Serving
                | Pane::Removed
                | Pane::About
        )
    }

    /// Where what a pane available offline changes is kept, and for what.
    fn kept_here(self) -> &'static str {
        match self {
            Pane::Environment => "Kept on this computer, for this environment.",
            Pane::Serving => "This computer, for other computers to work on.",
            Pane::Removed => "Kept on this computer; one restored is listed in every window again.",
            _ => "Saved on this computer for all environments.",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Pane::Environment => "Environment",
            Pane::Serving => "Accept connections",
            Pane::Removed => "Removed environments",
            Pane::Window => "Window",
            Pane::General => "General",
            Pane::Appearance => "Appearance",
            Pane::Console => "Console",
            Pane::Prompt => "Prompt",
            Pane::Agents => "Agents",
            Pane::Programs => "Programs",
            Pane::Keys => "Keys",
            Pane::About => "About",
        }
    }

    /// Whether the pane is headed by its title: a profile's page and About
    /// name themselves.
    fn titled(self) -> bool {
        !matches!(self, Pane::Agents | Pane::About)
    }

    fn icon(self) -> Icon {
        match self {
            Pane::Environment => Icon::Server,
            Pane::Serving => Icon::UsersGroup,
            Pane::Removed => Icon::SquareMinus,
            Pane::Window => Icon::AdjustmentsHorizontal,
            Pane::General => Icon::AdjustmentsHorizontal,
            Pane::Appearance => Icon::SunMoon,
            Pane::Console => Icon::Terminal,
            Pane::Prompt => Icon::Pencil,
            Pane::Agents => Icon::Message,
            Pane::Programs => Icon::Terminal2,
            Pane::Keys => Icon::Keyboard,
            Pane::About => Icon::InfoCircle,
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
    run_detached("open", [std::ffi::OsStr::new(how), path.as_os_str()]);
}

/// Hand `target`, a folder or an address, to `open`, as the Finder would.
fn run_open(target: impl AsRef<std::ffi::OsStr>) {
    run_detached("open", [target]);
}

const KEYS_STARTER: &str =
    "# Key bindings over the defaults, area by area, as in\n# [tree]\n# \"J\" = \"cursor.down\"\n";

/// The settings, `agents` being what the work host tells of each agent.
#[component]
fn SettingsWindow(agents: AgentsWork) -> Element {
    use_hook(|| {
        let me = window();
        OPEN.with(|o| *o.borrow_mut() = Rc::downgrade(&me));
    });
    let config = use_config();
    let link = use_context::<Link>();
    let edited = use_context::<Edited>();
    let pane = use_signal(|| Pane::General);
    let mut remote = use_signal(|| link.remote_status());
    use_future({
        let link = link.clone();
        move || {
            let link = link.clone();
            async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let next = link.remote_status();
                    if *remote.peek() != next {
                        remote.set(next);
                    }
                }
            }
        }
    });
    let mut chosen = use_signal(|| None::<AgentPage>);
    let mut now = use_signal(|| config.peek().settings.clone());
    let mut failed = use_signal(|| None::<String>);
    let left = use_signal(profiles::LeftHomes::new);
    // Changes made elsewhere, such as in the file, show here as well.
    use_effect(move || now.set(config.read().settings.clone()));
    let change = move |f: Change| {
        let mut next = now.peek().clone();
        f(&mut next);
        if crate::client_settings::Preferences::of(&next)
            != crate::client_settings::Preferences::of(&now.peek())
        {
            failed.set(crate::client_settings::save(&next).err());
            if failed.peek().is_none() {
                now.set(next);
            }
            return;
        }
        if link.remote_status().is_some_and(|status| !status.writable) {
            failed.set(Some(
                "Reconnect to the work machine before editing this environment's work settings"
                    .into(),
            ));
            return;
        }
        // A change refused leaves the file as the user wrote it.
        if next == *now.peek() {
            return;
        }
        // Shown at once; the host sends the settings as it kept them.
        now.set(next.clone());
        let saved = toml::to_string_pretty(&next)
            .map(|text| {
                let expected = config
                    .peek()
                    .files()
                    .settings
                    .as_ref()
                    .map_or(adroit_protocol::FileText::Missing, |file| file.text.clone());
                link.ask(Ask::SaveSettingsChecked { text, expected })
            })
            .map_err(|e| e.to_string());
        spawn(async move {
            failed.set(match saved {
                Ok(saved) => failure(saved.await),
                Err(e) => Some(e),
            });
        });
    };
    let page = agent_pages::shown(&now.read(), chosen.read().as_ref());
    // One scrolling pane shows every page, so it would keep the last one's
    // place.
    use_effect(move || {
        let _ = (pane(), chosen());
        document::eval("document.querySelector('.settings-pane')?.scrollTo(0, 0);");
    });
    // The window in the theme's colours, as the main one is.
    let root_style = root_style(&config.read());
    rsx! {
        style { "{root_style}" }
        div { class: "settings",
            nav { class: "settings-nav",
                p { class: "settings-nav-heading", "{edited.name}" }
                for p in Pane::ALL {
                    div { key: "{p.title()}",
                        if p == Pane::Window { p { class: "settings-nav-heading", "This computer · all environments" } }
                        NavItem { pane: p, shown: pane }
                        if p == Pane::Agents && pane() == Pane::Agents {
                            for entry in agent_pages::entries(&now()) {
                                button {
                                    key: "{entry.page:?}",
                                    r#type: "button",
                                    class: if entry.page == page { "settings-nav-item settings-agent on" } else { "settings-nav-item settings-agent" },
                                    aria_current: if entry.page == page { "page" } else { "false" },
                                    title: if entry.default { format!("{}, used for new sessions", entry.label) } else { entry.label.clone() },
                                    onclick: {
                                        let to = entry.page.clone();
                                        move |_| chosen.set(Some(to.clone()))
                                    },
                                    span { class: "settings-nav-label", "{entry.label}" }
                                    if entry.default {
                                        span { class: "settings-nav-mark", aria_label: "Used for new sessions",
                                            IconView { icon: Icon::Check, size: 12 }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "settings-nav-spacer" }
                NavItem { pane: Pane::About, shown: pane }
            }
            main { class: "settings-pane",
                if pane().titled() {
                    header { class: "settings-pane-head",
                        h1 { "{pane().title()}" }
                    }
                }
                div { class: "settings-measure",
                    if pane() != Pane::About {
                        p { class: "settings-agent-note", role: "status",
                            if pane().available_offline() {
                                "{pane().kept_here()}"
                            } else if let Some(status) = remote() {
                                "Work settings on {status.host}, for this environment."
                            } else {
                                "Work settings for this environment on this computer."
                            }
                        }
                    }
                    SettingsFileError { config }
                    if let Some(e) = failed() {
                        SettingsError { "Could not keep the settings: {e}" }
                    }
                    fieldset { class: "settings-fields", disabled: remote().is_some_and(|s| !s.writable) && !pane().available_offline(),
                    match pane() {
                        Pane::Window => rsx! { WindowPane { now: now(), change } },
                        Pane::General => rsx! { General { now: now(), change } },
                        Pane::Appearance => rsx! { AppearancePane { now: now(), change } },
                        Pane::Console => rsx! { ConsolePane { now: now(), change } },
                        Pane::Prompt => rsx! { PromptPane { now: now(), change } },
                        Pane::Agents => rsx! {
                            profiles::AgentsPane {
                                page: page.clone(),
                                now: now(),
                                agents: agents.clone(),
                                change,
                                choose: move |to: AgentPage| chosen.set(Some(to)),
                                left,
                            }
                        },
                        Pane::Programs => rsx! { ProgramsPane { now: now(), change } },
                        Pane::Keys => rsx! { KeysPane {} },
                        Pane::Environment => rsx! { environment::EnvironmentPane {} },
                        Pane::Serving => rsx! { crate::serving_pane::ServingPane {} },
                        Pane::Removed => rsx! { environment::RemovedPane {} },
                        Pane::About => rsx! { about::AboutPane {} },
                    }
                    }
                }
            }
        }
    }
}

/// The sidebar's item for `pane`, on while it is the one `shown`.
#[component]
fn NavItem(pane: Pane, shown: Signal<Pane>) -> Element {
    let on = shown() == pane;
    rsx! {
        button {
            r#type: "button",
            aria_current: if on { "page" } else { "false" },
            class: if on { "settings-nav-item on" } else { "settings-nav-item" },
            onclick: move |_| shown.set(pane),
            IconView { icon: pane.icon(), size: 16 }
            "{pane.title()}"
        }
    }
}

type Change = Box<dyn FnOnce(&mut Settings)>;

/// Options as a group shows them, with what each of its rows makes of them.
struct Edits<T> {
    options: Rc<T>,
    set: Callback<T>,
}

impl<T: Clone + 'static> Edits<T> {
    fn new(options: &T, set: Callback<T>) -> Self {
        Self {
            options: Rc::new(options.clone()),
            set,
        }
    }

    /// What a row does with a value: the options `edit` makes of it are
    /// `set`.
    fn of<V>(&self, edit: impl Fn(&mut T, V) + 'static) -> impl FnMut(V) + 'static {
        let (options, set) = (self.options.clone(), self.set);
        move |value| {
            let mut next = T::clone(&options);
            edit(&mut next, value);
            set(next);
        }
    }
}

/// What a number field does with what it holds once left: `set` it on the
/// settings when it reads as a number, and nothing otherwise.
fn set_number<T: std::str::FromStr + 'static>(
    change: Callback<Change>,
    set: fn(&mut Settings, T),
) -> impl FnMut(String) {
    move |value: String| {
        if let Ok(n) = value.parse::<T>() {
            change(Box::new(move |s: &mut Settings| set(s, n)));
        }
    }
}

#[component]
fn General(now: Settings, change: Callback<Change>) -> Element {
    let remote = use_context::<Link>().remote_status().is_some();
    rsx! {
        SettingGroup {
            SettingRow { label: if remote { "Keep the work machine awake" } else { "Keep the Mac awake" }, hint: "While a session or a shell runs on the work machine, even with the window closed; the display still sleeps.",
                Toggle {
                    checked: now.keep_awake,
                    onchange: move |on| change(Box::new(move |s: &mut Settings| s.keep_awake = on)),
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
                    onchange: set_number::<usize>(change, |s, n| s.details.log_commits = n),
                }
            }
            SettingRow { label: "Lines of a diff", hint: "How much of a diff is shown before it is cut.",
                NumberField {
                    min: "1", max: "100000",
                    value: "{now.details.diff_lines}",
                    onchange: set_number::<usize>(change, |s, n| s.details.diff_lines = n),
                }
            }
            SettingRow { label: "Links shown", hint: "How many links of each kind the overview shows before a row that shows them all.",
                NumberField {
                    min: "1", max: "1000",
                    value: "{now.details.links_shown}",
                    onchange: set_number::<usize>(change, |s, n| s.details.links_shown = n),
                }
            }
            SettingRow { label: "Links kept", hint: "How many links of each kind the overview shows, the newest.",
                NumberField {
                    min: "1", max: "1000",
                    value: "{now.details.links_kept}",
                    onchange: set_number::<usize>(change, |s, n| s.details.links_kept = n),
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

/// The appearances to choose among, as the settings name them.
const APPEARANCES: [(Appearance, &str); 3] = [
    (Appearance::System, "System"),
    (Appearance::Light, "Light"),
    (Appearance::Dark, "Dark"),
];

#[component]
fn AppearancePane(now: Settings, change: Callback<Change>) -> Element {
    let remote = use_context::<Link>().remote_status().is_some();
    let config = use_config();
    let made = config.read().files().configured.clone().unwrap_or_default();
    let of = |kind: ThemeKind| -> Vec<String> {
        made.themes
            .iter()
            .filter(|t| t.kind == kind)
            .map(|t| t.name.clone())
            .collect()
    };
    let (light, dark) = (of(ThemeKind::Light), of(ThemeKind::Dark));
    let system = match made.system {
        ThemeKind::Light => "light",
        ThemeKind::Dark => "dark",
    };
    let themes_dir = made.themes_dir;
    rsx! {
        SettingGroup {
            SettingRow { label: "Appearance", hint: "System follows the work machine's appearance, now {system}; Light and Dark keep to one theme.",
                select {
                    onchange: move |e| {
                        let chosen = APPEARANCES
                            .iter()
                            .find(|(a, _)| a.name() == e.value())
                            .map_or(Appearance::System, |(a, _)| *a);
                        change(Box::new(move |s: &mut Settings| s.appearance = chosen));
                    },
                    for (appearance, label) in APPEARANCES {
                        option { key: "{appearance.name()}", value: "{appearance.name()}", selected: now.appearance == appearance, "{label}" }
                    }
                }
            }
            SettingRow { label: "Light theme", hint: "The colours while the appearance is light. A theme of your own goes in the themes folder, a file in Ghostty's theme format named as the theme.",
                ThemeSelect {
                    themes: light,
                    now: now.light_theme.clone(),
                    onchange: move |theme: String| change(Box::new(move |s: &mut Settings| s.light_theme = theme)),
                }
            }
            SettingRow { label: "Dark theme", hint: "The colours while the appearance is dark.",
                ThemeSelect {
                    themes: dark,
                    now: now.dark_theme.clone(),
                    onchange: move |theme: String| change(Box::new(move |s: &mut Settings| s.dark_theme = theme)),
                }
            }
            SettingRow { label: "Themes folder", hint: if remote { themes_dir.display().to_string() } else { tilde(&themes_dir, &home()) },
                button {
                    disabled: remote,
                    title: if remote { "Manage themes on the work machine" } else { "Open the themes folder" },
                    onclick: move |_| {
                        if remote { return; }
                        let _ = std::fs::create_dir_all(&themes_dir);
                        run_open(&themes_dir);
                    },
                    IconView { icon: Icon::FolderOpen, size: 14 }
                    "Open"
                }
            }
        }
    }
}

/// A choice among `themes`, keeping `now` in view when it is not among
/// them, as a name with no such theme or one of the other kind.
#[component]
fn ThemeSelect(themes: Vec<String>, now: String, onchange: Callback<String>) -> Element {
    let known = themes.contains(&now);
    rsx! {
        select {
            onchange: move |e| onchange(e.value()),
            for t in themes {
                option { key: "{t}", value: "{t}", selected: now == t, "{t}" }
            }
            if !known {
                option { value: "{now}", selected: true, "{now}" }
            }
        }
    }
}

#[component]
fn ConsolePane(now: Settings, change: Callback<Change>) -> Element {
    let installed = use_resource(|| async {
        tokio::task::spawn_blocking(crate::installed_fonts::installed)
            .await
            .ok()
    });
    rsx! {
        SettingGroup {
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
                    onchange: set_number::<f64>(change, |s, n| s.font_size = n),
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
        neovim::NeovimGroup { name: "console" }
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
                    onchange: set_number::<usize>(change, |s, n| s.prompt_min_rows = n),
                }
            }
            SettingRow { label: "Most lines", hint: "Past this the prompt scrolls inside.",
                NumberField {
                    min: "1",
                    max: "60",
                    value: "{now.prompt_max_rows}",
                    onchange: set_number::<usize>(change, |s, n| s.prompt_max_rows = n),
                }
            }
        }
        neovim::NeovimGroup { name: "prompt" }
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

#[component]
fn WindowPane(now: Settings, change: Callback<Change>) -> Element {
    rsx! { SettingGroup {
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
            SettingRow { label: "Notify when a session waits", hint: "A notification when a session starts waiting on you while the window is not in front.",
                Toggle {
                    checked: now.notify_waiting,
                    onchange: move |on| change(Box::new(move |s: &mut Settings| s.notify_waiting = on)),
                }
            }
    } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnected_environments_keep_computer_preferences_editable() {
        for pane in [
            Pane::Environment,
            Pane::Window,
            Pane::Console,
            Pane::Prompt,
            Pane::Serving,
            Pane::Removed,
            Pane::About,
        ] {
            assert!(pane.available_offline(), "{pane:?} should remain available");
        }
        for pane in [
            Pane::General,
            Pane::Appearance,
            Pane::Agents,
            Pane::Programs,
            Pane::Keys,
        ] {
            assert!(
                !pane.available_offline(),
                "{pane:?} edits work-machine settings"
            );
        }
    }
}
