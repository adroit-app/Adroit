//! The tabs above the console: the sessions and shells of one worktree.

use std::path::PathBuf;

/// What a tab shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Tab {
    /// A Claude Code session, by id.
    Session(String),
    /// A shell adroit started in a worktree, by the daemon's id.
    Shell(u128),
    /// A session not started yet: the next prompt sent from here starts it.
    Draft { worktree: PathBuf, role: Role },
}

/// Whether the prompt is shown under the console while `tab` is: a shell
/// takes every key in the console itself.
pub fn shows_prompt(tab: Option<&Tab>) -> bool {
    !matches!(tab, Some(Tab::Shell(_)))
}

/// What a session started from a draft is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Works in its worktree.
    Worker,
    /// Reads and reports, without editing.
    Reader,
}

impl Role {
    /// What the tab of a draft with this role says.
    /// The title, as a heading starts.
    pub fn draft_heading(self) -> String {
        let title = self.draft_title();
        title[..1].to_uppercase() + &title[1..]
    }

    pub fn draft_title(self) -> &'static str {
        match self {
            Role::Worker => "new session",
            Role::Reader => "new reader",
        }
    }
}

/// Words that say little about what a session is for.
const FILLER: &[&str] = &[
    "a", "an", "the", "to", "of", "for", "and", "or", "in", "on", "at", "by", "with", "is", "are",
    "be", "please", "can", "could", "would", "you", "i", "we", "me", "my", "it", "this", "that",
    "why", "what", "how",
];

/// A short label for a new session from its first prompt: the first few
/// words that say something, lower case, joined by `-`, keeping letters and
/// digits only.
pub fn label_from(prompt: &str) -> String {
    let words: Vec<String> = prompt
        .split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase()
        })
        .filter(|w| !w.is_empty() && !FILLER.contains(&w.as_str()))
        .take(3)
        .collect();
    if words.is_empty() {
        "session".into()
    } else {
        words.join("-")
    }
}

/// The label a first prompt gives its session on a first line `name: …`,
/// and the prompt to send, without that line. A prompt without one is sent
/// whole.
pub fn named(prompt: &str) -> (Option<String>, String) {
    let (first, rest) = prompt.split_once('\n').unwrap_or((prompt, ""));
    let given = first
        .trim()
        .get(..5)
        .filter(|head| head.eq_ignore_ascii_case("name:"))
        .map(|_| first.trim()[5..].trim())
        .filter(|name| !name.is_empty());
    match given {
        Some(name) => (label_of(name), rest.trim_start_matches('\n').to_string()),
        None => (None, prompt.to_string()),
    }
}

/// The label a name given by hand makes: its words lower case, joined by
/// `-`, keeping letters, digits and `-`; `None` when nothing is left.
pub fn label_of(name: &str) -> Option<String> {
    let label = name
        .split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect::<String>()
                .to_ascii_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    (!label.is_empty()).then_some(label)
}

/// A name for a new session no other session has: the label, then the label
/// with `-2`, `-3`, … until one is free.
pub fn fresh_name(label: &str, taken: &[&str]) -> String {
    let mut name = label.to_string();
    let mut n = 2;
    while taken.contains(&name.as_str()) {
        name = format!("{label}-{n}");
        n += 1;
    }
    name
}

/// How a tab is kept for the next run of the window. A draft holds nothing
/// to come back to, so it is not kept.
pub fn kept_name(tab: &Tab) -> Option<String> {
    match tab {
        Tab::Session(id) => Some(format!("session:{id}")),
        Tab::Shell(id) => Some(format!("shell:{id:x}")),
        Tab::Draft { .. } => None,
    }
}

/// The tab a name [`kept_name`] gave stands for.
pub fn from_kept(name: &str) -> Option<Tab> {
    let name = name.trim();
    if let Some(id) = name.strip_prefix("session:") {
        return Some(Tab::Session(id.to_string()));
    }
    let id = name.strip_prefix("shell:")?;
    u128::from_str_radix(id, 16).ok().map(Tab::Shell)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shell_shows_no_prompt_and_a_session_a_draft_or_nothing_open_does() {
        let draft = Tab::Draft {
            worktree: PathBuf::from("/w"),
            role: Role::Worker,
        };
        assert!(!shows_prompt(Some(&Tab::Shell(1))));
        assert!(shows_prompt(Some(&Tab::Session("a".into()))));
        assert!(shows_prompt(Some(&draft)));
        assert!(shows_prompt(None));
    }

    #[test]
    fn a_session_s_or_a_shell_s_tab_is_kept_by_name_and_a_draft_s_is_not() {
        for tab in [Tab::Session("ab12".into()), Tab::Shell(0xff)] {
            let name = kept_name(&tab).unwrap();
            assert_eq!(from_kept(&name), Some(tab), "{name}");
        }
        let draft = Tab::Draft {
            worktree: "/src".into(),
            role: Role::Worker,
        };
        assert_eq!(kept_name(&draft), None);
        assert_eq!(from_kept("shell:not hex"), None);
        assert_eq!(from_kept("something else"), None);
    }

    #[test]
    fn a_draft_is_headed_with_what_it_starts() {
        assert_eq!(Role::Worker.draft_heading(), "New session");
        assert_eq!(Role::Reader.draft_heading(), "New reader");
    }

    #[test]
    fn a_label_is_the_first_three_words_of_the_prompt_that_say_something() {
        assert_eq!(
            label_from("Fix the OAuth callback test"),
            "fix-oauth-callback"
        );
        assert_eq!(
            label_from("investigate why the integration tests keep failing"),
            "investigate-integration-tests"
        );
    }

    #[test]
    fn a_first_line_naming_the_session_is_its_label_and_is_not_sent() {
        assert_eq!(
            named("name: Login Flow\n\nfix the login"),
            (Some("login-flow".to_string()), "fix the login".to_string())
        );
        assert_eq!(named("Name:  ci \nrun it").0.as_deref(), Some("ci"));
    }

    #[test]
    fn a_prompt_that_names_nothing_is_sent_whole() {
        let text = "fix the name: field\nplease";
        assert_eq!(named(text), (None, text.to_string()));
        assert_eq!(named("name:\nfix"), (None, "name:\nfix".to_string()));
    }

    #[test]
    fn punctuation_and_non_ascii_words_are_dropped() {
        assert_eq!(label_from("認証の テスト: add PKCE!"), "add-pkce");
    }

    #[test]
    fn a_prompt_without_usable_words_gets_a_generic_label() {
        assert_eq!(label_from("日本語だけ"), "session");
    }

    #[test]
    fn a_taken_name_gets_the_next_free_number() {
        let taken = ["fix-the-test", "fix-the-test-2"];
        assert_eq!(fresh_name("fix-the-test", &taken), "fix-the-test-3");
        assert_eq!(fresh_name("new", &taken), "new");
    }
}
