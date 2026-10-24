use std::collections::BTreeMap;

use std::path::PathBuf;

use adroit_model::{Agent, Process, Session, TerminalSession};
use serde::{Deserialize, Serialize};

use crate::{Answer, Ask, Sight, Typing, Version, WorkPatch, WorkSnapshot};

/// What the window asks of the work host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// The first thing a window says: the versions it speaks.
    Hello { versions: Vec<Version> },
    /// Send the whole work again, as after losing track of the patches.
    Snapshot,
    /// Keep read what the window now shows, in place of what it showed.
    Watch(Box<Sight>),
    /// Read again now what the window shows, as after the window changed it.
    ReadAgain,
    /// List the sessions again now, asking git afresh of every directory
    /// they run in, as after the window started, ended or renamed one.
    Relist,
    /// When the user last typed into each session the window shows, in
    /// milliseconds since the epoch, by the session's terminal.
    Typed {
        #[serde(with = "crate::wire::wide_keys")]
        inputs: BTreeMap<u128, u64>,
    },
    /// The window started `terminal`, which the next list holds too.
    TerminalOpened {
        #[serde(with = "crate::wire::terminal")]
        terminal: TerminalSession,
    },
    /// The window ended the terminal `id`, which the next list leaves out.
    TerminalClosed {
        #[serde(with = "crate::wire::wide")]
        id: u128,
    },
    /// End `session`; its agent keeps its conversation.
    Stop { session: Box<Session> },
    /// Kill `process`, which runs in `session`.
    Kill {
        session: Box<Session>,
        process: Process,
    },
    /// Name the session `id`, called `was` until now, `name`, and tell its
    /// `agent` so too.
    Rename {
        id: String,
        name: String,
        was: String,
        agent: Agent,
    },
    /// The screen of the session `session`, whose prompt goes to `agent`,
    /// was drawn anew in the window's console.
    Redrawn { session: String, agent: Agent },
    /// Ask again for the commands `/` offers where the window has in sight,
    /// if those held are old, as a `/` typed asks.
    CommandsWanted,
    /// Compose how `text`, with `images` attached, is typed into a session of
    /// `agent`, for the request numbered `asked`.
    Compose {
        asked: u64,
        agent: Agent,
        text: String,
        images: Vec<PathBuf>,
    },
    /// Read the prompts `agent` kept as sent in `project`, in the login whose
    /// folder is `home`, none for the agent's own environment,
    /// for the request numbered `asked`.
    History {
        asked: u64,
        agent: Agent,
        home: Option<PathBuf>,
        project: PathBuf,
    },
    /// What `ask` asks, answered once by `asked`, the number its asker gave
    /// it.
    Ask { asked: u64, ask: Box<Ask> },
    /// Let the program of the session or terminal `id` go, which a window
    /// attached to and told its colours.
    Release {
        #[serde(with = "crate::wire::wide")]
        id: u128,
    },
    /// End the terminal `id`.
    EndTerminal {
        #[serde(with = "crate::wire::wide")]
        id: u128,
    },
}

/// What the work host tells a window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// The host works with the window in `version`.
    Welcome { version: Version },
    /// The host shares no version with the window; it speaks these.
    Refused { versions: Vec<Version> },
    /// The whole work, replacing what the window had.
    Snapshot(Box<WorkSnapshot>),
    /// A change to the work, in the order the host made it.
    Patch(WorkPatch),
    /// How a request the host carried out went, for the user.
    Told(Told),
    /// Type this into the session `session`'s program, which the window
    /// holds a pane of.
    Type {
        session: String,
        typing: Vec<Typing>,
    },
    /// What came of the ask numbered `asked`, for the one that asked it.
    Answer { asked: u64, answer: Answer },
}

/// What the host says of a request it carried out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Told {
    /// Said for a while.
    Notice { text: String },
    /// Said until something else is.
    Failure { text: String },
}
