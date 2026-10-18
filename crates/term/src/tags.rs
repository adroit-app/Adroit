//! Values kept in a session's tags, `key=value`, written across as many
//! tags as a long one takes: a tag holds only so many bytes, and a path or
//! a name can hold more.

use felis_protocol::messages::MAX_TAG_BYTES;

/// The tags that hold `value` under `key`.
pub fn tagged(key: &str, value: &str) -> Vec<String> {
    let whole = format!("{key}={value}");
    if whole.len() <= MAX_TAG_BYTES {
        return vec![whole];
    }
    let mut out = Vec::new();
    let mut rest = value;
    while !rest.is_empty() {
        let head = format!("{key}.{}=", out.len());
        let room = MAX_TAG_BYTES - head.len();
        let mut end = room.min(rest.len());
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        out.push(format!("{head}{}", &rest[..end]));
        rest = &rest[end..];
    }
    out
}

/// The value held under `key`, whole or in parts.
pub fn value(tags: &[String], key: &str) -> Option<String> {
    let whole = format!("{key}=");
    if let Some(v) = tags.iter().find_map(|t| t.strip_prefix(&whole)) {
        return Some(v.to_string());
    }
    let mut out = String::new();
    for n in 0.. {
        let head = format!("{key}.{n}=");
        match tags.iter().find_map(|t| t.strip_prefix(&head)) {
            Some(part) => out.push_str(part),
            None if n == 0 => return None,
            None => break,
        }
    }
    Some(out)
}

/// The tags among `tags` that hold something under `key`.
pub fn holding(tags: &[String], key: &str) -> Vec<String> {
    let whole = format!("{key}=");
    let part = format!("{key}.");
    tags.iter()
        .filter(|t| t.starts_with(&whole) || t.starts_with(&part))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_value_is_one_tag() {
        assert_eq!(tagged("adroit.kind", "shell"), vec!["adroit.kind=shell"]);
        assert_eq!(
            value(&tagged("adroit.kind", "shell"), "adroit.kind").as_deref(),
            Some("shell")
        );
    }

    #[test]
    fn a_long_value_is_spread_over_tags_and_read_back_whole() {
        let long = "日本語のとても長い名前".repeat(10);
        let tags = tagged("adroit.name", &long);
        assert!(tags.len() > 1);
        assert!(tags.iter().all(|t| t.len() <= MAX_TAG_BYTES));
        assert_eq!(value(&tags, "adroit.name"), Some(long));
    }

    #[test]
    fn a_key_holds_nothing_when_no_tag_names_it() {
        let tags = vec!["adroit.kindly=x".to_string()];
        assert_eq!(value(&tags, "adroit.kind"), None);
        assert!(holding(&tags, "adroit.kind").is_empty());
    }

    #[test]
    fn the_tags_holding_a_key_are_found_to_be_replaced() {
        let mut tags = tagged("adroit.name", &"x".repeat(200));
        tags.push("adroit.kind=shell".into());
        assert_eq!(holding(&tags, "adroit.name").len(), 2);
    }
}
