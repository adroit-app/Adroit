//! The window's settings as `settings.toml` writes them: what each one is,
//! what it is when the file does not say, and what is brought within reason.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use adroit_protocol::ThemeKind;

pub use adroit_model::Agent;
pub use adroit_model::config::CodexOptions;
pub use adroit_model::config::{Launch, SessionOptions, ShellLaunch};
use adroit_model::profile::earlier;
pub use adroit_model::profile::{AgentOptions, AgentProfile, ProfileId};

/// Which Option keys count as Alt, sending Meta, in a console; the others type
/// what macOS makes of the key with Option, as `Option-2` typing `™`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum OptionAsAlt {
    #[default]
    Both,
    Left,
    Right,
    None,
}

impl OptionAsAlt {
    pub fn name(self) -> &'static str {
        match self {
            OptionAsAlt::Both => "both",
            OptionAsAlt::Left => "left",
            OptionAsAlt::Right => "right",
            OptionAsAlt::None => "none",
        }
    }
}

/// Which of the light and the dark theme is drawn.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub fn name(self) -> &'static str {
        match self {
            Appearance::System => "system",
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        }
    }
}

/// The theme drawn in a light appearance when the settings name none.
pub const LIGHT_THEME: &str = "Dawnfox";

/// The theme drawn in a dark appearance when the settings name none.
pub const DARK_THEME: &str = "Nightfox";

/// How much the details panel reads and shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct DetailsSettings {
    /// The changes Log lists.
    pub log_commits: usize,
    /// The lines of a diff shown before it is cut.
    pub diff_lines: usize,
    /// The links of each kind the overview keeps, the newest, up to 1000.
    pub links_kept: usize,
    /// The links of each kind the overview shows before a row that shows them
    /// all.
    pub links_shown: usize,
}

impl Default for DetailsSettings {
    fn default() -> Self {
        Self {
            log_commits: 50,
            diff_lines: 2000,
            links_kept: 100,
            links_shown: 5,
        }
    }
}

/// Machine resources shown in Usage, independently of the active program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct UsageSettings {
    /// Show the whole machine's CPU use.
    pub cpu: bool,
    /// Show the GPU's use, when the system reports it.
    pub gpu: bool,
    /// Show the physical memory used, as a share of all of it.
    pub memory: bool,
    /// Show the capacity used on the startup volume.
    pub storage: bool,
}

impl Default for UsageSettings {
    fn default() -> Self {
        Self {
            cpu: true,
            gpu: true,
            memory: true,
            storage: true,
        }
    }
}

impl UsageSettings {
    pub fn any(self) -> bool {
        self.cpu || self.gpu || self.memory || self.storage
    }
}

/// How the console takes keys and draws its cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ConsoleSettings {
    /// Which Option keys send Alt to what runs there, `both`, `left`, `right` or
    /// `none`, the others typing what macOS makes of the key.
    pub option_as_alt: OptionAsAlt,
    /// Whether the cursor blinks when the program lets it.
    pub cursor_blink: bool,
}

// Read through `SettingsFile`, so a file and a launch the work host kept
// are read, and checked, the one way; `default` stays for the reference,
// which takes each setting's default from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, try_from = "SettingsFile")]
pub struct Settings {
    /// The profile a new session starts from, by id, when it names neither a
    /// profile nor an agent; left out, the first profile. A
    /// session not started yet can be switched to another agent for the
    /// current run of the window.
    pub profile: ProfileId,
    /// The profiles sessions start from, each `[[profiles]]` table one of them
    /// ([settings](settings.md#profiles)). An agent's default profile is the
    /// default profile when that starts the agent, else the agent's first
    /// profile.
    pub profiles: Vec<AgentProfile>,
    /// The profiles that a file written with connections had
    /// share one with a profile before them, and that run in their agent's
    /// own environment since; read to say so, never written.
    #[serde(skip)]
    #[schemars(skip)]
    pub unshared: Vec<ProfileId>,
    /// How sessions opened by sessions run.
    pub delegation: DelegationSettings,
    /// How large the window draws everything when it opens, from 0.67 to 1.75.
    /// Command 0 comes back to it.
    pub zoom: f64,
    /// The fewest lines the prompt shows.
    pub prompt_min_rows: usize,
    /// The most lines the prompt grows to before it scrolls inside.
    pub prompt_max_rows: usize,
    /// The font the console and the prompt are drawn in, as the system names its
    /// family; empty for Adroit's own, JetBrainsMono Nerd Font Mono. Adroit's own
    /// stands behind any other, so Nerd Font glyphs draw whatever font is chosen,
    /// and a Japanese font behind that. A line is as tall as the font asks, as in
    /// a terminal.
    pub font_family: String,
    /// The font's size, in points, from 8 to 32.
    pub font_size: f64,
    /// The font's weight, from 100 to 900; Adroit's own comes in regular and bold,
    /// and draws other weights in the nearer of them.
    pub font_weight: u16,
    /// Which of the two themes colours the console and the window: `system`
    /// follows macOS as it switches between light and dark, `light` and `dark`
    /// keep to one.
    pub appearance: Appearance,
    /// The theme drawn while the appearance is light, by name: one adroit comes
    /// with, or one in `themes/` beside `settings.toml`.
    pub light_theme: String,
    /// The theme drawn while the appearance is dark, named as the light one is.
    pub dark_theme: String,
    /// The one theme files written before themes followed the appearance
    /// name; read to keep their look, never written.
    #[serde(skip_serializing)]
    #[schemars(skip)]
    pub theme: Option<String>,
    /// Keep the Mac from idle sleep while a session or a shell runs, the window
    /// open or not; the display still sleeps.
    pub keep_awake: bool,
    /// Show a notification when a session starts waiting on you while the window
    /// is not in front; with several windows open, the one holding the lowest
    /// number tells.
    pub notify_waiting: bool,
    /// How a shell tab's shell starts.
    pub shell: ShellLaunch,
    /// How a Neovim tab's Neovim starts, as an agent does; an empty command
    /// runs `nvim`.
    pub neovim: Launch,
    /// Where scratch sessions run, `~` standing for your home folder; empty for
    /// Adroit's own folder.
    pub scratch_dir: String,
    /// Where the picker looks for directories to start a session in.
    pub picker: PickerSettings,
    /// How the console takes keys and draws its cursor.
    pub console: ConsoleSettings,
    /// How much the details panel reads and shows.
    pub details: DetailsSettings,
    /// Machine resources shown in Usage across all tabs.
    pub usage: UsageSettings,
}

/// `settings.toml` as it is read: the settings, and what files written
/// before profiles hold in their place.
// Its schema is never written: the reference describes the settings as they
// are written, which is `Settings`.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
struct SettingsFile {
    profile: Option<ProfileId>,
    profiles: Option<Vec<earlier::Written>>,
    #[schemars(skip)]
    connections: Vec<earlier::Connection>,
    agent: Option<Agent>,
    claude: Option<Launch>,
    sessions: Option<SessionOptions>,
    codex: Option<Launch>,
    codex_sessions: Option<CodexOptions>,
    delegation: DelegationSettings,
    #[schemars(skip)]
    orchestration: Retired,
    zoom: f64,
    prompt_min_rows: usize,
    prompt_max_rows: usize,
    font_family: String,
    font_size: f64,
    font_weight: u16,
    appearance: Appearance,
    light_theme: String,
    dark_theme: String,
    theme: Option<String>,
    keep_awake: bool,
    notify_waiting: bool,
    shell: ShellLaunch,
    neovim: Launch,
    scratch_dir: String,
    picker: PickerSettings,
    console: ConsoleSettings,
    details: DetailsSettings,
    usage: UsageSettings,
}

impl Default for SettingsFile {
    fn default() -> Self {
        let s = Settings::default();
        Self {
            profile: None,
            profiles: None,
            connections: Vec::new(),
            agent: None,
            claude: None,
            sessions: None,
            codex: None,
            codex_sessions: None,
            delegation: s.delegation,
            orchestration: Retired,
            zoom: s.zoom,
            prompt_min_rows: s.prompt_min_rows,
            prompt_max_rows: s.prompt_max_rows,
            font_family: s.font_family,
            font_size: s.font_size,
            font_weight: s.font_weight,
            appearance: s.appearance,
            light_theme: s.light_theme,
            dark_theme: s.dark_theme,
            theme: s.theme,
            keep_awake: s.keep_awake,
            notify_waiting: s.notify_waiting,
            shell: s.shell,
            neovim: s.neovim,
            scratch_dir: s.scratch_dir,
            picker: s.picker,
            console: s.console,
            details: s.details,
            usage: s.usage,
        }
    }
}

impl TryFrom<SettingsFile> for Settings {
    type Error = String;

    fn try_from(file: SettingsFile) -> Result<Self, Self::Error> {
        let earlier: Vec<&str> = [
            ("agent", file.agent.is_some()),
            ("claude", file.claude.is_some()),
            ("sessions", file.sessions.is_some()),
            ("codex", file.codex.is_some()),
            ("codex_sessions", file.codex_sessions.is_some()),
        ]
        .into_iter()
        .filter_map(|(key, named)| named.then_some(key))
        .collect();
        let mixed = |profile_key: &str| {
            let keys: Vec<String> = earlier.iter().map(|k| format!("`{k}`")).collect();
            format!(
                "`{profile_key}` cannot be named beside {}, which only settings written before profiles hold; move what they say into a profile",
                keys.join(", ")
            )
        };
        let (profile, (profiles, unshared)) = match file.profiles {
            Some(_) if !earlier.is_empty() => return Err(mixed("profiles")),
            Some(written) => {
                let profiles = earlier::adopt(written, &file.connections)?;
                let profile = match file.profile {
                    Some(profile) => profile,
                    None => profiles.0.first().map(|p| p.id.clone()).ok_or(
                        "the settings hold no profile; add one, or leave `profiles` out for the ones adroit comes with",
                    )?,
                };
                (profile, profiles)
            }
            None if file.profile.is_some() && !earlier.is_empty() => {
                return Err(mixed("profile"));
            }
            None if !file.connections.is_empty() && !earlier.is_empty() => {
                return Err(mixed("connections"));
            }
            None => {
                let claude = AgentProfile {
                    launch: file.claude.unwrap_or_default(),
                    options: AgentOptions::Claude(file.sessions.unwrap_or_default()),
                    ..AgentProfile::builtin(Agent::Claude)
                };
                let codex = AgentProfile {
                    launch: file.codex.unwrap_or_default(),
                    options: AgentOptions::Codex(file.codex_sessions.unwrap_or_default()),
                    ..AgentProfile::builtin(Agent::Codex)
                };
                let profile = file
                    .profile
                    .unwrap_or_else(|| ProfileId::builtin(file.agent.unwrap_or_default()));
                (
                    profile,
                    (
                        vec![
                            claude,
                            codex,
                            AgentProfile::builtin(Agent::Pi),
                            AgentProfile::builtin(Agent::OpenCode),
                        ],
                        Vec::new(),
                    ),
                )
            }
        };
        adroit_model::profile::check(&profile, &profiles)?;
        Ok(Self {
            profile,
            profiles,
            unshared,
            delegation: file.delegation,
            zoom: file.zoom,
            prompt_min_rows: file.prompt_min_rows,
            prompt_max_rows: file.prompt_max_rows,
            font_family: file.font_family,
            font_size: file.font_size,
            font_weight: file.font_weight,
            appearance: file.appearance,
            light_theme: file.light_theme,
            dark_theme: file.dark_theme,
            theme: file.theme,
            keep_awake: file.keep_awake,
            notify_waiting: file.notify_waiting,
            shell: file.shell,
            neovim: file.neovim,
            scratch_dir: file.scratch_dir,
            picker: file.picker,
            console: file.console,
            details: file.details,
            usage: file.usage,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct DelegationSettings {
    /// How many sessions opened by sessions may run at once, from 0 to 64. Zero
    /// refuses new ones without ending those already running. Sessions you
    /// start and terminals do not count.
    pub max_started: usize,
}

impl Default for DelegationSettings {
    fn default() -> Self {
        Self { max_started: 4 }
    }
}

/// A setting no longer read, accepted in whatever shape it was written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Retired;

impl<'de> Deserialize<'de> for Retired {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        serde::de::IgnoredAny::deserialize(deserializer).map(|_| Self)
    }
}

/// The fonts the console falls back to: adroit's own first, for the Nerd Font
/// glyphs a chosen font may lack, and a Japanese one for what a monospace
/// font leaves out.
pub const MONO: &str = r#""JetBrainsMono Nerd Font Mono", "SF Mono", Menlo, ui-monospace, "Hiragino Kaku Gothic ProN", monospace"#;

impl Default for Settings {
    fn default() -> Self {
        Self {
            profile: ProfileId::builtin(Agent::Claude),
            profiles: Agent::ALL.map(AgentProfile::builtin).to_vec(),
            unshared: Vec::new(),
            delegation: DelegationSettings::default(),
            zoom: 1.0,
            prompt_min_rows: 3,
            prompt_max_rows: 12,
            font_family: String::new(),
            font_size: 13.0,
            font_weight: 400,
            appearance: Appearance::System,
            light_theme: LIGHT_THEME.into(),
            dark_theme: DARK_THEME.into(),
            theme: None,
            keep_awake: true,
            notify_waiting: false,
            shell: ShellLaunch::default(),
            neovim: Launch::default(),
            scratch_dir: String::new(),
            picker: PickerSettings::default(),
            console: ConsoleSettings::default(),
            details: DetailsSettings::default(),
            usage: UsageSettings::default(),
        }
    }
}

/// Where the picker looks for directories to start a session in, beside where
/// sessions run and Claude Code has been used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PickerSettings {
    /// Offer the repositories ghq keeps, and its roots in the file tree.
    pub ghq: bool,
    /// Folders whose folders are offered, and which the file tree lists, `~`
    /// standing for your home folder.
    pub roots: Vec<String>,
}

impl Default for PickerSettings {
    fn default() -> Self {
        Self {
            ghq: true,
            roots: Vec::new(),
        }
    }
}

impl PickerSettings {
    /// The roots as paths, `~` standing for `home`.
    pub fn root_paths(&self, home: &std::path::Path) -> Vec<PathBuf> {
        self.roots
            .iter()
            .map(|r| r.trim())
            .filter(|r| !r.is_empty())
            .map(|r| expand(r, home))
            .collect()
    }
}

/// `path` with a leading `~` standing for `home`.
pub(crate) fn expand(path: &str, home: &std::path::Path) -> PathBuf {
    match path.strip_prefix('~') {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None => PathBuf::from(path),
    }
}

/// Settings earlier versions wrote, for projects adroit no longer keeps:
/// dropped as they are read, so a file they are in still reads.
const RETIRED: &[&str] = &["naming_model", "projects_dir"];

impl Settings {
    /// The agent of the profile a new session starts from.
    pub fn default_agent(&self) -> Agent {
        self.profiles
            .iter()
            .find(|p| p.id == self.profile)
            .map_or_else(Agent::default, AgentProfile::agent)
    }

    /// The profile a session of `agent` starts from when it names none: the
    /// default profile when that starts `agent`, else `agent`'s first
    /// profile, else the one adroit comes with for it.
    pub fn profile_of(&self, agent: Agent) -> Cow<'_, AgentProfile> {
        match self.position_of(agent) {
            Some(at) => Cow::Borrowed(&self.profiles[at]),
            None => Cow::Owned(self.own(agent)),
        }
    }

    /// The profile whose id is `id`: one of the profiles, or the one adroit
    /// comes with for an agent that has none, as [`Settings::profile_of`]
    /// starts it.
    pub fn find_profile(&self, id: &ProfileId) -> Option<Cow<'_, AgentProfile>> {
        if let Some(profile) = self.profiles.iter().find(|p| &p.id == id) {
            return Some(Cow::Borrowed(profile));
        }
        Agent::ALL
            .into_iter()
            .filter(|a| self.position_of(*a).is_none())
            .map(|a| self.own(a))
            .find(|p| &p.id == id)
            .map(Cow::Owned)
    }

    /// The profile [`Settings::profile_of`] names, added to the profiles when
    /// `agent` has none.
    pub fn profile_of_mut(&mut self, agent: Agent) -> &mut AgentProfile {
        let at = match self.position_of(agent) {
            Some(at) => at,
            None => {
                self.profiles.push(self.own(agent));
                self.profiles.len() - 1
            }
        };
        &mut self.profiles[at]
    }

    /// The profile adroit comes with for `agent`, under the agent's key, or
    /// numbered after it while a profile holds that: an id shared with a
    /// profile of another agent would find that one instead.
    fn own(&self, agent: Agent) -> AgentProfile {
        let taken = |id: &ProfileId| self.profiles.iter().any(|p| &p.id == id);
        let id = crate::profiles::numbered(agent.key())
            .filter_map(|id| id.parse::<ProfileId>().ok())
            .find(|id| !taken(id))
            .expect("some numbered id is free");
        AgentProfile {
            id,
            ..AgentProfile::builtin(agent)
        }
    }

    /// What a session of `agent` started from the profile `profile` in the
    /// login `home` is shown as: its agent, and the profile's name when it
    /// is not the agent's own name, as for the profile adroit comes with. A
    /// profile these settings no longer hold, or hold under its id for
    /// another agent or login, as one deleted and its id taken again, is
    /// named `called`, what it was called as the session started, or by its
    /// id when that is not known.
    pub fn started_as(
        &self,
        agent: Agent,
        profile: &ProfileId,
        home: Option<&std::path::Path>,
        called: &str,
    ) -> String {
        let same = |p: &&AgentProfile| p.agent() == agent && p.home.as_deref() == home;
        let label = match self.profiles.iter().find(|p| &p.id == profile) {
            Some(p) if same(&p) => p.label().to_string(),
            None if home.is_none()
                && (*profile == ProfileId::builtin(agent) || self.own(agent).id == *profile) =>
            {
                agent.label().to_string()
            }
            _ if !called.trim().is_empty() => called.trim().to_string(),
            _ => profile.to_string(),
        };
        if label == agent.label() {
            label
        } else {
            format!("{} · {label}", agent.label())
        }
    }

    fn position_of(&self, agent: Agent) -> Option<usize> {
        let open = |p: &AgentProfile| p.agent() == agent;
        self.profiles
            .iter()
            .position(|p| p.id == self.profile && open(p))
            .or_else(|| self.profiles.iter().position(open))
    }

    /// The settings made sensible: rows of at least one and the most no fewer
    /// than the fewest, a zoom among the levels' range.
    pub fn sane(mut self) -> Self {
        self.delegation.max_started = self.delegation.max_started.min(64);
        self.prompt_min_rows = self.prompt_min_rows.clamp(1, 40);
        self.prompt_max_rows = self.prompt_max_rows.clamp(self.prompt_min_rows, 60);
        let (least, most) = (
            crate::zoom::LEVELS[0],
            crate::zoom::LEVELS[crate::zoom::LEVELS.len() - 1],
        );
        self.zoom = self.zoom.clamp(least, most);
        self.font_family = match self.font_family.trim() {
            crate::fonts::FAMILY => String::new(),
            family => family.to_string(),
        };
        // Earlier versions offered a theme of adroit's own as "Adroit", in
        // Nightfox's colours on a darker ground.
        let named = |name: &str, otherwise: &str| match name.trim() {
            "" => otherwise.to_string(),
            "Adroit" => DARK_THEME.to_string(),
            name => name.to_string(),
        };
        self.light_theme = named(&self.light_theme, LIGHT_THEME);
        self.dark_theme = named(&self.dark_theme, DARK_THEME);
        self.theme = self
            .theme
            .filter(|t| !t.trim().is_empty())
            .map(|t| named(&t, DARK_THEME));
        self.font_size = self.font_size.clamp(8.0, 32.0);
        self.font_weight = (self.font_weight.clamp(100, 900) + 50) / 100 * 100;
        let words = |c: Vec<String>| -> Vec<String> {
            c.into_iter()
                .map(|w| w.trim().to_string())
                .filter(|w| !w.is_empty())
                .collect()
        };
        self.shell.command = words(self.shell.command);
        for profile in &mut self.profiles {
            profile.launch.command = words(std::mem::take(&mut profile.launch.command));
        }
        self.neovim.command = words(self.neovim.command);
        let d = &mut self.details;
        d.log_commits = d.log_commits.clamp(1, 1000);
        d.diff_lines = d.diff_lines.clamp(1, 100_000);
        d.links_kept = d.links_kept.clamp(1, adroit_model::links::KEPT);
        d.links_shown = d.links_shown.max(1);
        self
    }

    /// The theme in force while the system's appearance is `system`.
    pub fn theme_in(&self, system: ThemeKind) -> &str {
        let kind = match self.appearance {
            Appearance::System => system,
            Appearance::Light => ThemeKind::Light,
            Appearance::Dark => ThemeKind::Dark,
        };
        match kind {
            ThemeKind::Light => &self.light_theme,
            ThemeKind::Dark => &self.dark_theme,
        }
    }

    /// The settings with the one theme an earlier version named, of `kind`,
    /// taken as the theme of that kind and that appearance chosen, so the
    /// look stays as it was; settings without one as they are.
    pub fn migrated(mut self, kind: ThemeKind) -> Self {
        if let Some(theme) = self.theme.take() {
            match kind {
                ThemeKind::Light => {
                    self.light_theme = theme;
                    self.appearance = Appearance::Light;
                }
                ThemeKind::Dark => {
                    self.dark_theme = theme;
                    self.appearance = Appearance::Dark;
                }
            }
        }
        self
    }

    /// Where scratch sessions run, `~` standing for `home`, adroit's own
    /// folder being under `data`.
    pub fn scratch_folder(&self, home: &Path, data: &Path) -> PathBuf {
        match self.scratch_dir.trim() {
            "" => data.join("adroit").join("scratch"),
            dir => expand(dir, home),
        }
    }

    /// The console's fonts as CSS names them: the chosen one first.
    pub fn console_font(&self) -> String {
        if self.font_family.is_empty() {
            return MONO.to_string();
        }
        let quoted = self.font_family.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{quoted}\", {MONO}")
    }

    /// The settings `file` holds, as designed when there is none, or why
    /// they cannot be had, naming the file.
    pub fn read(file: &adroit_protocol::ConfigFile) -> Result<Self, String> {
        use adroit_protocol::FileText;
        let path = file.path.display();
        match &file.text {
            FileText::Missing => Ok(Settings::default()),
            FileText::Read { text } => {
                Settings::parse(text).map_err(|error| format!("Cannot load {path}: {error}"))
            }
            FileText::Unreadable { error } => Err(format!("Cannot read {path}: {error}")),
        }
    }

    /// The settings in `text`; what it does not name is as designed.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        let mut table: toml::Table = text.parse()?;
        for key in RETIRED {
            table.remove(*key);
        }
        Ok(Settings::deserialize(table)?.sane())
    }
}

/// The settings `text`, the file read again, holds when it reads and differs
/// from `current`; `None` otherwise, so a file half written or one adroit just
/// wrote itself changes nothing.
pub fn reread(current: &Settings, text: &str) -> Option<Settings> {
    Settings::parse(text).ok().filter(|s| s != current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// `settings` as adroit writes them, read back.
    fn written_and_read(settings: &Settings) -> Settings {
        Settings::parse(&toml::to_string_pretty(settings).unwrap()).unwrap()
    }

    #[test]
    fn a_file_missing_reads_as_designed_and_one_that_cannot_be_had_names_itself() {
        use adroit_protocol::{ConfigFile, FileText};
        let file = |text| ConfigFile {
            path: "/c/settings.toml".into(),
            text,
        };
        assert_eq!(
            Settings::read(&file(FileText::Missing)),
            Ok(Settings::default())
        );
        let read = Settings::read(&file(FileText::Read {
            text: "zoom = 1.25\n".into(),
        }));
        assert_eq!(read.map(|s| s.zoom), Ok(1.25));
        let broken = Settings::read(&file(FileText::Read {
            text: "zoom = ".into(),
        }))
        .unwrap_err();
        assert!(
            broken.starts_with("Cannot load /c/settings.toml: "),
            "{broken}"
        );
        assert_eq!(
            Settings::read(&file(FileText::Unreadable {
                error: "denied".into()
            })),
            Err("Cannot read /c/settings.toml: denied".into())
        );
    }

    #[test]
    fn each_agent_s_profile_is_edited_as_its_default_profile() {
        let mut s = Settings::parse("[codex]\ncommand = [\"/opt/codex\"]\n").unwrap();
        assert_eq!(s.profile_of(Agent::Codex).launch.command, ["/opt/codex"]);
        assert_eq!(s.profile_of(Agent::Claude).launch, Launch::default());
        s.profile_of_mut(Agent::Claude).launch.via_shell = true;
        assert!(s.profiles[0].launch.via_shell);
    }

    #[test]
    fn a_file_written_before_sessions_could_start_sessions_still_loads_and_loses_that_section() {
        let settings =
            Settings::parse("[orchestration]\nmax_workers = 2\nmax_workers_per_run = 1\n").unwrap();
        assert_eq!(settings.delegation, DelegationSettings::default());
        let written = toml::to_string_pretty(&settings).unwrap();
        assert!(!written.contains("orchestration"), "{written}");
    }

    #[test]
    fn machine_usage_is_enabled_by_default_and_each_resource_can_be_hidden() {
        let defaults = Settings::parse("").unwrap().usage;
        assert!(defaults.cpu && defaults.gpu && defaults.memory && defaults.storage);
        let settings =
            Settings::parse("[usage]\ncpu = false\ngpu = false\nmemory = true\nstorage = false\n")
                .unwrap();
        assert!(!settings.usage.cpu && !settings.usage.gpu && !settings.usage.storage);
        assert!(settings.usage.memory);
        assert_eq!(written_and_read(&settings), settings);
        assert!(Settings::parse("[usage]\nmemroy = false\n").is_err());
    }

    #[test]
    fn confirming_adroit_s_channel_warning_is_on_unless_the_user_turns_it_off() {
        let claude = |s: &Settings| s.profile_of(Agent::Claude).claude().cloned().unwrap();
        assert!(claude(&Settings::parse("").unwrap()).confirm_channel_warning);
        let settings = Settings::parse("[sessions]\nconfirm_channel_warning = false\n").unwrap();
        assert!(!claude(&settings).confirm_channel_warning);
        assert_eq!(written_and_read(&settings), settings);
    }

    #[test]
    fn a_file_written_before_profiles_that_chose_codex_starts_from_the_codex_profile() {
        assert_eq!(Settings::parse("").unwrap().default_agent(), Agent::Claude);
        let s = Settings::parse(
            r#"agent = "codex"
[codex]
command = ["/opt/bin/codex"]
via_shell = true
[codex.env]
FOO = "bar"
[codex_sessions]
model = "chosen-model"
"#,
        )
        .unwrap();
        assert_eq!(s.profile.as_str(), "codex");
        assert_eq!(s.default_agent(), Agent::Codex);
        let codex = s.profile_of(Agent::Codex);
        assert_eq!(codex.id.as_str(), "codex");
        assert_eq!(codex.home, None);
        assert_eq!(codex.launch.command, ["/opt/bin/codex"]);
        assert!(codex.launch.via_shell);
        assert_eq!(codex.launch.env.get("FOO").map(String::as_str), Some("bar"));
        assert_eq!(codex.codex().unwrap().model, "chosen-model");
        assert_eq!(
            s.profile_of(Agent::Claude).into_owned(),
            AgentProfile::builtin(Agent::Claude)
        );
        assert_eq!(written_and_read(&s), s);
        assert!(Settings::parse(r#"agent = "other""#).is_err());
    }

    #[test]
    fn settings_not_named_are_as_designed() {
        let s = Settings::parse("zoom = 1.25\n").unwrap();
        assert_eq!(s.zoom, 1.25);
        assert_eq!(s.prompt_min_rows, 3);
        assert!(Settings::parse("colour = \"red\"\n").is_err());
    }

    #[test]
    fn the_mac_is_kept_awake_unless_the_setting_says_not() {
        assert!(Settings::parse("").unwrap().keep_awake);
        assert!(!Settings::parse("keep_awake = false\n").unwrap().keep_awake);
    }

    #[test]
    fn settings_earlier_versions_kept_for_projects_are_read_and_dropped() {
        let s = Settings::parse(
            "zoom = 1.25\nnaming_model = \"haiku\"\nprojects_dir = \"~/Projects\"\n",
        )
        .unwrap();
        assert_eq!(s.zoom, 1.25);
    }

    #[test]
    fn settings_out_of_reason_are_brought_in() {
        let s = Settings {
            zoom: 9.0,
            prompt_min_rows: 0,
            prompt_max_rows: 0,
            font_family: "  ".into(),
            font_size: 99.0,
            font_weight: 1234,
            light_theme: " ".into(),
            dark_theme: "".into(),
            theme: Some("  ".into()),
            keep_awake: true,
            ..Settings::default()
        }
        .sane();
        assert_eq!(s.zoom, 1.75);
        assert_eq!((s.prompt_min_rows, s.prompt_max_rows), (1, 1));
        assert_eq!(s.font_family, "");
        assert_eq!(s.font_size, 32.0);
        assert_eq!(s.font_weight, 900);
        assert_eq!(s.light_theme, LIGHT_THEME);
        assert_eq!(s.dark_theme, DARK_THEME);
        assert_eq!(s.theme, None);
    }

    #[test]
    fn nightfox_and_dawnfox_are_drawn_when_the_settings_name_no_theme() {
        let s = Settings::default();
        assert_eq!(
            (s.dark_theme.as_str(), s.light_theme.as_str()),
            ("Nightfox", "Dawnfox")
        );
    }

    #[test]
    fn the_theme_earlier_versions_called_adroit_s_own_is_nightfox() {
        let s = Settings {
            light_theme: "Adroit".into(),
            dark_theme: " Adroit ".into(),
            theme: Some("Adroit".into()),
            ..Settings::default()
        }
        .sane();
        assert_eq!(s.light_theme, "Nightfox");
        assert_eq!(s.dark_theme, "Nightfox");
        assert_eq!(s.theme.as_deref(), Some("Nightfox"));
    }

    #[test]
    fn the_built_in_font_named_is_the_font_left_unnamed() {
        let s = Settings {
            font_family: format!(" {} ", crate::fonts::FAMILY),
            ..Settings::default()
        }
        .sane();
        assert_eq!(s.font_family, "");
    }

    #[test]
    fn the_theme_follows_the_system_with_a_light_and_a_dark_one_unless_one_appearance_is_chosen() {
        let s = Settings::parse("").unwrap();
        assert_eq!(s.appearance, Appearance::System);
        assert_eq!(s.light_theme, LIGHT_THEME);
        assert_eq!(s.dark_theme, DARK_THEME);
        let s = Settings::parse("light_theme = \"Latte\"\ndark_theme = \"Nord\"\n").unwrap();
        for (appearance, system, theme) in [
            (Appearance::System, ThemeKind::Light, "Latte"),
            (Appearance::System, ThemeKind::Dark, "Nord"),
            (Appearance::Light, ThemeKind::Light, "Latte"),
            (Appearance::Light, ThemeKind::Dark, "Latte"),
            (Appearance::Dark, ThemeKind::Light, "Nord"),
            (Appearance::Dark, ThemeKind::Dark, "Nord"),
        ] {
            let s = Settings {
                appearance,
                ..s.clone()
            };
            assert_eq!(s.theme_in(system), theme, "{appearance:?} in {system:?}");
        }
        let s = Settings::parse("appearance = \"light\"\n").unwrap();
        assert_eq!(s.appearance, Appearance::Light);
        assert_eq!(written_and_read(&s), s);
        assert!(Settings::parse("appearance = \"auto\"\n").is_err());
    }

    #[test]
    fn the_one_theme_earlier_versions_named_takes_the_slot_of_its_kind_and_that_appearance() {
        let s = Settings::parse("theme = \"Nord\"\n").unwrap();
        assert_eq!(s.theme.as_deref(), Some("Nord"));
        let dark = s.clone().migrated(ThemeKind::Dark);
        assert_eq!(
            (dark.appearance, dark.dark_theme.as_str(), dark.theme),
            (Appearance::Dark, "Nord", None)
        );
        assert_eq!(dark.light_theme, LIGHT_THEME);
        let light = Settings::parse("theme = \"Latte\"\n")
            .unwrap()
            .migrated(ThemeKind::Light);
        assert_eq!(
            (light.appearance, light.light_theme.as_str()),
            (Appearance::Light, "Latte")
        );
        assert_eq!(light.dark_theme, DARK_THEME);
        // Written again, the earlier key is gone.
        let written = toml::to_string_pretty(&light).unwrap();
        assert!(!written.contains("\ntheme"), "{written}");
        assert!(written.contains("light_theme = \"Latte\""), "{written}");
        // Settings without it are left as they are.
        let none = Settings::default();
        assert_eq!(none.clone().migrated(ThemeKind::Light), none);
    }

    #[test]
    fn the_console_s_font_comes_first_with_the_built_in_ones_behind_it() {
        let s = Settings {
            font_family: "IntoneMono Nerd Font".into(),
            ..Settings::default()
        };
        let family = s.console_font();
        assert!(family.starts_with("\"IntoneMono Nerd Font\", "), "{family}");
        assert!(family.ends_with("monospace"), "{family}");
        assert_eq!(Settings::default().console_font(), MONO);
    }

    #[test]
    fn the_console_is_drawn_in_the_built_in_nerd_font_with_a_japanese_one_behind_it() {
        let family = Settings::default().console_font();
        assert!(
            family.starts_with(&format!("\"{}\", ", crate::fonts::FAMILY)),
            "{family}"
        );
        assert!(family.contains("\"Hiragino Kaku Gothic ProN\""), "{family}");
    }

    #[test]
    fn settings_written_read_back_the_same() {
        let s = Settings {
            profile: "codex-review".parse().unwrap(),
            profiles: vec![
                AgentProfile {
                    launch: Launch {
                        command: vec!["claude".into(), "--model".into(), "opus".into()],
                        via_shell: true,
                        env: BTreeMap::from([("FOO".into(), "1".into())]),
                    },
                    options: AgentOptions::Claude(SessionOptions {
                        model: "opus".into(),
                        ..SessionOptions::default()
                    }),
                    home: Some("/Users/me/claude-work".into()),
                    ..AgentProfile::builtin(Agent::Claude)
                },
                AgentProfile {
                    id: "codex-review".parse().unwrap(),
                    name: "Review".into(),
                    ..AgentProfile::builtin(Agent::Codex)
                },
            ],
            unshared: Vec::new(),
            delegation: DelegationSettings { max_started: 8 },
            zoom: 1.1,
            prompt_min_rows: 2,
            prompt_max_rows: 20,
            font_family: "IntoneMono Nerd Font".into(),
            font_size: 14.0,
            font_weight: 300,
            appearance: Appearance::Dark,
            light_theme: "Catppuccin Latte".into(),
            dark_theme: "Nightfox".into(),
            theme: None,
            keep_awake: false,
            notify_waiting: true,
            shell: ShellLaunch {
                command: vec!["/bin/bash".into(), "-l".into()],
                env: BTreeMap::from([("LANG".into(), "C".into())]),
            },
            neovim: Launch {
                command: vec!["nvim".into(), "--listen".into(), "/tmp/n".into()],
                via_shell: false,
                env: BTreeMap::new(),
            },
            scratch_dir: "~/notes".into(),
            picker: PickerSettings {
                ghq: false,
                roots: vec!["~/src".into()],
            },
            console: ConsoleSettings {
                option_as_alt: OptionAsAlt::Right,
                cursor_blink: true,
            },
            usage: UsageSettings::default(),
            details: DetailsSettings {
                log_commits: 10,
                diff_lines: 300,
                links_kept: 20,
                links_shown: 3,
            },
        };
        assert_eq!(written_and_read(&s), s);
    }

    #[test]
    fn how_shells_and_claude_code_start_is_read_from_tables_of_their_own() {
        let s = Settings::parse(
            "[shell]\ncommand = [\"/bin/bash\", \"-l\"]\n[claude]\nvia_shell = true\n[claude.env]\nFOO = \"1\"\n",
        )
        .unwrap();
        assert_eq!(s.shell.command, ["/bin/bash", "-l"]);
        let claude = s.profile_of(Agent::Claude);
        assert!(claude.launch.via_shell);
        assert_eq!(claude.launch.env.get("FOO").map(String::as_str), Some("1"));
        assert_eq!(
            Settings::parse("")
                .unwrap()
                .profile_of(Agent::Claude)
                .launch,
            Launch::default()
        );
        let s = Settings::parse("[neovim]\ncommand = [\"/opt/nvim/bin/nvim\"]\nvia_shell = true\n")
            .unwrap();
        assert_eq!(s.neovim.command, ["/opt/nvim/bin/nvim"]);
        assert!(s.neovim.via_shell);
        assert!(Settings::parse("[claude]\nvia_shel = true\n").is_err());
    }

    #[test]
    fn a_command_keeps_no_empty_words() {
        let mut s = Settings {
            shell: ShellLaunch {
                command: vec!["  ".into(), " /bin/zsh ".into(), "".into()],
                ..ShellLaunch::default()
            },
            ..Settings::default()
        };
        s.profile_of_mut(Agent::Claude).launch.command = vec![" ".into()];
        let s = s.sane();
        assert_eq!(s.shell.command, ["/bin/zsh"]);
        assert!(s.profile_of(Agent::Claude).launch.command.is_empty());
    }

    #[test]
    fn the_file_read_again_is_taken_in_only_when_it_reads_and_says_something_new() {
        let now = Settings::default();
        let next = reread(&now, "zoom = 1.25\n").unwrap();
        assert_eq!(next.zoom, 1.25);
        // What adroit wrote itself, or a file caught half written, changes nothing.
        assert_eq!(reread(&now, &toml::to_string_pretty(&now).unwrap()), None);
        assert_eq!(reread(&now, "zoom = "), None);
        assert_eq!(reread(&now, "colour = \"red\"\n"), None);
    }

    #[test]
    fn the_picker_asks_ghq_and_lists_the_folders_the_settings_name() {
        let home = std::path::Path::new("/h");
        let s = Settings::parse("").unwrap();
        assert!(s.picker.ghq);
        assert!(s.picker.root_paths(home).is_empty());
        let s = Settings::parse("[picker]\nghq = false\nroots = [\"~/src\", \"/work\", \" \"]\n")
            .unwrap();
        assert!(!s.picker.ghq);
        assert_eq!(
            s.picker.root_paths(home),
            [PathBuf::from("/h/src"), PathBuf::from("/work")]
        );
    }

    #[test]
    fn option_counts_as_alt_and_the_cursor_holds_still_unless_the_settings_say_otherwise() {
        let s = Settings::parse("").unwrap();
        assert_eq!(s.console, ConsoleSettings::default());
        assert_eq!(s.console.option_as_alt, OptionAsAlt::Both);
        assert!(!s.console.cursor_blink);
        let s =
            Settings::parse("[console]\noption_as_alt = \"left\"\ncursor_blink = true\n").unwrap();
        assert_eq!(s.console.option_as_alt, OptionAsAlt::Left);
        assert!(s.console.cursor_blink);
        assert!(Settings::parse("[console]\noption_as_alt = \"up\"\n").is_err());
    }

    #[test]
    fn a_session_starting_to_wait_is_told_of_only_when_the_settings_ask() {
        assert!(!Settings::parse("").unwrap().notify_waiting);
        assert!(
            Settings::parse("notify_waiting = true\n")
                .unwrap()
                .notify_waiting
        );
    }

    #[test]
    fn the_details_panel_reads_as_much_as_the_settings_say_and_at_least_one() {
        let s = Settings::parse("").unwrap();
        assert_eq!(
            (
                s.details.log_commits,
                s.details.diff_lines,
                s.details.links_kept
            ),
            (50, 2000, 100)
        );
        assert_eq!(s.details.links_shown, 5);
        assert_eq!(
            Settings::parse("[details]\nlinks_shown = 0\n")
                .unwrap()
                .details
                .links_shown,
            1
        );
        let s = Settings::parse("[details]\nlog_commits = 0\ndiff_lines = 9\nlinks_kept = 5000\n")
            .unwrap();
        assert_eq!(
            (
                s.details.log_commits,
                s.details.diff_lines,
                s.details.links_kept
            ),
            (1, 9, 1000)
        );
    }

    const PROFILES: &str = r#"profile = "claude-work"

[[profiles]]
id = "claude"
name = "Claude Personal"
agent = "claude"

[[profiles]]
id = "claude-work"
name = "Claude Work"
agent = "claude"
home = "/Users/me/.local/share/adroit/profiles/claude-work"

[profiles.launch]
via_shell = true

[profiles.claude]
model = "opus"

[[profiles]]
id = "codex-review"
name = "Codex Review"
agent = "codex"

[profiles.codex]
sandbox_mode = "read-only"
"#;

    #[test]
    fn profiles_are_read_as_settings_toml_names_them() {
        let s = Settings::parse(PROFILES).unwrap();
        let ids: Vec<&str> = s.profiles.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["claude", "claude-work", "codex-review"]);
        assert_eq!(s.profile.as_str(), "claude-work");
        let work = &s.profiles[1];
        assert_eq!(work.label(), "Claude Work");
        assert_eq!(
            work.home.as_deref(),
            Some(Path::new(
                "/Users/me/.local/share/adroit/profiles/claude-work"
            ))
        );
        assert!(work.launch.via_shell);
        assert_eq!(work.claude().unwrap().model, "opus");
        assert_eq!(s.profiles[0].home, None);
        assert_eq!(s.profiles[2].codex().unwrap().sandbox_mode, "read-only");
        assert_eq!(written_and_read(&s), s);
    }

    /// A file as versions that kept logins in connections wrote it: two
    /// profiles share `work`, with one archived before them.
    const CONNECTIONS: &str = r#"profile = "claude-work"

[[profiles]]
id = "claude"
agent = "claude"

[[profiles]]
id = "old"
agent = "claude"
connection = "work"
archived = true

[[profiles]]
id = "claude-work"
name = "Claude Work"
agent = "claude"
connection = "work"

[[profiles]]
id = "claude-review"
name = "Claude Review"
agent = "claude"
connection = "work"

[[profiles]]
id = "pi-local"
agent = "pi"
connection = "local"

[[profiles]]
id = "codex"
agent = "codex"
connection = "codex"

[[connections]]
id = "work"
name = "Work"
agent = "claude"
home = "/Users/me/.adroit/claude-work"

[[connections]]
id = "local"
agent = "pi"
home = "/Users/me/.adroit/pi-local"

[[connections]]
id = "unused"
agent = "codex"
home = "/Users/me/.adroit/unused"
archived = true
"#;

    #[test]
    fn a_file_written_with_connections_gives_each_profile_its_connection_s_home() {
        let s = Settings::parse(CONNECTIONS).unwrap();
        let homes: Vec<(&str, Option<&Path>)> = s
            .profiles
            .iter()
            .map(|p| (p.id.as_str(), p.home.as_deref()))
            .collect();
        assert_eq!(
            homes,
            [
                ("claude", None),
                (
                    "claude-work",
                    Some(Path::new("/Users/me/.adroit/claude-work"))
                ),
                ("claude-review", None),
                ("pi-local", Some(Path::new("/Users/me/.adroit/pi-local"))),
                ("codex", None),
            ]
        );
    }

    #[test]
    fn of_profiles_sharing_a_connection_the_first_keeps_it_and_the_others_are_told_of() {
        let s = Settings::parse(CONNECTIONS).unwrap();
        assert_eq!(s.unshared, ["claude-review".parse::<ProfileId>().unwrap()]);
        assert_eq!(
            s.find_profile(&"claude-review".parse().unwrap())
                .unwrap()
                .home,
            None
        );
    }

    #[test]
    fn a_file_written_with_connections_is_written_with_homes_and_no_connections() {
        let earlier = Settings::parse(CONNECTIONS).unwrap();
        let written = toml::to_string_pretty(&earlier).unwrap();
        let table: toml::Table = written.parse().unwrap();
        assert!(!table.contains_key("connections"), "{written}");
        assert!(!written.contains("connection ="), "{written}");
        let again = Settings::parse(&written).unwrap();
        assert_eq!(again.profiles, earlier.profiles);
        // Once written, nothing is left to tell of.
        assert!(again.unshared.is_empty());
    }

    #[test]
    fn a_file_without_profiles_reads_as_the_profiles_adroit_comes_with() {
        let s = Settings::parse("").unwrap();
        assert_eq!(s, Settings::default());
        assert_eq!(s.profile, ProfileId::builtin(Agent::Claude));
        assert_eq!(s.profiles, Agent::ALL.map(AgentProfile::builtin));
        assert!(s.unshared.is_empty());
    }

    #[test]
    fn a_file_written_before_profiles_reads_its_claude_tables_as_the_claude_profile() {
        let s = Settings::parse(
            r#"[claude]
command = ["/opt/claude", "--verbose"]
via_shell = true
[claude.env]
FOO = "1"
[sessions]
model = "opus"
permission_mode = "plan"
"#,
        )
        .unwrap();
        assert_eq!(s.profile.as_str(), "claude");
        let claude = s.profile_of(Agent::Claude);
        assert_eq!(claude.id.as_str(), "claude");
        assert_eq!(claude.home, None);
        assert_eq!(claude.launch.command, ["/opt/claude", "--verbose"]);
        assert!(claude.launch.via_shell);
        assert_eq!(claude.launch.env.get("FOO").map(String::as_str), Some("1"));
        let options = claude.claude().unwrap();
        assert_eq!(
            (options.model.as_str(), options.permission_mode.as_str()),
            ("opus", "plan")
        );
        assert_eq!(
            s.profile_of(Agent::Codex).into_owned(),
            AgentProfile::builtin(Agent::Codex)
        );
    }

    #[test]
    fn a_file_written_before_profiles_is_written_with_profiles_and_reads_back_the_same() {
        let earlier = Settings::parse(
            "agent = \"codex\"\n[claude]\nvia_shell = true\n[codex_sessions]\nmodel = \"m\"\n",
        )
        .unwrap();
        let written = toml::to_string_pretty(&earlier).unwrap();
        let table: toml::Table = written.parse().unwrap();
        for key in ["agent", "claude", "sessions", "codex", "codex_sessions"] {
            assert!(!table.contains_key(key), "{key} written:\n{written}");
        }
        assert!(table.contains_key("profiles"), "{written}");
        let again = Settings::parse(&written).unwrap();
        assert_eq!(again, earlier);
        assert_eq!(again.profile, earlier.profile);
        // Written once more, the file stays as it is.
        assert_eq!(toml::to_string_pretty(&again).unwrap(), written);
    }

    #[test]
    fn a_file_naming_profiles_beside_the_earlier_agent_tables_is_refused_naming_them() {
        let error = Settings::parse(&format!("{PROFILES}\n[sessions]\nmodel = \"opus\"\n"))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("`profiles` cannot be named beside `sessions`"),
            "{error}"
        );
        let error =
            Settings::parse("profile = \"codex\"\nagent = \"codex\"\n[codex]\nvia_shell = true\n")
                .unwrap_err()
                .to_string();
        assert!(
            error.contains("`profile` cannot be named beside `agent`, `codex`"),
            "{error}"
        );
        let error = Settings::parse(
            "[claude]\nvia_shell = true\n[[connections]]\nid = \"work\"\nagent = \"claude\"\nhome = \"/w\"\n",
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("`connections` cannot be named beside `claude`"),
            "{error}"
        );
        // The profile alone picks among the profiles adroit comes with.
        let s = Settings::parse("profile = \"codex\"\n").unwrap();
        assert_eq!(s.default_agent(), Agent::Codex);
    }

    #[test]
    fn settings_whose_profiles_or_connections_do_not_hold_together_are_refused() {
        let profile = |id: &str, agent: &str, rest: &str| {
            format!("[[profiles]]\nid = \"{id}\"\nagent = \"{agent}\"\n{rest}\n")
        };
        let connection = |id: &str, agent: &str, home: &str| {
            format!("[[connections]]\nid = \"{id}\"\nagent = \"{agent}\"\nhome = \"{home}\"\n")
        };
        let cases = [
            (
                profile("claude", "claude", "") + &profile("claude", "claude", ""),
                "profile `claude` is defined twice",
            ),
            (
                profile("Claude Work", "claude", ""),
                "`Claude Work` is not a valid id",
            ),
            (
                profile("claude", "claude", "connection = \"gone\""),
                "profile `claude` names connection `gone`, which is not defined",
            ),
            (
                profile("claude", "claude", "connection = \"work\"")
                    + &connection("work", "codex", "/w"),
                "profile `claude` starts Claude Code but connection `work` belongs to Codex",
            ),
            (
                profile("claude", "claude", "home = \"/w\"")
                    + &profile("codex", "codex", "home = \"/w\""),
                "profiles `claude` and `codex` both keep their login in /w",
            ),
            (
                profile("claude", "claude", "home = \"~/w\""),
                "profile `claude` needs an absolute path as its home",
            ),
            (
                profile("claude", "claude", "home = \"/h\"\nconnection = \"work\"")
                    + &connection("work", "claude", "/w"),
                "profile `claude` names both `connection` and `home`",
            ),
            (
                profile("claude", "claude", "[profiles.codex]\nmodel = \"m\""),
                "profile `claude` starts Claude Code and cannot hold options for Codex",
            ),
            (
                profile("claude", "claude", "") + &connection("claude", "claude", "/w"),
                "connection id `claude` is reserved for Claude Code's own environment",
            ),
            (
                profile("claude", "claude", "") + &connection("work", "claude", "w"),
                "connection `work` needs an absolute path as its home",
            ),
            (
                profile("claude", "claude", "")
                    + &connection("work", "claude", "/w")
                    + &connection("work", "claude", "/v"),
                "connection `work` is defined twice",
            ),
            (
                format!("profile = \"other\"\n{}", profile("claude", "claude", "")),
                "profile `other` is not defined",
            ),
            (
                format!(
                    "profile = \"old\"\n{}{}",
                    profile("claude", "claude", ""),
                    profile("old", "claude", "archived = true")
                ),
                "profile `old` is not defined",
            ),
            (
                profile("old", "claude", "archived = true"),
                "the settings hold no profile",
            ),
        ];
        for (text, why) in cases {
            let error = Settings::parse(&text).unwrap_err().to_string();
            assert!(error.contains(why), "{text}\n{error}\ndoes not say {why}");
        }
    }

    #[test]
    fn with_no_profile_named_the_first_profile_is_the_default() {
        let s = Settings::parse(
            "[[profiles]]\nid = \"review\"\nagent = \"codex\"\n[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n",
        )
        .unwrap();
        assert_eq!(s.profile.as_str(), "review");
        assert_eq!(s.default_agent(), Agent::Codex);
    }

    #[test]
    fn a_profile_written_archived_is_read_as_if_it_were_not_there_and_dropped_when_written() {
        let s = Settings::parse(
            "[[profiles]]\nid = \"old\"\nagent = \"claude\"\nhome = \"/o\"\narchived = true\n[[profiles]]\nid = \"review\"\nagent = \"codex\"\n[[profiles]]\nid = \"new\"\nagent = \"claude\"\nhome = \"/o\"\n",
        )
        .unwrap();
        let ids: Vec<&str> = s.profiles.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["review", "new"]);
        assert_eq!(s.profile.as_str(), "review");
        assert_eq!(s.find_profile(&"old".parse().unwrap()), None);
        let written = toml::to_string_pretty(&s).unwrap();
        assert!(!written.contains("old"), "{written}");
        assert!(!written.contains("archived"), "{written}");
    }

    #[test]
    fn an_agent_starts_from_the_default_profile_when_that_is_its_own_else_from_its_first() {
        let s = Settings::parse(PROFILES).unwrap();
        assert_eq!(s.profile_of(Agent::Claude).id.as_str(), "claude-work");
        assert_eq!(s.profile_of(Agent::Codex).id.as_str(), "codex-review");
        let s = Settings {
            profile: "codex-review".parse().unwrap(),
            ..s
        };
        assert_eq!(s.profile_of(Agent::Claude).id.as_str(), "claude");
        let mut s = s;
        s.profiles.remove(0);
        assert_eq!(s.profile_of(Agent::Claude).id.as_str(), "claude-work");
    }

    #[test]
    fn a_profile_is_found_by_its_id_and_adroit_s_own_stands_for_an_agent_given_none() {
        let s = Settings::parse(
            "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n[[profiles]]\nid = \"claude-work\"\nagent = \"claude\"\n",
        )
        .unwrap();
        let found =
            |s: &Settings, id: &str| s.find_profile(&id.parse().unwrap()).map(Cow::into_owned);
        assert_eq!(found(&s, "claude-work").unwrap().id.as_str(), "claude-work");
        assert_eq!(
            found(&s, "codex"),
            Some(AgentProfile::builtin(Agent::Codex))
        );
        assert_eq!(
            found(&s, "codex"),
            Some(s.profile_of(Agent::Codex).into_owned())
        );
        assert_eq!(found(&s, "gone"), None);
        // Once the agent has a profile of its own, adroit's stands for none.
        let s = Settings::parse(
            "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n[[profiles]]\nid = \"codex-review\"\nagent = \"codex\"\n",
        )
        .unwrap();
        assert_eq!(found(&s, "codex"), None);
    }

    #[test]
    fn adroit_s_own_profile_takes_an_id_no_profile_holds_so_it_is_found_as_offered() {
        let found = |s: &Settings, id: &ProfileId| s.find_profile(id).map(Cow::into_owned).unwrap();
        // The agent's key held by a profile of another agent.
        let s = Settings::parse(
            "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n[[profiles]]\nid = \"codex\"\nagent = \"claude\"\n",
        )
        .unwrap();
        let offered = s.profile_of(Agent::Codex).into_owned();
        assert_eq!(offered.id.as_str(), "codex-2");
        assert_eq!(found(&s, &offered.id), offered);
        assert_eq!(
            found(&s, &"codex".parse().unwrap()).id.as_str(),
            "codex",
            "the profile that holds the key is still found by it"
        );
    }

    #[test]
    fn a_session_is_shown_by_its_agent_and_a_profile_of_another_name_by_that_too() {
        let s = Settings::parse(PROFILES).unwrap();
        let shown = |agent, profile: &str, home: Option<&str>| {
            s.started_as(agent, &profile.parse().unwrap(), home.map(Path::new), "")
        };
        assert_eq!(
            shown(
                Agent::Claude,
                "claude-work",
                Some("/Users/me/.local/share/adroit/profiles/claude-work")
            ),
            "Claude Code · Claude Work"
        );
        assert_eq!(
            shown(Agent::Claude, "claude", None),
            "Claude Code · Claude Personal"
        );
        assert_eq!(shown(Agent::Codex, "gone", None), "Codex · gone");
        let own = Settings::default();
        assert_eq!(
            own.started_as(Agent::Codex, &"codex".parse().unwrap(), None, ""),
            "Codex"
        );
        // One started before profiles, from a profile since removed.
        let none = Settings::parse("[[profiles]]\nid = \"mine\"\nagent = \"claude\"\n").unwrap();
        assert_eq!(
            none.started_as(Agent::Codex, &"codex".parse().unwrap(), None, ""),
            "Codex"
        );
        // Adroit's own, numbered past a profile of another agent that holds
        // the key.
        let keyed = Settings::parse(
            "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n[[profiles]]\nid = \"codex\"\nname = \"Old\"\nagent = \"claude\"\n",
        )
        .unwrap();
        assert_eq!(
            keyed.started_as(Agent::Codex, &"codex-2".parse().unwrap(), None, ""),
            "Codex"
        );
    }

    #[test]
    fn a_session_of_a_profile_deleted_since_is_named_as_its_profile_was_called_when_it_started() {
        let s = Settings::parse(PROFILES).unwrap();
        let work = "claude-work".parse().unwrap();
        let shown = |s: &Settings, home: Option<&str>| {
            s.started_as(Agent::Claude, &work, home.map(Path::new), "Old Work")
        };
        let home = Some("/Users/me/.local/share/adroit/profiles/claude-work");
        // Still held, it goes by the name it has now.
        assert_eq!(shown(&s, home), "Claude Code · Claude Work");
        let mut deleted = s.clone();
        deleted.profiles.retain(|p| p.id != work);
        assert_eq!(shown(&deleted, home), "Claude Code · Old Work");
        // Its id taken again by a profile of another login is not it.
        assert_eq!(
            shown(&s, Some("/Users/me/old-work")),
            "Claude Code · Old Work"
        );
        // Not known, it goes by its id.
        assert_eq!(
            deleted.started_as(Agent::Claude, &work, home.map(Path::new), ""),
            "Claude Code · claude-work"
        );
    }

    #[test]
    fn an_agent_without_a_profile_starts_as_adroit_s_own_and_is_given_one_when_edited() {
        let mut s = Settings::parse("[[profiles]]\nid = \"codex\"\nagent = \"claude\"\n").unwrap();
        assert_eq!(
            s.profile_of(Agent::Codex).into_owned().launch,
            Launch::default()
        );
        assert_eq!(s.profile_of(Agent::Codex).agent(), Agent::Codex);
        s.profile_of_mut(Agent::Codex).launch.via_shell = true;
        let added = s.profiles.last().unwrap();
        // The agent's key is taken, so the profile adroit adds is numbered.
        assert_eq!(
            (added.id.as_str(), added.agent()),
            ("codex-2", Agent::Codex)
        );
        assert!(
            written_and_read(&s)
                .profile_of(Agent::Codex)
                .launch
                .via_shell
        );
    }

    #[test]
    fn a_launch_kept_as_json_is_read_and_checked_as_the_file_is() {
        let s = Settings::parse(PROFILES).unwrap();
        let kept = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&kept).unwrap(), s);
        let mut broken: serde_json::Value = serde_json::from_str(&kept).unwrap();
        broken["profile"] = "gone".into();
        let error = serde_json::from_value::<Settings>(broken)
            .unwrap_err()
            .to_string();
        assert!(error.contains("profile `gone` is not defined"), "{error}");
    }

    #[test]
    fn a_launch_kept_as_json_before_profiles_reads_as_the_profiles_built_from_it() {
        let kept = r#"{"agent":"codex","codex":{"command":["/opt/codex"],"via_shell":false,"env":{}},
            "codex_sessions":{"model":"m","reasoning_effort":"","approval_policy":"","sandbox_mode":""},
            "delegation":{"max_started":4},"zoom":1.0,"prompt_min_rows":3,"prompt_max_rows":12,
            "font_family":"","font_size":13.0,"font_weight":400,"appearance":"system",
            "light_theme":"Dawnfox","dark_theme":"Nightfox","keep_awake":true,"notify_waiting":false,
            "shell":{"command":[],"env":{}},"claude":{"command":[],"via_shell":true,"env":{}},
            "neovim":{"command":[],"via_shell":false,"env":{}},
            "sessions":{"model":"opus","permission_mode":"","disallowed_tools":[],"confirm_channel_warning":false},
            "scratch_dir":"","picker":{"ghq":true,"roots":[]},
            "console":{"option_as_alt":"both","cursor_blink":false},
            "details":{"log_commits":50,"diff_lines":2000,"links_kept":100,"links_shown":5},
            "usage":{"cpu":true,"gpu":true,"memory":true,"storage":true}}"#;
        let s: Settings = serde_json::from_str(kept).unwrap();
        assert_eq!(s.default_agent(), Agent::Codex);
        assert_eq!(s.profile_of(Agent::Codex).launch.command, ["/opt/codex"]);
        assert_eq!(s.profile_of(Agent::Codex).codex().unwrap().model, "m");
        let claude = s.profile_of(Agent::Claude);
        assert!(claude.launch.via_shell);
        assert!(!claude.claude().unwrap().confirm_channel_warning);
    }
}
