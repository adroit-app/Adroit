//! What an update asks the world to do, as values the window runs.

use std::time::Duration;

use adroit_protocol::{Request, Typing};

use std::collections::BTreeSet;
use std::path::PathBuf;

use adroit_model::{EnvironmentId, Key};

use crate::actions::Action;
use crate::content::{Opening, Tab, TerminalAsk, WindowId};
use crate::details::Scroll;
use crate::layout::Layout;
use crate::prompt::send::{Start, Taken};
use crate::viewers::ViewerAction;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Send `request` to the work host of `environment`.
    Host {
        environment: EnvironmentId,
        request: Request,
    },
    /// Draw the window at this zoom.
    Zoom(f64),

    // The details panel.
    /// Open `target`, an address or a file of `environment`, in the app
    /// macOS opens it with.
    Open {
        environment: EnvironmentId,
        target: String,
    },
    /// Show the file at `path` of `environment` in the Finder.
    Reveal {
        environment: EnvironmentId,
        path: String,
    },
    /// Put `text` on the clipboard and say `done`.
    Copy { text: String, done: String },
    /// Scroll the details panel's sheet.
    ScrollDetails(Scroll),
    /// Show the session by this id of this environment in the middle.
    ShowSession(EnvironmentId, String),
    /// Show `thing` in the content as `how` says, the keys staying where
    /// they are.
    Show { thing: Tab, how: Opening },
    /// Tell the user this in the status line.
    Notice(String),

    /// Draw the consoles in these colours.
    Palette(adroit_protocol::Colours),

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
    /// Open a terminal running the CLI's own login where this profile runs
    /// it, and show it.
    OpenLogin {
        profile: adroit_model::profile::ProfileId,
    },
    /// The prompt's text for a draft no window shows any longer goes to
    /// what the focused window shows now, or to nothing open.
    PromptClose(Option<Tab>),
    /// Start writing in the prompt, whatever mode it was left in.
    StartWriting,
    /// Keep the content as this for the next run, each window by the name
    /// its tab is kept by.
    KeepContent(crate::content::Kept<String>),
    /// Keep the panes and the images drawn of `environment` only of these
    /// sessions and terminals.
    KeepPanes {
        environment: EnvironmentId,
        sessions: BTreeSet<String>,
        terminals: BTreeSet<u128>,
    },
    /// Continue this conversation, which an agent of `environment` kept, in
    /// a new session there.
    Resume {
        environment: EnvironmentId,
        past: adroit_model::Past,
    },
    /// Tell the user, outside the window, that these sessions started
    /// waiting, each by its name and what it waits for.
    NotifyWaiting(Vec<(String, String)>),

    // The tree.
    /// Give the filter's field the keys, or take them from it.
    FilterFocus(bool),

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
    /// Keep a copy of each of these image files, in this order, for the
    /// prompt.
    KeepImageFiles(Vec<PathBuf>),
    /// Run `action`, which `key` ran, where the keys are now.
    PerformKey(Action, Key),
    /// Ask the session in this tab for its input with `Ctrl-g`.
    AskInput(Tab),
    /// Tell the window with [`crate::msg::Msg::AskInput`] after this long.
    AskInputAfter(Duration),
    /// Tell the window with [`crate::msg::Msg::PullLapsed`] after this long
    /// that the request numbered `pull` lapsed.
    LapsePull { pull: u64, after: Duration },
    /// Tell the window with [`crate::msg::Msg::ComposeLapsed`] after this long
    /// that the host was asked to compose the prompt numbered `asked`.
    LapseCompose { asked: u64, after: Duration },

    // The prompt.
    /// Put the prompt sent one before, or one after when not `back`, among
    /// those `sent`, oldest first, in the prompt, if it still shows `tab`.
    PromptRecall {
        tab: Option<Tab>,
        back: bool,
        sent: Vec<String>,
    },
    /// The prompt shows what is written for this tab, or for nothing open.
    PromptShow(Option<Tab>),
    /// Take this image away from the prompt shown.
    RemoveImage(PathBuf),
    /// Take what is written from the prompt to send it, telling the window
    /// with [`crate::msg::Msg::PromptTaken`], or
    /// [`crate::msg::Msg::PromptEmpty`] when it holds nothing.
    TakePrompt,
    /// Send this to the session it was written for, telling the window with
    /// [`crate::msg::Msg::PromptWent`] whether it went.
    SendPrompt(Box<Taken>),
    /// Start a session from a draft with its first prompt, telling the
    /// window with [`crate::msg::Msg::PromptWent`] whether it started.
    StartDraft(Box<Start>),
    /// Offer these after `@` in the prompt.
    PromptOffer(Vec<crate::complete::Offer>),
    /// Walk a path after `@` in the prompt from this worktree, or from
    /// nowhere.
    PromptRoot(Option<PathBuf>),
    /// Offer these after `/` in the prompt.
    PromptCommands(Vec<adroit_model::Command>),
    /// A prompt went: stepping through those sent starts again from the
    /// newest.
    PromptSent,
    /// Put back what could not be sent, for the tab it was written for.
    PromptRestore {
        tab: Option<Tab>,
        text: String,
        images: Vec<PathBuf>,
    },

    // The actions.
    /// Hand this action to the viewer of the window by this id, which shows
    /// a file of `environment`.
    Viewer {
        environment: EnvironmentId,
        window: WindowId,
        action: ViewerAction,
    },
    /// Tab pressed in the prompt as `key`: to Neovim, or to the session for
    /// the prompt it suggests, once the window knows whether the prompt is
    /// empty. `session` while a session's pane can take the key,
    /// `hands_back` while that session hands its input back, `no_images`
    /// while none is attached; `tab` is what the focused window shows.
    TakeSuggestion {
        key: Key,
        session: bool,
        hands_back: bool,
        no_images: bool,
        tab: Option<Tab>,
    },
    /// Type this into the program of the session `tab` shows, attaching to
    /// it if need be.
    TypeInto { tab: Tab, typing: Vec<Typing> },
    /// End the shell by this id of `environment` and close its windows.
    EndShell {
        environment: EnvironmentId,
        id: u128,
    },
    /// Open the Settings window.
    OpenSettings,
    /// Close the window.
    CloseWindow,
    /// Open another window of the instance.
    OpenWindow,

    // The overlays.
    /// Run `action` where the keys are now.
    Perform(Action),
    /// Replace the sessions' daemon with one of this build.
    RestartDaemon,
}
