//! The prompts Claude Code keeps to recall with its Up arrow, in
//! `history.jsonl`: one line a prompt, naming the directory it was sent in.

use std::path::{Path, PathBuf};

/// Where Claude Code keeps the prompts sent: under `$CLAUDE_CONFIG_DIR`,
/// else under `.claude/` in the home directory.
fn history_path(home: &Path) -> PathBuf {
    match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) => PathBuf::from(dir).join("history.jsonl"),
        None => home.join(".claude").join("history.jsonl"),
    }
}

/// The prompts sent in `project`, oldest first, as they were written: text
/// Claude Code shows as `[Pasted text #1 +3 lines]` is put back.
pub fn parse(text: &str, project: &Path) -> Vec<String> {
    text.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|v| v.get("project").and_then(|p| p.as_str()) == project.to_str())
        .filter_map(|v| {
            let display = v.get("display")?.as_str()?;
            let pasted = v.get("pastedContents").and_then(|p| p.as_object());
            let content = |id: &str| pasted?.get(id)?.get("content").and_then(|c| c.as_str());
            let prompt = put_back(display, content);
            (!prompt.trim().is_empty()).then_some(prompt)
        })
        .collect()
}

/// `display` with every placeholder replaced by the content `content` finds
/// for its id; a placeholder with none is left as it stands. The display is
/// read once, so a paste that itself holds a placeholder is not read again.
fn put_back<'a>(display: &str, content: impl Fn(&str) -> Option<&'a str>) -> String {
    const HEAD: &str = "[Pasted text #";
    let mut out = String::new();
    let mut rest = display;
    while let Some(at) = rest.find(HEAD) {
        let after = &rest[at + HEAD.len()..];
        let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let (id, tail) = after.split_at(digits);
        let end = match tail.chars().next() {
            Some(']') => Some(0),
            Some(' ') => tail.find(']'),
            _ => None,
        };
        match end.zip(content(id)).filter(|_| !id.is_empty()) {
            Some((end, pasted)) => {
                out.push_str(&rest[..at]);
                out.push_str(pasted);
                rest = &tail[end + 1..];
            }
            None => {
                out.push_str(&rest[..at + HEAD.len()]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The prompts sent in `project`, oldest first; none when Claude Code has
/// kept none or they cannot be read.
pub fn read(project: &Path) -> Vec<String> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    std::fs::read_to_string(history_path(Path::new(&home)))
        .map(|text| parse(&text, project))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(display: &str, project: &str, pasted: serde_json::Value) -> String {
        serde_json::json!({
            "display": display,
            "pastedContents": pasted,
            "project": project,
            "sessionId": "s",
            "timestamp": 1,
        })
        .to_string()
    }

    #[test]
    fn only_the_prompts_sent_in_the_project_are_recalled_oldest_first() {
        let text = [
            line("first", "/p", serde_json::json!({})),
            line("elsewhere", "/q", serde_json::json!({})),
            line("second", "/p", serde_json::json!({})),
        ]
        .join("\n");
        assert_eq!(parse(&text, Path::new("/p")), ["first", "second"]);
    }

    #[test]
    fn pasted_text_is_put_back_where_its_placeholder_stands() {
        let text = line(
            "see [Pasted text #1 +2 lines] and [Pasted text #2]",
            "/p",
            serde_json::json!({
                "1": {"id": 1, "type": "text", "content": "a\nb\nc"},
                "2": {"id": 2, "type": "text", "content": "d"},
            }),
        );
        assert_eq!(parse(&text, Path::new("/p")), ["see a\nb\nc and d"]);
    }

    #[test]
    fn a_paste_does_not_take_the_placeholder_of_another_whose_id_it_begins() {
        let text = line(
            "[Pasted text #12 +1 lines] [Pasted text #1]",
            "/p",
            serde_json::json!({"1": {"id": 1, "type": "text", "content": "x"}}),
        );
        assert_eq!(
            parse(&text, Path::new("/p")),
            ["[Pasted text #12 +1 lines] x"]
        );
    }

    #[test]
    fn a_paste_quoting_another_s_placeholder_is_put_back_as_it_was_pasted() {
        let text = line(
            "[Pasted text #1 +1 lines] then [Pasted text #2]",
            "/p",
            serde_json::json!({
                "1": {"id": 1, "type": "text", "content": "log:\n[Pasted text #2]"},
                "2": {"id": 2, "type": "text", "content": "d"},
            }),
        );
        assert_eq!(
            parse(&text, Path::new("/p")),
            ["log:\n[Pasted text #2] then d"]
        );
    }

    #[test]
    fn a_paste_kept_elsewhere_leaves_its_placeholder() {
        let text = line(
            "[Pasted text #1 +40 lines]",
            "/p",
            serde_json::json!({"1": {"id": 1, "type": "text", "contentHash": "abc"}}),
        );
        assert_eq!(
            parse(&text, Path::new("/p")),
            ["[Pasted text #1 +40 lines]"]
        );
    }

    #[test]
    fn lines_that_are_not_prompts_are_passed_over() {
        let text = [
            "not json",
            &line("", "/p", serde_json::json!({})),
            &line("kept", "/p", serde_json::json!({})),
        ]
        .join("\n");
        assert_eq!(parse(&text, Path::new("/p")), ["kept"]);
    }
}
