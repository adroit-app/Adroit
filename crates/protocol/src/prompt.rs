//! What the prompt asks of the work host: the prompts sent before in a
//! project, read afresh each time the user steps back through them, and the
//! commands `/` offers where the prompt's session runs, and how a prompt is
//! typed into the session it goes to.

use std::path::PathBuf;

use adroit_model::{Agent, Command};
use serde::{Deserialize, Serialize};

/// The prompts an agent kept as sent in a project, oldest first, read for
/// the request numbered `asked`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryRead {
    pub asked: u64,
    pub project: PathBuf,
    pub sent: Vec<String>,
}

/// The agent, the folder of a profile's own it runs in, none for its own
/// environment, and the directory whose commands `/` offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandsSight {
    pub agent: Agent,
    pub home: Option<PathBuf>,
    pub dir: PathBuf,
}

/// The commands `/` offers for an agent, in a login and a directory: its own, and those
/// it was found to offer there, however old.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandsRead {
    pub agent: Agent,
    pub home: Option<PathBuf>,
    pub dir: PathBuf,
    pub commands: Vec<Command>,
}

impl CommandsRead {
    /// Whether this was read for what `sight` names.
    pub fn is_for(&self, sight: &CommandsSight) -> bool {
        self.agent == sight.agent && self.home == sight.home && self.dir == sight.dir
    }
}

/// One step of typing into a session's program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Typing {
    /// Type this, as keys are typed.
    Text { text: String },
    /// Paste this.
    Paste { text: String },
    /// Press the key by this name.
    Key { name: String },
    /// Leave the program this long, in milliseconds, before the next step.
    Pause { ms: u64 },
}

/// How a prompt is typed into a session of the agent it goes to, composed
/// for the request numbered `asked`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Composed {
    pub asked: u64,
    /// The agent runs the prompt as a shell command.
    pub shell: bool,
    /// An image attached to it is no longer there.
    pub image_missing: bool,
    /// The steps that type it, Enter last.
    pub typing: Vec<Typing>,
}

/// What the host read for the prompt.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PromptWork {
    /// The history last read; none until one is asked for.
    pub history: Option<HistoryRead>,
    /// The commands last read for `/`; none until some are.
    pub commands: Option<CommandsRead>,
    /// The prompt last composed; none until one is asked for.
    pub composed: Option<Composed>,
}

/// One change to what the host read for the prompt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromptChange {
    History { read: HistoryRead },
    Commands { read: CommandsRead },
    Composed { composed: Composed },
}

impl PromptWork {
    /// What was read with `change` made; whether that changed it.
    pub fn apply(&mut self, change: PromptChange) -> bool {
        match change {
            PromptChange::History { read } => {
                if self.history.as_ref() == Some(&read) {
                    return false;
                }
                self.history = Some(read);
                true
            }
            PromptChange::Commands { read } => {
                if self.commands.as_ref() == Some(&read) {
                    return false;
                }
                self.commands = Some(read);
                true
            }
            PromptChange::Composed { composed } => {
                if self.composed.as_ref() == Some(&composed) {
                    return false;
                }
                self.composed = Some(composed);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_request_s_history_replaces_the_one_before() {
        let mut work = PromptWork::default();
        let read = |asked| PromptChange::History {
            read: HistoryRead {
                asked,
                project: "/w".into(),
                sent: vec!["fix it".into()],
            },
        };
        assert!(work.apply(read(1)));
        assert!(!work.apply(read(1)));
        assert!(work.apply(read(2)), "the same lines read again are news");
        assert_eq!(work.history.map(|h| h.asked), Some(2));
    }

    #[test]
    fn the_commands_read_last_replace_those_before_and_say_what_they_are_for() {
        let mut work = PromptWork::default();
        let read = |dir: &str, names: &[&str]| PromptChange::Commands {
            read: CommandsRead {
                agent: Agent::Claude,
                home: None,
                dir: dir.into(),
                commands: names
                    .iter()
                    .map(|n| Command {
                        name: (*n).into(),
                        description: String::new(),
                        argument_hint: String::new(),
                    })
                    .collect(),
            },
        };
        assert!(work.apply(read("/a", &["compact"])));
        assert!(!work.apply(read("/a", &["compact"])));
        assert!(work.apply(read("/b", &["compact", "review"])));
        let held = work.commands.unwrap();
        let sight = |agent, dir: &str| CommandsSight {
            agent,
            home: None,
            dir: dir.into(),
        };
        assert!(held.is_for(&sight(Agent::Claude, "/b")));
        // Another login of the agent offers commands of its own.
        assert!(!held.is_for(&CommandsSight {
            home: Some("/w".into()),
            ..sight(Agent::Claude, "/b")
        }));
        assert!(!held.is_for(&sight(Agent::Claude, "/a")));
        assert!(!held.is_for(&sight(Agent::Codex, "/b")));
    }
}
