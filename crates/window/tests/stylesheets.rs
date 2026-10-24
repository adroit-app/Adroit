//! Every class and id the window's stylesheets rule is written in a string
//! of the sources that draw the windows, so a rule left behind when its
//! markup went is caught.
//!
//! Only string literals count, comments and identifiers do not: a class
//! named like a word the code says elsewhere, such as `run`, would pass
//! otherwise. A class put together from pieces is named by its prefix in
//! [`COMPOSED`].

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Classes a source puts together as a prefix and a value, such as
/// `lane-{}` for the colour of a lane of the log's graph.
const COMPOSED: [&str; 1] = ["lane-"];

/// The directories whose sources write the classes: the window's, its page
/// scripts, and the view state's, which hands some of them over.
const SOURCES: [&str; 3] = ["src", "assets/js", "../core/src"];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn files(dir: &Path, extensions: &[&str], into: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, extensions, into);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| extensions.contains(&e))
        {
            into.push(path);
        }
    }
}

/// `css` without its comments.
fn uncommented(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(at) = rest.find("/*") {
        out.push_str(&rest[..at]);
        rest = rest[at..]
            .find("*/")
            .map_or("", |end| &rest[at + end + 2..]);
    }
    out.push_str(rest);
    out
}

/// The text before each `{` of `css`: the selectors of its rules, and the
/// preludes of its at-rules.
fn preludes(css: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut text = String::new();
    for c in uncommented(css).chars() {
        match c {
            '{' => found.push(std::mem::take(&mut text)),
            ';' | '}' => text.clear(),
            c => text.push(c),
        }
    }
    found
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// The classes and ids `selector` names, each with its `.` or `#`.
fn named(selector: &str) -> Vec<String> {
    if selector.trim_start().starts_with('@') {
        return Vec::new();
    }
    let chars: Vec<char> = selector.chars().collect();
    let mut found = Vec::new();
    let mut quoted = None;
    let mut bracketed = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match (quoted, c) {
            (Some(q), c) if c == q => quoted = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quoted = Some(c),
            (None, '[') => bracketed = true,
            (None, ']') => bracketed = false,
            (None, '.' | '#')
                if !bracketed
                    && chars
                        .get(i + 1)
                        .is_some_and(|n| n.is_ascii_alphabetic() || *n == '_' || *n == '-') =>
            {
                let end = (i + 1..chars.len())
                    .find(|&j| !is_name_char(chars[j]))
                    .unwrap_or(chars.len());
                found.push(chars[i..end].iter().collect());
                i = end;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    found
}

/// The string literals of a Rust or JavaScript `source`, comments skipped:
/// `"…"`, raw strings, and in scripts `'…'` and `` `…` ``. A Rust `'` starts
/// a character or a lifetime, which hold no class.
fn literals(source: &str, script: bool) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    let at = |j: usize| chars.get(j).copied().unwrap_or('\0');
    while i < chars.len() {
        match (at(i), at(i + 1)) {
            ('/', '/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            ('/', '*') => {
                i += 2;
                while i < chars.len() && !(at(i) == '*' && at(i + 1) == '/') {
                    i += 1;
                }
                i += 2;
            }
            ('r', '#' | '"') if !script && (i == 0 || !is_name_char(at(i - 1))) => {
                let mut j = i + 1;
                while at(j) == '#' {
                    j += 1;
                }
                if at(j) != '"' {
                    i += 1;
                    continue;
                }
                let hashes = j - i - 1;
                let start = j + 1;
                let mut end = start;
                while end < chars.len()
                    && !(at(end) == '"' && (1..=hashes).all(|h| at(end + h) == '#'))
                {
                    end += 1;
                }
                found.push(chars[start..end.min(chars.len())].iter().collect());
                i = end + 1 + hashes;
            }
            ('\'', next) if !script => {
                // A character such as 'a', '\n' or '"'; a lifetime otherwise.
                if next == '\\' {
                    i += 2;
                    while i < chars.len() && chars[i] != '\'' {
                        i += 1;
                    }
                    i += 1;
                } else if at(i + 2) == '\'' {
                    i += 3;
                } else {
                    i += 1;
                }
            }
            (q @ ('"' | '\'' | '`'), _) => {
                let mut text = String::new();
                i += 1;
                while i < chars.len() && chars[i] != q {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    if let Some(&c) = chars.get(i) {
                        text.push(c);
                    }
                    i += 1;
                }
                found.push(text);
                i += 1;
            }
            _ => i += 1,
        }
    }
    found
}

/// Every word of the sources' string literals, a word being a run of the
/// characters a class name is made of.
fn written() -> BTreeSet<String> {
    let mut paths = Vec::new();
    for dir in SOURCES {
        files(&root().join(dir), &["rs", "js"], &mut paths);
    }
    paths
        .iter()
        .flat_map(|path| {
            let script = path.extension().is_some_and(|e| e == "js");
            literals(&fs::read_to_string(path).unwrap(), script)
        })
        .flat_map(|text| {
            text.split(|c: char| !is_name_char(c))
                .filter(|w| !w.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn every_class_and_id_a_stylesheet_rules_is_written_by_a_source() {
    let written = written();
    let mut sheets = Vec::new();
    files(&root().join("assets/css"), &["css"], &mut sheets);
    assert!(!sheets.is_empty());
    let mut unwritten: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for sheet in &sheets {
        let css = fs::read_to_string(sheet).unwrap();
        for name in preludes(&css).iter().flat_map(|p| named(p)) {
            let bare = &name[1..];
            let composed = COMPOSED.iter().any(|prefix| {
                bare.starts_with(prefix)
                    && written
                        .iter()
                        .any(|w| w == prefix.trim_end_matches('-') || w.starts_with(prefix))
            });
            if !written.contains(bare) && !composed {
                let file = sheet.file_name().unwrap().to_string_lossy().into_owned();
                unwritten.entry(name).or_default().insert(file);
            }
        }
    }
    assert!(
        unwritten.is_empty(),
        "ruled by a stylesheet but written by no source: {unwritten:#?}"
    );
}

#[test]
fn selectors_name_their_classes_and_ids_but_not_what_brackets_or_quotes_hold() {
    assert_eq!(
        named(".a > .b-c:hover, #d_e [data-x=\".no\"] .f::after"),
        [".a", ".b-c", "#d_e", ".f"]
    );
    assert_eq!(
        named("@media (prefers-color-scheme: light)"),
        Vec::<String>::new()
    );
}

#[test]
fn rules_are_found_inside_at_rules_and_declarations_are_not_rules() {
    let css = "/* .gone {} */ .a { color: #fff; width: 0.5em }\n@media (x) { .b { top: 0 } }";
    let names: Vec<String> = preludes(css).iter().flat_map(|p| named(p)).collect();
    assert_eq!(names, [".a", ".b"]);
}

#[test]
fn only_string_literals_count_not_comments_lifetimes_or_characters() {
    let rust = r##"
        // "commented"
        fn f<'a>(x: &'a str) -> char { let _ = r#"raw "kept" here"#; let _ = "plain \" quote"; '"' }
        /* "blocked" */
        const S: &str = "after";
    "##;
    assert_eq!(
        literals(rust, false),
        ["raw \"kept\" here", "plain \" quote", "after"]
    );
    let js = "// don't\nconst a = 'one'; const b = `two`; /* \"three\" */ const c = \"four\";";
    assert_eq!(literals(js, true), ["one", "two", "four"]);
}
