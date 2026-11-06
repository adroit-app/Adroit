//! What the Neovims a session runs are given to draw in the theme in force:
//! the colorscheme adroit ships, the data folder it is found through, and
//! the file of the theme's colours it reads.

use std::io::Write;
use std::path::{Path, PathBuf};

use adroit_term::Palette;

use crate::paths::state_dir;

/// The colorscheme, the same for every theme: what it draws is read from the
/// theme's file as Neovim loads it.
const COLORSCHEME: &str = include_str!("../nvim/colors/adroit.lua");

/// Names the file of the theme's colours in a session's environment.
pub const THEME_FILE: &str = "ADROIT_THEME_FILE";

/// The data folder a session's Neovim finds the colorscheme through.
pub fn data_folder() -> PathBuf {
    state_dir().join("adroit").join("share")
}

/// Put the colorscheme into the data folder `dir`, where Neovim looks.
pub fn install_colorscheme(dir: &Path) -> std::io::Result<()> {
    replace(
        &dir.join("nvim")
            .join("site")
            .join("colors")
            .join("adroit.lua"),
        COLORSCHEME,
    )
}

/// The `XDG_DATA_DIRS` a session starts with: `dir` first, then what adroit
/// was given, or else the folders the XDG specification names for none, so
/// the session's other programs still find theirs.
pub fn data_dirs(dir: &Path, given: Option<&str>) -> String {
    let (separator, none) = if cfg!(windows) {
        (";", None)
    } else {
        (":", Some("/usr/local/share:/usr/share"))
    };
    match given.filter(|g| !g.is_empty()).or(none) {
        Some(rest) => format!("{}{separator}{rest}", dir.display()),
        None => dir.display().to_string(),
    }
}

/// The file of the colours of the theme in force in the environment.
pub fn theme_file() -> PathBuf {
    crate::paths::instance_state().join("theme").join("colours")
}

/// Write `palette` to `file`, replacing it whole so a Neovim reading it never
/// sees it half written.
pub fn write_theme(file: &Path, palette: &Palette) -> std::io::Result<()> {
    replace(file, &palette.to_ghostty())
}

fn replace(file: &Path, text: &str) -> std::io::Result<()> {
    let dir = file.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut staged = tempfile::NamedTempFile::new_in(dir)?;
    staged.write_all(text.as_bytes())?;
    staged.persist(file).map_err(|e| e.error)?;
    Ok(())
}

/// What every session's environment holds for its Neovims.
pub fn environment() -> Vec<(String, String)> {
    let given = std::env::var("XDG_DATA_DIRS").ok();
    vec![
        (
            "XDG_DATA_DIRS".into(),
            data_dirs(&data_folder(), given.as_deref()),
        ),
        (
            THEME_FILE.into(),
            theme_file().to_string_lossy().into_owned(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_term::Rgb;

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Windows lists folders with `;` and names no default"
    )]
    fn sessions_look_in_adroit_s_data_folder_first_then_where_they_would_anyway() {
        let dir = Path::new("/s/share");
        assert_eq!(data_dirs(dir, None), "/s/share:/usr/local/share:/usr/share");
        assert_eq!(
            data_dirs(dir, Some("")),
            "/s/share:/usr/local/share:/usr/share"
        );
        assert_eq!(data_dirs(dir, Some("/opt/share")), "/s/share:/opt/share");
    }

    #[test]
    fn the_colorscheme_is_put_where_neovim_looks_in_a_data_folder() {
        let dir = tempfile::tempdir().unwrap();
        install_colorscheme(dir.path()).unwrap();
        let file = dir.path().join("nvim/site/colors/adroit.lua");
        assert_eq!(std::fs::read_to_string(file).unwrap(), COLORSCHEME);
    }

    #[test]
    fn the_theme_file_holds_the_palette_and_is_replaced_whole() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("theme").join("colours");
        write_theme(&file, &Palette::default()).unwrap();
        let light = Palette {
            background: Rgb::new(0xff, 0xff, 0xff),
            ..Palette::default()
        };
        write_theme(&file, &light).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert_eq!(Palette::from_ghostty(&text).unwrap(), light);
        let left: Vec<_> = std::fs::read_dir(file.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, ["colours"]);
    }

    #[tokio::test]
    async fn each_environment_has_a_theme_file_of_its_own() {
        use crate::environment::{Environment, run};
        let root = tempfile::tempdir().unwrap();
        let work = Environment::at("work", root.path()).unwrap();
        let personal = Environment::at("personal", root.path()).unwrap();
        let of = |e| run(Some(e), async { theme_file() });
        let (work, personal) = (of(work).await, of(personal).await);
        assert_ne!(work, personal);
        assert!(work.starts_with(root.path()), "{}", work.display());
    }

    #[test]
    fn every_session_is_told_the_theme_file_and_adroit_s_data_folder() {
        let env = environment();
        let get = |key: &str| {
            env.iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert_eq!(get(THEME_FILE), theme_file().to_string_lossy());
        let first = data_folder();
        assert!(get("XDG_DATA_DIRS").starts_with(&*first.to_string_lossy()));
    }

    /// Has the Neovim it runs in choose the colorscheme, then has the work
    /// host's next theme replace the file, as the work host does, and then
    /// a third once another colorscheme is chosen; writes what each drew.
    const FOLLOWS: &str = r#"
local function drawn()
  local s = vim.api.nvim_get_hl(0, { name = 'String', link = false })
  local n = vim.api.nvim_get_hl(0, { name = 'Normal', link = false })
  return string.format('%s %s %06x %06x %06x', vim.g.colors_name, vim.o.background, s.fg or 0, n.fg or 0, n.bg or 0)
end
local function replace(by)
  local file = vim.env.ADROIT_THEME_FILE
  vim.fn.writefile(vim.fn.readfile(by), file .. '.next')
  assert(vim.uv.fs_rename(file .. '.next', file))
end
vim.cmd.colorscheme('adroit')
local seen = { drawn() }
replace(vim.env.NEXT)
vim.wait(5000, function() return drawn() ~= seen[1] end)
seen[#seen + 1] = drawn()
vim.cmd.colorscheme('default')
local chosen = drawn()
replace(vim.env.FIRST)
vim.wait(500, function() return drawn() ~= chosen end)
seen[#seen + 1] = drawn() == chosen and 'still default' or drawn()
io.stdout:write(table.concat(seen, '\n'), '\n')
"#;

    fn drawn(palette: &Palette, string: usize) -> String {
        let kind = if palette.is_dark() { "dark" } else { "light" };
        let hex = |c: Rgb| format!("{:02x}{:02x}{:02x}", c.r, c.g, c.b);
        format!(
            "adroit {kind} {} {} {}",
            hex(palette.ansi[string]),
            hex(palette.foreground),
            hex(palette.background)
        )
    }

    #[test]
    fn a_neovim_that_chooses_adroit_draws_in_the_theme_and_follows_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let share = dir.path().join("share");
        install_colorscheme(&share).unwrap();
        let nightfox = Palette::from_ghostty(include_str!("../themes/Nightfox")).unwrap();
        let dawnfox = Palette::from_ghostty(include_str!("../themes/Dawnfox")).unwrap();
        let (first, next) = (dir.path().join("first"), dir.path().join("next"));
        std::fs::write(&first, nightfox.to_ghostty()).unwrap();
        std::fs::write(&next, dawnfox.to_ghostty()).unwrap();
        let file = dir.path().join("theme").join("colours");
        write_theme(&file, &nightfox).unwrap();
        let script = dir.path().join("follows.lua");
        std::fs::write(&script, FOLLOWS).unwrap();
        let out = std::process::Command::new(adroit_nvim::program())
            .args(["--headless", "--clean", "-l"])
            .arg(&script)
            .env("XDG_DATA_DIRS", data_dirs(&share, None))
            .env("XDG_CONFIG_HOME", dir.path().join("config"))
            .env("XDG_DATA_HOME", dir.path().join("data"))
            .env("XDG_STATE_HOME", dir.path().join("state"))
            .env(THEME_FILE, &file)
            .env("FIRST", &first)
            .env("NEXT", &next)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = stdout.lines().collect();
        // A dark theme draws strings in its bright green, a light one in its
        // green, as Neovim's default does in a terminal's colours.
        assert_eq!(
            lines,
            [
                drawn(&nightfox, 10).as_str(),
                drawn(&dawnfox, 2).as_str(),
                "still default"
            ],
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    const WAITS: &str = r#"
vim.cmd.colorscheme('adroit')
local file = vim.env.ADROIT_THEME_FILE
vim.fn.mkdir(vim.fs.dirname(file), 'p')
vim.fn.writefile(vim.fn.readfile(vim.env.FIRST), file)
local function drawn()
  local s = vim.api.nvim_get_hl(0, { name = 'String', link = false })
  local n = vim.api.nvim_get_hl(0, { name = 'Normal', link = false })
  return string.format('%s %s %06x %06x %06x', vim.g.colors_name, vim.o.background, s.fg or 0, n.fg or 0, n.bg or 0)
end
local before = drawn()
vim.wait(5000, function() return drawn() ~= before end)
io.stdout:write(drawn(), '\n')
"#;

    #[test]
    fn a_neovim_that_chooses_adroit_before_the_theme_s_folder_is_made_draws_it_once_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let share = dir.path().join("share");
        install_colorscheme(&share).unwrap();
        let nightfox = Palette::from_ghostty(include_str!("../themes/Nightfox")).unwrap();
        let first = dir.path().join("first");
        std::fs::write(&first, nightfox.to_ghostty()).unwrap();
        let script = dir.path().join("waits.lua");
        std::fs::write(&script, WAITS).unwrap();
        let out = std::process::Command::new(adroit_nvim::program())
            .args(["--headless", "--clean", "-l"])
            .arg(&script)
            .env("XDG_DATA_DIRS", data_dirs(&share, None))
            .env("XDG_CONFIG_HOME", dir.path().join("config"))
            .env("XDG_DATA_HOME", dir.path().join("data"))
            .env("XDG_STATE_HOME", dir.path().join("state"))
            .env(THEME_FILE, dir.path().join("theme").join("colours"))
            .env("FIRST", &first)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim_end(),
            drawn(&nightfox, 10),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
