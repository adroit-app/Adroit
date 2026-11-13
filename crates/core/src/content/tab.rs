//! What a window of the content shows, and the names new sessions are given.

use std::path::{Path, PathBuf};

use adroit_model::profile::ProfileId;
use adroit_model::{AgentPermissions, DraftId, EnvironmentId};

use super::{Kept, KeptNode, KeptTab};

/// What a window shows, by the environment it belongs to and its own
/// identity there.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Tab {
    /// A Claude Code session, by id.
    Session(EnvironmentId, String),
    /// A shell adroit started in a worktree, by the daemon's id.
    Terminal(EnvironmentId, u128),
    /// A session not started yet: the next prompt sent from here starts it.
    Draft {
        environment: EnvironmentId,
        worktree: PathBuf,
        id: DraftId,
    },
    /// A file shown in the viewer of its kind.
    Viewer(EnvironmentId, PathBuf),
    /// An image `session`'s console drew, `width` by `height` pixels, shown
    /// in the image viewer from the copy the window keeps by `id`.
    Drawn {
        environment: EnvironmentId,
        session: String,
        id: u64,
        width: u32,
        height: u32,
    },
}

/// Whether the prompt is shown under the console of a window showing `tab`, or
/// with nothing open: a shell takes every key in the console itself, and a
/// viewer has no console.
pub fn shows_prompt(tab: Option<&Tab>) -> bool {
    !matches!(
        tab,
        Some(Tab::Terminal(..) | Tab::Viewer(..) | Tab::Drawn { .. })
    )
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DraftOptions {
    /// The profile chosen, which names its agent; the default profile when
    /// none is.
    pub profile: Option<ProfileId>,
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

impl Tab {
    pub fn draft(environment: EnvironmentId, worktree: impl Into<PathBuf>) -> Self {
        Self::Draft {
            environment,
            worktree: worktree.into(),
            id: DraftId::new(),
        }
    }

    pub fn environment(&self) -> &EnvironmentId {
        match self {
            Tab::Session(environment, _)
            | Tab::Terminal(environment, _)
            | Tab::Viewer(environment, _)
            | Tab::Draft { environment, .. }
            | Tab::Drawn { environment, .. } => environment,
        }
    }
}

/// Words that say little about what a session is for.
const FILLER: &[&str] = &[
    "a", "an", "the", "to", "of", "for", "and", "or", "in", "on", "at", "by", "with", "is", "are",
    "be", "please", "can", "could", "would", "you", "i", "we", "me", "my", "it", "this", "that",
    "why", "what", "how",
];

/// How many columns of a sidebar row a label made from text in a script
/// besides ASCII fills: Japanese, for one, has no spaces to say where its
/// words end, and each of its characters takes two columns.
const LABEL_COLUMNS: usize = 24;

/// Parts of a URL's path that say what kind of page it is, not what it is
/// about.
const URL_KINDS: &[&str] = &[
    "pull", "pulls", "issues", "blob", "tree", "commit", "commits", "compare", "main", "master",
    "-",
];

/// Whether `word` is a path or a URL, which says what it is by its words
/// rather than its letters.
fn is_path(word: &str) -> bool {
    word.contains('/')
}

/// The ASCII words of one word of a prompt. A path says what it is by its
/// file name's words, without the dates and numbers that tell files apart,
/// and a URL by the last part of its path that names something, else by
/// its host.
fn words_of(word: &str) -> Vec<String> {
    let word = word.to_ascii_lowercase();
    if let Some((_, rest)) = word.split_once("://") {
        let rest = rest.split(['?', '#']).next().unwrap_or_default();
        let mut parts = rest.split('/');
        let host = parts.next().unwrap_or_default();
        let named = parts
            .rfind(|p| {
                !p.is_empty() && !URL_KINDS.contains(p) && !p.chars().all(|c| c.is_ascii_digit())
            })
            .unwrap_or_else(|| host.split('.').find(|l| *l != "www").unwrap_or_default());
        return ascii_words(named);
    }
    if !is_path(&word) {
        return vec![word.chars().filter(char::is_ascii_alphanumeric).collect()];
    }
    let stem = Path::new(&word)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    ascii_words(&stem)
}

/// The runs of ASCII letters and digits in `text` that are not numbers alone.
fn ascii_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty() && !w.chars().all(|c| c.is_ascii_digit()))
        .map(String::from)
        .collect()
}

/// A short label for a new session from its first prompt, lower case.
/// A prompt in ASCII alone is labelled by its first few words that say
/// something, each once, joined by `-`. One with words in another script,
/// or with no words that say something, is labelled by its letters and
/// digits as far as a sidebar row holds them, its clauses joined by `-`.
pub fn label_from(prompt: &str) -> String {
    let other_script = prompt
        .split_whitespace()
        .filter(|w| !is_path(w))
        .any(|w| w.chars().any(|c| c.is_alphabetic() && !c.is_ascii()));
    if other_script {
        return fitted(clauses_of(
            &prompt
                .split_whitespace()
                .map(|w| match is_path(w) {
                    true => words_of(w)
                        .into_iter()
                        .take(3)
                        .collect::<Vec<_>>()
                        .join(" "),
                    false => w.to_string(),
                })
                .collect::<Vec<_>>()
                .join(" "),
        ));
    }
    let mut words: Vec<String> = Vec::new();
    for word in prompt.split_whitespace().flat_map(words_of) {
        if !word.is_empty() && !FILLER.contains(&word.as_str()) && !words.contains(&word) {
            words.push(word);
        }
    }
    match words.len() {
        0 => fitted(clauses_of(prompt)),
        _ => words[..words.len().min(3)].join("-"),
    }
}

/// The clauses of `text`: its runs of letters and digits in any script,
/// lower case, broken at punctuation. Words a space parts are joined by `-`,
/// unless one of them is written without spaces.
fn clauses_of(text: &str) -> Vec<String> {
    let mut clauses = Vec::new();
    let mut clause = String::new();
    let mut spaced = false;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if spaced
                && clause
                    .chars()
                    .last()
                    .is_some_and(|last| !unspaced(last) && !unspaced(c))
            {
                clause.push('-');
            }
            spaced = false;
            clause.extend(c.to_lowercase());
        } else if c.is_whitespace() {
            spaced = true;
        } else {
            spaced = false;
            if !clause.is_empty() {
                clauses.push(std::mem::take(&mut clause));
            }
        }
    }
    if !clause.is_empty() {
        clauses.push(clause);
    }
    clauses
}

/// A label of `clauses` as far as a sidebar row holds them: the first cut
/// if need be, and each after it only if it fits whole. A short clause in
/// kana alone that leads into the rest, a call like `あれ`, is left out.
fn fitted(mut clauses: Vec<String>) -> String {
    if clauses.len() > 1 && clauses[0].chars().count() <= 3 && clauses[0].chars().all(kana) {
        clauses.remove(0);
    }
    let mut label = String::new();
    for clause in clauses {
        if label.is_empty() {
            label = cut(&clause);
        } else if columns(&label) + 1 + columns(&clause) <= LABEL_COLUMNS {
            label.push('-');
            label.push_str(&clause);
        } else {
            break;
        }
    }
    match label.trim_end_matches('-') {
        "" => "session".into(),
        label => label.to_string(),
    }
}

/// `clause` cut to fit a sidebar row where a word likely ends: before the
/// last particle, a kana alone after other letters, at least half the row
/// in; else before the last kana that end a word; else where the row ends.
fn cut(clause: &str) -> String {
    let chars: Vec<char> = clause.chars().collect();
    let (mut fit, mut width) = (0, 0);
    let (mut particle, mut ending) = (None, None);
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && width >= LABEL_COLUMNS / 2 && hiragana(c) && !hiragana(chars[i - 1]) {
            match chars.get(i + 1) {
                Some(&next) if hiragana(next) => ending = Some(i),
                _ => particle = Some(i),
            }
        }
        width += columns_of(c);
        if width > LABEL_COLUMNS {
            break;
        }
        fit = i + 1;
    }
    if fit == chars.len() {
        return clause.to_string();
    }
    chars[..particle.or(ending).unwrap_or(fit)].iter().collect()
}

fn hiragana(c: char) -> bool {
    ('\u{3041}'..='\u{309F}').contains(&c)
}

fn kana(c: char) -> bool {
    hiragana(c) || ('\u{30A0}'..='\u{30FF}').contains(&c)
}

/// Whether `c` belongs to a script written without spaces between words.
fn unspaced(c: char) -> bool {
    kana(c)
        || ('\u{3400}'..='\u{4DBF}').contains(&c)
        || ('\u{4E00}'..='\u{9FFF}').contains(&c)
        || ('\u{F900}'..='\u{FAFF}').contains(&c)
        || c == '々'
}

/// How many columns of a row `c` takes.
fn columns_of(c: char) -> usize {
    let wide = unspaced(c)
        || ('\u{AC00}'..='\u{D7AF}').contains(&c)
        || ('\u{FF01}'..='\u{FF60}').contains(&c);
    if wide { 2 } else { 1 }
}

fn columns(text: &str) -> usize {
    text.chars().map(columns_of).sum()
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

/// How a tab is kept for the next run of the window: `<environment>@` and
/// what it is in that environment, or what it is alone in a window without
/// an environment, as every tab was kept before environments. A draft holds
/// nothing to come back to, a viewer opens again from the tree in a moment,
/// and an image drawn is gone with the window that kept it, so none is kept.
pub fn kept_name(tab: &Tab) -> Option<String> {
    let name = match tab {
        Tab::Session(_, id) => format!("session:{id}"),
        Tab::Terminal(_, id) => format!("terminal:{id:x}"),
        Tab::Draft { .. } | Tab::Viewer(..) | Tab::Drawn { .. } => return None,
    };
    let environment = tab.environment();
    Some(if environment.is_unbound() {
        name
    } else {
        format!("{environment}@{name}")
    })
}

/// The tab a name [`kept_name`] gave stands for, in `window`'s environment
/// when the name was kept without one.
pub fn from_kept(name: &str, window: &EnvironmentId) -> Option<Tab> {
    let name = name.trim();
    // An environment's id holds no `:`, so a name that starts with what it
    // is was kept without one, whatever `@` its session id holds.
    let (environment, name) = match name.split_once('@') {
        Some((environment, rest)) if !environment.contains(':') => {
            (EnvironmentId::new(environment), rest)
        }
        _ => (window.clone(), name),
    };
    if let Some(id) = name.strip_prefix("session:") {
        return Some(Tab::Session(environment, id.to_string()));
    }
    let id = name.strip_prefix("terminal:")?;
    u128::from_str_radix(id, 16)
        .ok()
        .map(|id| Tab::Terminal(environment, id))
}

/// The content kept for the next run, as it is written down: each window by
/// the name [`kept_name`] gave its tab.
pub fn kept_text(kept: &Kept<String>) -> String {
    serde_json::to_string(kept).expect("the content kept is plain data")
}

/// The content [`kept_text`] wrote. A window kept only the one tab it
/// showed before the rest of the content was kept, by its name alone, which
/// reads as that tab alone; anything else reads as nothing kept.
pub fn read_kept(text: &str) -> Kept<String> {
    if let Ok(kept) = serde_json::from_str(text) {
        return kept;
    }
    match from_kept(text, &EnvironmentId::unbound()) {
        Some(_) => Kept {
            tabs: vec![KeptTab {
                root: KeptNode::Window(text.trim().to_string()),
                focus: 0,
                preview: false,
            }],
            current: 0,
        },
        None => Kept::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{Axis, Content};
    use crate::testing::here;

    #[test]
    fn options_chosen_with_nothing_open_go_to_the_next_draft_only() {
        let chosen = DraftOptions {
            profile: None,
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
    fn a_shell_or_a_viewer_shows_no_prompt_and_a_session_a_draft_or_nothing_open_does() {
        let draft = Tab::Draft {
            environment: here(),
            worktree: PathBuf::from("/w"),
            id: Default::default(),
        };
        assert!(!shows_prompt(Some(&Tab::Terminal(here(), 1))));
        assert!(!shows_prompt(Some(&Tab::Viewer(here(), "/a.png".into()))));
        assert!(!shows_prompt(Some(&Tab::Drawn {
            environment: here(),
            session: "a".into(),
            id: 1,
            width: 2,
            height: 2,
        })));
        assert!(shows_prompt(Some(&Tab::Session(here(), "a".into()))));
        assert!(shows_prompt(Some(&draft)));
        assert!(shows_prompt(None));
    }

    #[test]
    fn a_session_s_or_a_shell_s_tab_is_kept_by_name_and_a_draft_s_or_a_drawn_image_s_is_not() {
        for tab in [
            Tab::Session(here(), "ab12".into()),
            Tab::Terminal(here(), 0xff),
        ] {
            let name = kept_name(&tab).unwrap();
            assert_eq!(from_kept(&name, &here()), Some(tab), "{name}");
        }
        let draft = Tab::Draft {
            environment: here(),
            worktree: "/src".into(),
            id: Default::default(),
        };
        assert_eq!(kept_name(&draft), None);
        let drawn = Tab::Drawn {
            environment: here(),
            session: "a".into(),
            id: 1,
            width: 2,
            height: 2,
        };
        assert_eq!(kept_name(&drawn), None);
        assert_eq!(from_kept("shell:not hex", &here()), None);
        assert_eq!(from_kept("something else", &here()), None);
    }

    #[test]
    fn a_tab_is_kept_with_its_environment_and_reads_back_in_it_in_any_window() {
        let here = EnvironmentId::new("here");
        let away = EnvironmentId::new("away");
        for tab in [
            Tab::Session(away.clone(), "ab12".into()),
            Tab::Terminal(away.clone(), 0xff),
        ] {
            let name = kept_name(&tab).unwrap();
            assert_eq!(from_kept(&name, &here), Some(tab), "{name}");
        }
    }

    #[test]
    fn a_tab_kept_before_environments_reads_back_in_the_window_s_environment() {
        let here = EnvironmentId::new("here");
        assert_eq!(
            from_kept("session:ab12", &here),
            Some(Tab::Session(here.clone(), "ab12".into()))
        );
        assert_eq!(
            from_kept("terminal:ff", &here),
            Some(Tab::Terminal(here, 0xff))
        );
    }

    #[test]
    fn a_tab_of_a_window_without_an_environment_is_kept_as_before() {
        let unbound = EnvironmentId::unbound();
        assert_eq!(
            kept_name(&Tab::Session(unbound.clone(), "ab12".into())).as_deref(),
            Some("session:ab12")
        );
        assert_eq!(
            kept_name(&Tab::Terminal(unbound, 0xff)).as_deref(),
            Some("terminal:ff")
        );
    }

    #[test]
    fn the_content_kept_reads_back_as_it_was_written() {
        let mut windows = Content::default();
        windows.open(Tab::Session(here(), "a".into()));
        windows.split(Axis::Row, Tab::Terminal(here(), 7));
        windows.open_tab(Tab::Session(here(), "b".into()));
        let kept = windows.kept(kept_name);
        assert_eq!(read_kept(&kept_text(&kept)), kept);
    }

    #[test]
    fn an_old_file_holding_one_kept_tab_reads_as_that_tab_alone_and_anything_else_as_nothing() {
        let one = Kept {
            tabs: vec![KeptTab {
                root: KeptNode::Window("session:a".to_string()),
                focus: 0,
                preview: false,
            }],
            current: 0,
        };
        assert_eq!(read_kept("session:a\n"), one);
        assert_eq!(read_kept(""), Kept::default());
        assert_eq!(read_kept("{not json"), Kept::default());
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
    fn a_path_in_the_prompt_says_what_its_file_name_says() {
        assert_eq!(
            label_from(
                "/Users/me/notes/2026-10-23-1502-adroit-tree-content-01-layout.md を実装して"
            ),
            "adroit-tree-content"
        );
        assert_eq!(
            label_from("Follow ~/notes/login-flow.md please"),
            "follow-login-flow"
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
    fn a_word_said_twice_is_labelled_once() {
        assert_eq!(
            label_from("overview: the details overview is wrong"),
            "overview-details-wrong"
        );
    }

    #[test]
    fn a_url_says_what_the_last_part_of_its_path_names() {
        assert_eq!(
            label_from("review https://github.com/adroit-app/Adroit/pull/12"),
            "review-adroit"
        );
        assert_eq!(label_from("read https://example.com/"), "read-example");
        assert_eq!(
            label_from("https://github.com/owner/felis-gui/issues/3 を直して"),
            "felis-guiを直して"
        );
    }

    #[test]
    fn a_prompt_with_words_in_other_scripts_keeps_its_ascii_words_among_them() {
        assert_eq!(
            label_from("認証の テスト: add PKCE!"),
            "認証のテスト-add-pkce"
        );
        assert_eq!(label_from("felis の GUI を見て"), "felisのguiを見て");
    }

    #[test]
    fn a_prompt_without_ascii_words_is_labelled_by_its_own_letters() {
        assert_eq!(label_from("日本語だけ"), "日本語だけ");
        assert_eq!(label_from("Тест входа"), "тест-входа");
    }

    #[test]
    fn unspaced_text_too_long_for_a_row_is_cut_before_a_particle() {
        // (prompt, label)
        let cases = [
            (
                "ログインのテストが落ちる理由を調べてください",
                "ログインのテスト",
            ),
            ("ブランドイメージを作りなさい", "ブランドイメージ"),
            // With no particle far enough in, before the kana that end a word.
            ("大量にセッション走ってたけど", "大量にセッション走"),
            ("各セッションの状況はどう？", "各セッションの状況はどう"),
        ];
        for (prompt, label) in cases {
            assert_eq!(label_from(prompt), label, "{prompt:?}");
        }
    }

    #[test]
    fn a_clause_too_long_to_fit_after_another_is_left_out() {
        assert_eq!(
            label_from("認証のテストを直して、確認"),
            "認証のテストを直して"
        );
    }

    #[test]
    fn a_short_kana_clause_that_leads_into_the_prompt_is_left_out() {
        assert_eq!(
            label_from("あれ、大量にセッション走ってたけど"),
            "大量にセッション走"
        );
        assert_eq!(label_from("ねえ"), "ねえ");
    }

    #[test]
    fn punctuation_in_unspaced_text_becomes_a_single_dash() {
        assert_eq!(label_from("認証、直して。"), "認証-直して");
        assert_eq!(label_from("「認証」の　テスト"), "認証-のテスト");
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
