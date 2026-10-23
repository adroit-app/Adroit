//! Turns the file tree's icon tables into Rust, and tells the build which
//! source it was made from, for the daemon's build stamp and a debug
//! build's window title.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let icons = manifest.join("../../assets/file-icons");
    // Naming a path stops cargo from rerunning on any change in the package,
    // which the description below relies on, so the package is named too.
    println!("cargo:rerun-if-changed={}", icons.display());
    println!("cargo:rerun-if-changed={}", manifest.display());
    file_icons(&icons);
    describe();
}

fn file_icons(dir: &Path) {
    let nerdfont = dir.join("vim-nerdfont");
    let mut out = String::new();
    for (name, file) in [
        ("BASENAME", "basename.json"),
        ("EXTENSION", "extension.json"),
        ("DIRECTORY", "directory.json"),
    ] {
        let table = read_json(&nerdfont.join(file));
        let rows: String = table
            .iter()
            .map(|(k, v)| format!("({k:?}, {v:?}),"))
            .collect();
        writeln!(out, "static {name}: &[(&str, &str)] = &[{rows}];").unwrap();
    }
    let patterns = read_json(&nerdfont.join("pattern.json"));
    let rows: String = patterns
        .iter()
        .map(|(k, v)| format!("({:?}, {v:?}),", vim_to_regex(k)))
        .collect();
    writeln!(out, "static PATTERN: &[(&str, &str)] = &[{rows}];").unwrap();

    let defaults = dir.join("vim-glyph-palette/defaults.vim");
    let mut palette: Vec<(String, String)> = Vec::new();
    for (group, glyphs) in read_palette(&defaults) {
        let colour = match group.as_str() {
            "Directory" => "Colour::Directory".to_string(),
            n => format!(
                "Colour::Ansi({})",
                n.parse::<u8>()
                    .unwrap_or_else(|_| panic!("GlyphPalette{n} is no ANSI colour"))
            ),
        };
        palette.extend(glyphs.into_iter().map(|g| (g, colour.clone())));
    }
    palette.sort();
    if let Some(w) = palette.windows(2).find(|w| w[0].0 == w[1].0) {
        panic!("{:?} is in two palettes", w[0].0);
    }
    let rows: String = palette
        .iter()
        .map(|(g, c)| format!("({g:?}, {c}),"))
        .collect();
    writeln!(out, "static PALETTE: &[(&str, Colour)] = &[{rows}];").unwrap();

    let path = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("file_icons.rs");
    std::fs::write(path, out).unwrap();
}

/// The table in `path`, ordered by key so it can be searched.
fn read_json(path: &Path) -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The `'GlyphPaletteN': ['…', …]` lines of glyph-palette's defaults, as
/// `N` and the glyphs.
fn read_palette(path: &Path) -> Vec<(String, Vec<String>)> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let out: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let rest = line.trim_start().strip_prefix("\\ 'GlyphPalette")?;
            let (group, list) = rest.split_once("': [")?;
            let list = &list[..list.rfind(']')?];
            let glyphs = list
                .split(',')
                .map(|g| g.trim().trim_matches('\'').to_string())
                .filter(|g| !g.is_empty())
                .collect();
            Some((group.to_string(), glyphs))
        })
        .collect();
    assert!(!out.is_empty(), "{}: no palette", path.display());
    out
}

/// A Vim regular expression in the default `magic` mode, as the regex
/// crate writes it.
fn vim_to_regex(vim: &str) -> String {
    let mut out = String::new();
    let mut ignore_case = false;
    let mut chars = vim.chars().peekable();
    let mut first = true;
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('?' | '=') => out.push('?'),
                Some('+') => out.push('+'),
                Some('(') => out.push_str("(?:"),
                Some(')') => out.push(')'),
                Some('|') => out.push('|'),
                Some('d') => out.push_str("[0-9]"),
                Some('c') => ignore_case = true,
                Some(l @ ('.' | '/' | '\\' | '[' | ']' | '*' | '^' | '$' | '~' | '-')) => {
                    out.push_str(&regex_escape(l))
                }
                other => panic!("{vim:?}: \\{other:?} is not translated"),
            },
            '[' => {
                out.push('[');
                for c in chars.by_ref() {
                    out.push(c);
                    if c == ']' {
                        break;
                    }
                }
            }
            '^' if first => out.push('^'),
            '$' if chars.peek().is_none() => out.push('$'),
            '.' | '*' => out.push(c),
            c => out.push_str(&regex_escape(c)),
        }
        first = false;
    }
    if ignore_case {
        out.insert_str(0, "(?i)");
    }
    out
}

fn regex_escape(c: char) -> String {
    if "\\.+*?()|[]{}^$#&-~".contains(c) {
        format!("\\{c}")
    } else {
        c.to_string()
    }
}

fn describe() {
    // Release builds are described too: one installed from a checkout keeps
    // the version of the last release, which cannot tell it from the daemon
    // an earlier install left running.
    let described = std::process::Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|d| !d.is_empty());
    if let Some(d) = described {
        println!("cargo:rustc-env=ADROIT_DESCRIBE={d}");
    }
}
