//! The tabs above the console: the sessions and terminals of one worktree.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use adroit_model::{Agent, AgentPermissions, DraftId};

/// What a tab shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Tab {
    /// A Claude Code session, by id.
    Session(String),
    /// A shell adroit started in a worktree, by the daemon's id.
    Terminal(u128),
    /// A session not started yet: the next prompt sent from here starts it.
    Draft { worktree: PathBuf, id: DraftId },
}

/// Whether the prompt is shown under the console while `tab` is: a shell
/// takes every key in the console itself.
pub fn shows_prompt(tab: Option<&Tab>) -> bool {
    !matches!(tab, Some(Tab::Terminal(_)))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DraftOptions {
    pub agent: Option<Agent>,
    pub model: Option<String>,
    pub permissions: AgentPermissions,
}

impl DraftOptions {
    /// The options chosen with nothing open, used up by the draft they were
    /// chosen for, or else the defaults.
    pub fn for_new_draft(next: &mut Option<Self>) -> Self {
        next.take().unwrap_or_default()
    }
}

impl From<Agent> for DraftOptions {
    fn from(agent: Agent) -> Self {
        Self {
            agent: Some(agent),
            ..Self::default()
        }
    }
}

impl Tab {
    pub fn draft(worktree: impl Into<PathBuf>) -> Self {
        Self::Draft {
            worktree: worktree.into(),
            id: DraftId::new(),
        }
    }
}

pub fn drafts_in<'a>(opened: &'a HashSet<Tab>, worktree: &Path) -> Vec<&'a Tab> {
    let mut drafts: Vec<_> = opened
        .iter()
        .filter(|tab| matches!(tab, Tab::Draft { worktree: path, .. } if path == worktree))
        .collect();
    drafts.sort_by_key(|tab| match tab {
        Tab::Draft { id, .. } => id.to_string(),
        _ => unreachable!(),
    });
    drafts
}

/// Words that say little about what a session is for.
const FILLER: &[&str] = &[
    "a", "an", "the", "to", "of", "for", "and", "or", "in", "on", "at", "by", "with", "is", "are",
    "be", "please", "can", "could", "would", "you", "i", "we", "me", "my", "it", "this", "that",
    "why", "what", "how",
];

/// How many characters a label made from text without ASCII words keeps:
/// such text, Japanese for one, need not have spaces to cut it into words,
/// and each of its characters takes about two columns of a sidebar row.
const UNSPACED_LABEL_CHARS: usize = 12;

/// A short label for a new session from its first prompt: the first few
/// words that say something, lower case, joined by `-`, keeping ASCII
/// letters and digits only. A prompt without such words is labelled by its
/// first letters and digits in any script instead.
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
    if !words.is_empty() {
        return words.join("-");
    }
    let label: String = letter_runs(prompt)
        .join("-")
        .chars()
        .take(UNSPACED_LABEL_CHARS)
        .collect();
    match label.trim_end_matches('-') {
        "" => "session".into(),
        label => label.to_string(),
    }
}

/// The runs of letters and digits in `text`, in any script, lower case.
fn letter_runs(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|run| !run.is_empty())
        .map(str::to_lowercase)
        .collect()
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
/// `-`, keeping letters in any script, digits and `-`; `None` when nothing
/// is left.
pub fn label_of(name: &str) -> Option<String> {
    let label = name
        .split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .collect::<String>()
                .to_lowercase()
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
        Tab::Terminal(id) => Some(format!("terminal:{id:x}")),
        Tab::Draft { .. } => None,
    }
}

/// The tab a name [`kept_name`] gave stands for.
pub fn from_kept(name: &str) -> Option<Tab> {
    let name = name.trim();
    if let Some(id) = name.strip_prefix("session:") {
        return Some(Tab::Session(id.to_string()));
    }
    let id = name.strip_prefix("terminal:")?;
    u128::from_str_radix(id, 16).ok().map(Tab::Terminal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_chosen_with_nothing_open_go_to_the_next_draft_only() {
        let chosen = DraftOptions {
            agent: None,
            model: Some("opus".into()),
            permissions: AgentPermissions::ReadOnly,
        };
        let mut next = Some(chosen.clone());
        assert_eq!(DraftOptions::for_new_draft(&mut next), chosen);
        assert_eq!(next, None);
        assert_eq!(
            DraftOptions::for_new_draft(&mut next),
            DraftOptions::default()
        );
    }

    #[test]
    fn a_shell_shows_no_prompt_and_a_session_a_draft_or_nothing_open_does() {
        let draft = Tab::Draft {
            worktree: PathBuf::from("/w"),
            id: Default::default(),
        };
        assert!(!shows_prompt(Some(&Tab::Terminal(1))));
        assert!(shows_prompt(Some(&Tab::Session("a".into()))));
        assert!(shows_prompt(Some(&draft)));
        assert!(shows_prompt(None));
    }

    #[test]
    fn a_session_s_or_a_shell_s_tab_is_kept_by_name_and_a_draft_s_is_not() {
        for tab in [Tab::Session("ab12".into()), Tab::Terminal(0xff)] {
            let name = kept_name(&tab).unwrap();
            assert_eq!(from_kept(&name), Some(tab), "{name}");
        }
        let draft = Tab::Draft {
            worktree: "/src".into(),
            id: Default::default(),
        };
        assert_eq!(kept_name(&draft), None);
        assert_eq!(from_kept("shell:not hex"), None);
        assert_eq!(from_kept("something else"), None);
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
    fn punctuation_and_non_ascii_words_are_dropped_when_ascii_words_say_something() {
        assert_eq!(label_from("認証の テスト: add PKCE!"), "add-pkce");
    }

    #[test]
    fn a_prompt_without_ascii_words_is_labelled_by_its_own_letters() {
        assert_eq!(label_from("日本語だけ"), "日本語だけ");
        assert_eq!(label_from("Тест входа"), "тест-входа");
    }

    #[test]
    fn a_label_from_unspaced_text_is_cut_by_characters() {
        assert_eq!(
            label_from("ログインのテストが落ちる理由を調べてください"),
            "ログインのテストが落ちる"
        );
    }

    #[test]
    fn punctuation_in_unspaced_text_becomes_a_single_dash() {
        assert_eq!(label_from("認証、直して。"), "認証-直して");
        assert_eq!(label_from("「認証」の　テスト"), "認証-の-テスト");
        assert_eq!(
            label_from("認証のテストを直して、確認"),
            "認証のテストを直して-確"
        );
    }

    #[test]
    fn a_prompt_without_letters_or_digits_gets_a_generic_label() {
        assert_eq!(label_from("！？ ..."), "session");
        assert_eq!(label_from(""), "session");
    }

    #[test]
    fn a_name_given_by_hand_keeps_letters_of_any_script() {
        assert_eq!(label_of("ログイン 修正"), Some("ログイン-修正".to_string()));
        assert_eq!(
            named("name: 認証の修正\n直して").0.as_deref(),
            Some("認証の修正")
        );
        assert_eq!(label_of("「」"), None);
    }

    #[test]
    fn a_taken_name_gets_the_next_free_number() {
        let taken = ["fix-the-test", "fix-the-test-2"];
        assert_eq!(fresh_name("fix-the-test", &taken), "fix-the-test-3");
        assert_eq!(fresh_name("new", &taken), "new");
    }
}
