//! The window's settings, kept in `settings.toml` in adroit's configuration
//! folder, and told to every window as they change.

use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::paths::config_dir;
pub use crate::programs::{Launch, ReaderOptions, SessionOptions, ShellLaunch};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// How far the window is zoomed when it opens.
    pub zoom: f64,
    /// The fewest lines the prompt shows.
    pub prompt_min_rows: usize,
    /// The most lines the prompt grows to before it scrolls inside.
    pub prompt_max_rows: usize,
    /// The font family the console and the prompt are drawn in, ahead of
    /// the built-in ones; empty for those alone.
    pub font_family: String,
    /// Their font's size, in points.
    pub font_size: f64,
    /// Their font's weight, from 100 to 900.
    pub font_weight: u16,
    /// The console's colour theme, by name: one adroit comes with, or one
    /// in the themes folder.
    pub theme: String,
    /// Keep the Mac from idle sleep while a session or a shell runs.
    pub keep_awake: bool,
    /// How a shell tab's shell is started.
    pub shell: ShellLaunch,
    /// How Claude Code is started.
    pub claude: Launch,
    /// How a Neovim tab's Neovim is started.
    pub neovim: Launch,
    /// How every session but a reader runs.
    pub sessions: SessionOptions,
    /// How a reader runs.
    pub readers: ReaderOptions,
    /// Where scratch sessions run; empty for adroit's own folder.
    pub scratch_dir: String,
    /// Where the picker looks for directories to start a session in.
    pub picker: PickerSettings,
}

/// The fonts the console falls back to, a Japanese one among them for what a
/// monospace font leaves out.
pub const MONO: &str = r#""SF Mono", Menlo, ui-monospace, "Hiragino Kaku Gothic ProN", monospace"#;

impl Default for Settings {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            prompt_min_rows: 3,
            prompt_max_rows: 12,
            font_family: String::new(),
            font_size: 13.0,
            font_weight: 400,
            theme: crate::themes::DEFAULT.into(),
            keep_awake: true,
            shell: ShellLaunch::default(),
            claude: Launch::default(),
            neovim: Launch::default(),
            sessions: SessionOptions::default(),
            readers: ReaderOptions::default(),
            scratch_dir: String::new(),
            picker: PickerSettings::default(),
        }
    }
}

/// Where the picker looks for directories to start a session in, beside where
/// sessions run and Claude Code has been used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PickerSettings {
    /// Offer the repositories ghq keeps, and its roots in the file tree.
    pub ghq: bool,
    /// Folders whose folders are offered, and which the file tree lists.
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
fn expand(path: &str, home: &std::path::Path) -> PathBuf {
    match path.strip_prefix('~') {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None => PathBuf::from(path),
    }
}

/// Settings earlier versions wrote, for projects adroit no longer keeps:
/// dropped as they are read, so a file they are in still reads.
const RETIRED: &[&str] = &["naming_model", "projects_dir"];

impl Settings {
    /// The settings made sensible: rows of at least one and the most no fewer
    /// than the fewest, a zoom among the levels' range.
    pub fn sane(mut self) -> Self {
        self.prompt_min_rows = self.prompt_min_rows.clamp(1, 40);
        self.prompt_max_rows = self.prompt_max_rows.clamp(self.prompt_min_rows, 60);
        let (least, most) = (
            crate::zoom::LEVELS[0],
            crate::zoom::LEVELS[crate::zoom::LEVELS.len() - 1],
        );
        self.zoom = self.zoom.clamp(least, most);
        self.font_family = self.font_family.trim().to_string();
        self.theme = self.theme.trim().to_string();
        if self.theme.is_empty() {
            self.theme = crate::themes::DEFAULT.into();
        }
        self.font_size = self.font_size.clamp(8.0, 32.0);
        self.font_weight = (self.font_weight.clamp(100, 900) + 50) / 100 * 100;
        let words = |c: Vec<String>| -> Vec<String> {
            c.into_iter()
                .map(|w| w.trim().to_string())
                .filter(|w| !w.is_empty())
                .collect()
        };
        self.shell.command = words(self.shell.command);
        self.claude.command = words(self.claude.command);
        self.neovim.command = words(self.neovim.command);
        self
    }

    /// Where scratch sessions run, `~` standing for `home`.
    pub fn scratch_folder(&self, home: &std::path::Path) -> PathBuf {
        match self.scratch_dir.trim() {
            "" => crate::paths::data_dir().join("adroit").join("scratch"),
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

    pub fn path() -> PathBuf {
        config_dir().join("settings.toml")
    }

    /// Where the user's own themes are, one file a theme, named as it is.
    pub fn themes_dir() -> PathBuf {
        config_dir().join("themes")
    }

    /// The settings in `text`; what it does not name is as designed.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        let mut table: toml::Table = text.parse()?;
        for key in RETIRED {
            table.remove(*key);
        }
        Settings::deserialize(table).map(Settings::sane)
    }

    /// The settings kept, or as designed when there are none or they cannot
    /// be read.
    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|t| Self::parse(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }
}

/// The settings every window works by.
pub static SETTINGS: LazyLock<RwLock<Settings>> = LazyLock::new(|| RwLock::new(Settings::load()));

/// Told whenever [`SETTINGS`] changes.
pub static CHANGED: LazyLock<broadcast::Sender<()>> = LazyLock::new(|| broadcast::channel(16).0);

pub fn current() -> Settings {
    SETTINGS.read().map(|s| s.clone()).unwrap_or_default()
}

/// The settings `text`, the file read again, holds when it reads and differs
/// from `current`; `None` otherwise, so a file half written or one adroit just
/// wrote itself changes nothing.
pub fn reread(current: &Settings, text: &str) -> Option<Settings> {
    Settings::parse(text).ok().filter(|s| s != current)
}

/// Take in the file's settings whenever it changes on disk, edited by hand or
/// by another program, looking every `every`.
pub fn watch(every: std::time::Duration) {
    let modified = || {
        std::fs::metadata(Settings::path())
            .and_then(|m| m.modified())
            .ok()
    };
    std::thread::spawn(move || {
        let mut seen = modified();
        loop {
            std::thread::sleep(every);
            let now = modified();
            if now == seen {
                continue;
            }
            seen = now;
            let Ok(text) = std::fs::read_to_string(Settings::path()) else {
                continue;
            };
            if let Some(next) = reread(&current(), &text) {
                if let Ok(mut s) = SETTINGS.write() {
                    *s = next;
                }
                let _ = CHANGED.send(());
            }
        }
    });
}

/// Make `next` the settings: keep them for the next run and tell every window.
pub fn set(next: Settings) -> Result<(), String> {
    let next = next.sane();
    next.save()?;
    if let Ok(mut s) = SETTINGS.write() {
        *s = next;
    }
    let _ = CHANGED.send(());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

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
            theme: " ".into(),
            keep_awake: true,
            ..Settings::default()
        }
        .sane();
        assert_eq!(s.zoom, 1.75);
        assert_eq!((s.prompt_min_rows, s.prompt_max_rows), (1, 1));
        assert_eq!(s.font_family, "");
        assert_eq!(s.font_size, 32.0);
        assert_eq!(s.font_weight, 900);
        assert_eq!(s.theme, crate::themes::DEFAULT);
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
    fn settings_written_read_back_the_same() {
        let s = Settings {
            zoom: 1.1,
            prompt_min_rows: 2,
            prompt_max_rows: 20,
            font_family: "IntoneMono Nerd Font".into(),
            font_size: 14.0,
            font_weight: 300,
            theme: "Nightfox".into(),
            keep_awake: false,
            shell: ShellLaunch {
                command: vec!["/bin/bash".into(), "-l".into()],
                env: BTreeMap::from([("LANG".into(), "C".into())]),
            },
            claude: Launch {
                command: vec!["claude".into(), "--model".into(), "opus".into()],
                via_shell: true,
                env: BTreeMap::from([("FOO".into(), "1".into())]),
            },
            neovim: Launch {
                command: vec!["nvim".into(), "--listen".into(), "/tmp/n".into()],
                via_shell: false,
                env: BTreeMap::new(),
            },
            sessions: SessionOptions {
                model: "opus".into(),
                ..SessionOptions::default()
            },
            readers: ReaderOptions::default(),
            scratch_dir: "~/notes".into(),
            picker: PickerSettings {
                ghq: false,
                roots: vec!["~/src".into()],
            },
        };
        let text = toml::to_string_pretty(&s).unwrap();
        assert_eq!(Settings::parse(&text).unwrap(), s);
    }

    #[test]
    fn how_shells_and_claude_code_start_is_read_from_tables_of_their_own() {
        let s = Settings::parse(
            "[shell]\ncommand = [\"/bin/bash\", \"-l\"]\n[claude]\nvia_shell = true\n[claude.env]\nFOO = \"1\"\n",
        )
        .unwrap();
        assert_eq!(s.shell.command, ["/bin/bash", "-l"]);
        assert!(s.claude.via_shell);
        assert_eq!(s.claude.env.get("FOO").map(String::as_str), Some("1"));
        assert_eq!(Settings::parse("").unwrap().claude, Launch::default());
        let s = Settings::parse("[neovim]\ncommand = [\"/opt/nvim/bin/nvim\"]\nvia_shell = true\n")
            .unwrap();
        assert_eq!(s.neovim.command, ["/opt/nvim/bin/nvim"]);
        assert!(s.neovim.via_shell);
        assert!(Settings::parse("[claude]\nvia_shel = true\n").is_err());
    }

    #[test]
    fn a_command_keeps_no_empty_words() {
        let s = Settings {
            shell: ShellLaunch {
                command: vec!["  ".into(), " /bin/zsh ".into(), "".into()],
                ..ShellLaunch::default()
            },
            claude: Launch {
                command: vec![" ".into()],
                ..Launch::default()
            },
            ..Settings::default()
        }
        .sane();
        assert_eq!(s.shell.command, ["/bin/zsh"]);
        assert!(s.claude.command.is_empty());
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
    fn sessions_and_readers_run_with_the_model_mode_and_tools_the_settings_name() {
        let s = Settings::parse("").unwrap();
        assert_eq!(s.sessions, SessionOptions::default());
        // A reader plans and edits nothing unless told otherwise.
        assert_eq!(s.readers.permission_mode, "plan");
        assert_eq!(
            s.readers.disallowed_tools,
            ["Edit", "Write", "NotebookEdit"]
        );
        let s = Settings::parse(
            "[sessions]\nmodel = \"opus\"\npermission_mode = \"acceptEdits\"\n[readers]\ndisallowed_tools = []\n",
        )
        .unwrap();
        assert_eq!(
            s.sessions.options().args(),
            ["--model", "opus", "--permission-mode", "acceptEdits"]
        );
        assert_eq!(s.readers.options().args(), ["--permission-mode", "plan"]);
    }

    #[test]
    fn scratch_sessions_run_in_the_folder_the_settings_name_or_adroit_s_own() {
        let home = std::path::Path::new("/h");
        let at = |text: &str| Settings::parse(text).unwrap().scratch_folder(home);
        assert_eq!(
            at(""),
            crate::paths::data_dir().join("adroit").join("scratch")
        );
        assert_eq!(
            at("scratch_dir = \"~/notes/scratch\"\n"),
            PathBuf::from("/h/notes/scratch")
        );
        assert_eq!(at("scratch_dir = \"/tmp/s\"\n"), PathBuf::from("/tmp/s"));
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
    fn the_specification_s_settings_toml_reads_as_the_settings_as_designed() {
        let spec = include_str!("../../../docs/spec/settings.md");
        let block = spec
            .split_once("```toml\n")
            .and_then(|(_, rest)| rest.split_once("```"))
            .map(|(toml, _)| toml)
            .unwrap();
        assert_eq!(Settings::parse(block).unwrap(), Settings::default());
    }
}
