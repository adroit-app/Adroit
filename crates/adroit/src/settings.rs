//! The window's settings, kept in `settings.toml` in adroit's configuration
//! folder, and told to every window as they change.

use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::paths::config_dir;

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
        }
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
        self
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

    #[test]
    fn settings_not_named_are_as_designed() {
        let s = Settings::parse("zoom = 1.25\n").unwrap();
        assert_eq!(s.zoom, 1.25);
        assert_eq!(s.prompt_min_rows, 3);
        assert!(Settings::parse("colour = \"red\"\n").is_err());
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
        };
        let text = toml::to_string_pretty(&s).unwrap();
        assert_eq!(Settings::parse(&text).unwrap(), s);
    }
}
