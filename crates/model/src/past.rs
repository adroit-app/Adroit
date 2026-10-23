use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::Agent;

/// A conversation an agent kept, which a new session can resume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Past {
    pub agent: Agent,
    /// The conversation's id, which the agent resumes it by.
    pub id: String,
    /// The directory it ran in.
    pub cwd: PathBuf,
    /// The name it was given, or else its first prompt.
    pub title: String,
    /// When it was last written to, in milliseconds since the Unix epoch.
    pub at_ms: u64,
}

/// The most a title keeps of what it is made from, in characters.
const TITLE_CHARS: usize = 200;

/// `text` as a title: on one line, its runs of white space made one space,
/// and cut short.
pub fn title_of(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    words.join(" ").chars().take(TITLE_CHARS).collect()
}

/// `pasts` with the most recently written first.
pub fn newest_first(mut pasts: Vec<Past>) -> Vec<Past> {
    pasts.sort_by(|a, b| b.at_ms.cmp(&a.at_ms).then_with(|| a.id.cmp(&b.id)));
    pasts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn past(id: &str, at_ms: u64) -> Past {
        Past {
            agent: Agent::Claude,
            id: id.into(),
            cwd: "/r".into(),
            title: id.into(),
            at_ms,
        }
    }

    #[test]
    fn a_title_is_one_line_cut_short_by_characters() {
        assert_eq!(title_of("  fix\n\tthe  login \n"), "fix the login");
        let long = "あ".repeat(300);
        assert_eq!(title_of(&long).chars().count(), 200);
    }

    #[test]
    fn the_conversation_written_to_last_comes_first() {
        let got = newest_first(vec![past("a", 1), past("b", 3), past("c", 2)]);
        let ids: Vec<&str> = got.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["b", "c", "a"]);
    }
}
