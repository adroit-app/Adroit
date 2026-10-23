//! What an update asks the world to do, as values the window runs.

use std::time::Duration;

use adroit_protocol::Request;

use std::collections::BTreeSet;

use adroit_model::{Process, Session};

use crate::actions::Action;
use crate::content::{Tab, TerminalAsk};
use crate::details::{Scroll, Shot};
use crate::layout::Layout;
use crate::overlay::rename::Rename;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Send `Request` to the work host.
    Host(Request),
    /// Draw the window at this zoom.
    Zoom(f64),

    // The details panel.
    /// Open `target`, an address or a file, in the app macOS opens it with.
    Open(String),
    /// Show the file at this path in the Finder.
    Reveal(String),
    /// Put `text` on the clipboard and say `done`.
    Copy { text: String, done: String },
    /// Scroll the details panel's sheet.
    ScrollDetails(Scroll),
    /// Show the session by this id in the middle.
    ShowSession(String),
    /// Show an image of the overview in a window of its own; one the console
    /// drew is the console's of `session`.
    ShowShot { shot: Shot, session: String },
    /// Tell the user this in the status line.
    Notice(String),

    // The chrome.
    /// Keep this layout of the side panels for the next run.
    KeepLayout(Layout),
    /// Scroll the keys help by this many lines, down when positive.
    ScrollKeysHelp(i32),
    /// Tell the window with [`crate::msg::Msg::NoticeLapsed`], `after` this
    /// long, that the notice numbered `notice` saying `text` was shown.
    LapseNotice {
        notice: u64,
        text: String,
        after: Duration,
    },

    // The content.
    /// Start a shell or a Neovim as asked.
    StartTerminal(TerminalAsk),
    /// The prompt's text for a draft no window shows any longer goes to
    /// what the focused window shows now, or to nothing open.
    PromptClose(Option<Tab>),
    /// Start writing in the prompt, whatever mode it was left in.
    StartWriting,
    /// The keys came into the prompt from elsewhere: the console stops being
    /// read, and the session there is asked for what was typed into it.
    PromptEntered,
    /// Keep this tab as the one shown for the next run, or none.
    KeepShown(Option<Tab>),
    /// Keep the panes and the images drawn only of these sessions and
    /// terminals.
    KeepPanes {
        sessions: BTreeSet<String>,
        terminals: BTreeSet<u128>,
    },
    /// Continue this conversation in a new session.
    Resume(adroit_model::Past),
    /// Tell the user, outside the window, that these sessions started
    /// waiting, each by its name and what it waits for.
    NotifyWaiting(Vec<(String, String)>),

    // The overlays.
    /// Run `action` where the keys are now.
    Perform(Action),
    /// End this session.
    Stop(Session),
    /// Kill `process`, which runs in `session`.
    KillProcess { session: Session, process: Process },
    /// Replace the sessions' daemon with one of this build.
    RestartDaemon,
    /// Name a session anew.
    Rename(Rename),
}
