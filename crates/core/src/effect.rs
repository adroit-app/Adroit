//! What an update asks the world to do, as values the window runs.

use std::time::Duration;

use adroit_protocol::Request;

use std::collections::BTreeSet;
use std::path::PathBuf;

use adroit_model::{Key, Process, Session};

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

    // The console.
    /// Send this key to the focused window's program.
    PaneKey(Key),
    /// Send this key, which scrolls what it shows, to the focused window's
    /// program, and draw it again.
    PaneScrollKey(Key),
    /// Type or paste this into the focused window's program.
    PaneText { text: String, pasted: bool },
    /// Turn the wheel over the focused window's program this many lines,
    /// back when positive, and draw it again.
    PaneWheel(i32),
    /// Give this key to the console's reader.
    ReaderKey(Key),
    /// Start reading the console, giving the reader these keys, the ones that
    /// started it.
    ReadConsole(Vec<Key>),
    /// The console shown live again: a program that scrolls itself is
    /// returned to its latest output.
    ConsoleLive,
    /// Give this key to the prompt's Neovim.
    PromptKey(Key),
    /// Type or paste this into the prompt.
    PromptText { text: String, pasted: bool },
    /// Keep an image pasted, of the type `mime`, its bytes in base64, for the
    /// prompt.
    KeepImage { mime: String, data: String },
    /// Run `action`, which `key` ran, where the keys are now.
    PerformKey(Action, Key),
    /// Ask the session in this tab for its input with `Ctrl-g`.
    AskInput(Tab),
    /// Tell the window with [`crate::msg::Msg::AskInput`] after this long.
    AskInputAfter(Duration),
    /// Tell the window with [`crate::msg::Msg::PullLapsed`] after this long
    /// that the request numbered `pull` lapsed.
    LapsePull { pull: u64, after: Duration },

    // The prompt.
    /// Put the prompt sent one before, or one after when not `back`, among
    /// those `sent`, oldest first, in the prompt, if it still shows `tab`.
    PromptRecall {
        tab: Option<Tab>,
        back: bool,
        sent: Vec<String>,
    },
    /// Take this image away from the prompt shown.
    RemoveImage(PathBuf),

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
