//! What the collector finds in text: web addresses, paths, and the parts
//! of a message it passes over.

use std::ops::Range;
use std::path::Path;

use serde_json::Value;

/// The most of a tool's result or input read for addresses: past it lies
/// output too long to be worth reading for them.
pub(super) const SCANNED: usize = 64 * 1024;

/// Every string in `v`, at any depth.
pub(super) fn strings<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
    match v {
        Value::String(s) => out.push(s),
        Value::Array(items) => items.iter().for_each(|i| strings(i, out)),
        Value::Object(map) => map.values().for_each(|i| strings(i, out)),
        _ => {}
    }
}

/// A session's name without the ` [ref]` Claude Code adds to tell apart
/// sessions sharing it.
pub(super) fn without_ref(name: &str) -> &str {
    let name = name.trim();
    match name.rsplit_once(" [") {
        Some((bare, r))
            if r.strip_suffix(']')
                .is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_hexdigit())) =>
        {
            bare.trim_end()
        }
        _ => name,
    }
}

/// The first [`SCANNED`] bytes of `text`, cut at a character.
pub(super) fn head(text: &str) -> &str {
    if text.len() <= SCANNED {
        return text;
    }
    let mut end = SCANNED;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// What was said in a user's turn of `content`; none for a turn of tool
/// results alone, or of what a command printed.
pub(super) fn prompt_of(content: &Value) -> Option<String> {
    let said: Vec<&str> = match content {
        Value::String(s) => vec![s.as_str()],
        Value::Array(items) => items
            .iter()
            .filter(|i| i["type"].as_str() == Some("text"))
            .filter_map(|i| i["text"].as_str())
            .collect(),
        _ => Vec::new(),
    };
    let text = without_reminders(&said.join("\n"));
    let text = text.trim();
    // Claude Code writes the commands typed, and what they printed, in
    // the user's turn, wrapped in tags of its own.
    let said =
        !text.is_empty() && !text.starts_with("<command-") && !text.starts_with("<local-command");
    said.then(|| text.to_string())
}

/// Whether `said` is what Codex puts in the user's turn of its own accord,
/// before the first prompt.
pub(super) fn put_there_by_codex(said: &str) -> bool {
    [
        "# AGENTS.md instructions",
        "<environment_context>",
        "<user_instructions>",
    ]
    .iter()
    .any(|start| said.starts_with(start))
}

/// What a message Claude Code took in from a channel says, without the tag
/// it wraps it in.
pub(super) fn without_channel(text: &str) -> &str {
    text.strip_prefix("<channel ")
        .and_then(|rest| rest.split_once(">\n"))
        .map_or(text, |(_, said)| said)
        .trim_end()
        .trim_end_matches("</channel>")
        .trim_end()
}

pub(super) fn without_reminders(text: &str) -> String {
    const OPEN: &str = "<system-reminder>";
    const CLOSE: &str = "</system-reminder>";
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        rest = match rest[start..].find(CLOSE) {
            Some(end) => &rest[start + end + CLOSE.len()..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// The web addresses in `text`: where each lies, the address, and the text
/// of the Markdown link it is the target of.
pub(super) fn urls(text: &str) -> Vec<(Range<usize>, String, Option<String>)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = text[from..].find("http") {
        let start = from + i;
        let rest = &text[start..];
        let scheme = if rest.starts_with("https://") {
            8
        } else if rest.starts_with("http://") {
            7
        } else {
            from = start + 4;
            continue;
        };
        from = start + scheme;
        if text[..start]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        let len = rest
            .find(|c: char| {
                c.is_whitespace()
                    || matches!(c, '<' | '>' | '"' | '\'' | '`' | '\\' | '|' | '{' | '}')
            })
            .unwrap_or(rest.len());
        // A Markdown link whose text is its own address.
        let raw = &rest[..len];
        let raw = raw.find("](").map_or(raw, |i| &raw[..i]);
        let url = trim_url(raw);
        if host(url).is_empty() {
            continue;
        }
        from = start + url.len();
        out.push((
            start..start + url.len(),
            url.to_string(),
            markdown_title(&text[..start], url),
        ));
    }
    out
}

/// `url` without what closes the sentence or the brackets around it.
pub(super) fn trim_url(mut url: &str) -> &str {
    loop {
        let count = |c: char| url.matches(c).count();
        let Some(last) = url.chars().next_back() else {
            return url;
        };
        let drop = match last {
            '.' | ',' | ';' | ':' | '!' | '?' | '*' | '~' => true,
            ')' => count('(') < count(')'),
            ']' => count('[') < count(']'),
            _ => false,
        };
        if !drop {
            return url;
        }
        url = &url[..url.len() - last.len_utf8()];
    }
}

pub(super) fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or("", |(_, r)| r);
    rest.split(['/', '?', '#']).next().unwrap_or_default()
}

pub(super) fn markdown_title(before: &str, url: &str) -> Option<String> {
    let inner = before.strip_suffix("](")?;
    let label = &inner[inner.rfind('[')? + 1..];
    if label.contains(['\n', ']']) {
        return None;
    }
    let label = label.trim().trim_matches(['`', '*']).trim();
    (!label.is_empty() && label != url).then(|| label.to_string())
}

/// `token` without a `:line` or `:line:column` it ends with, and the line.
pub(super) fn split_line(token: &str) -> (&str, Option<u32>) {
    let number = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let mut path = token;
    let mut line = None;
    for _ in 0..2 {
        match path.rsplit_once(':') {
            Some((p, n)) if number(n) => {
                line = n.parse().ok();
                path = p;
            }
            _ => break,
        }
    }
    (path, line)
}

pub(super) fn looks_like_path(token: &str) -> bool {
    if token.is_empty() || token.len() > 1024 || token.contains("://") {
        return false;
    }
    let p = Path::new(token);
    token.contains('/')
        || p.extension().is_some_and(|e| {
            e.to_str()
                .is_some_and(|e| e.bytes().all(|b| b.is_ascii_alphanumeric()))
        }) && p.file_stem().is_some_and(|s| !s.is_empty())
}
