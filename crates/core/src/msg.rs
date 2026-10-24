//! What can happen to the window: what the user does, what the window is
//! told, and what the work host says.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;

use adroit_model::{Agent, Key, TerminalSession};
use adroit_protocol::Event;

use crate::actions::Action;
use crate::console::Probe;
use crate::content::{Area, Border, DraftOptions, Opening, Tab, WindowId};
use crate::details::{self, Sheet};
use crate::keymap::Context;
use crate::layout::{Layout, Side};
use crate::overlay::confirm::Ask;
use crate::overlay::context_menu::Target;
use crate::overlay::rename::Place as RenamePlace;
use crate::prompt::send::Taken;
use crate::tree;
use crate::tree::nav::{Item, Place};
use crate::zoom;

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    /// The window opened, with the home directory the paths it draws
    /// shorten to `~`.
    Opened {
        home: PathBuf,
    },
    /// The window was brought to the front, or sent to the back.
    WindowFocused(bool),
    /// A key asked to zoom.
    Zoom(zoom::Step),

    // The side panels.
    /// The layout kept from the last run was read.
    PanelsKept(Layout),
    /// The window's body was drawn this many pixels wide.
    BodyResized(f64),
    /// Show or hide a side panel.
    PanelToggle(Side),
    /// Show a side panel, if it is hidden.
    PanelShow(Side),
    /// A side panel back at its width as designed.
    PanelReset(Side),
    /// The pointer took a side panel's edge at `x`.
    PanelDragStart {
        side: Side,
        x: f64,
    },
    /// The pointer moved to `x`.
    PanelDragMove {
        x: f64,
    },
    /// The pointer let go, or left the window.
    PanelDragEnd,

    // The status line.
    /// Show this in the status line until something replaces it, or clear it.
    Status(Option<String>),
    /// Show this in the status line for a while.
    Notice(String),
    /// The notice numbered `notice`, saying `text`, has been shown long enough.
    NoticeLapsed {
        notice: u64,
        text: String,
    },
    /// A source that last failed with `last` now fails with `now`, or reads.
    Failing {
        last: Option<String>,
        now: Option<String>,
    },

    // The keys help.
    /// Show the keys help for these areas, nearest first.
    KeysHelpOpen(Vec<Context>),
    KeysHelpClose,

    /// The work host said `Event`.
    Host(Event),

    // The work.
    /// List the sessions again now, looking afresh at where they run.
    Relist,
    /// When the user last typed into each session shown, by its terminal.
    Typed(BTreeMap<u128, u64>),
    /// The window started this terminal.
    TerminalOpened(TerminalSession),
    /// The window ended the terminal by this id.
    TerminalClosed(u128),

    // The content.
    /// The window opened, with the content as it last closed and where a
    /// session started with nothing open runs.
    ContentOpened {
        kept: crate::content::Kept<String>,
        scratch: PathBuf,
    },
    /// The keys go to this area.
    Focus(Area),
    /// Show `thing` where the user is, or as `how` says, in the worktree
    /// `place` when one is given.
    Show {
        thing: Tab,
        place: Option<Place>,
        how: Option<Opening>,
    },
    /// Show the session by this id wherever it runs, and select its row.
    ShowSession(String),
    /// Show a new session's draft in this worktree; `write` gives the keys
    /// to the prompt to write its first prompt.
    Draft {
        worktree: PathBuf,
        write: bool,
    },
    /// Open what a row stands for, where `how` says; `take_keys` gives the
    /// keys to what opened.
    OpenRow {
        item: Item,
        how: Option<Opening>,
        take_keys: bool,
    },
    /// A row was clicked: it opens as `cursor.go` opens it.
    RowClicked(Item),
    /// A key or a menu asked the content to do this.
    Window(Action),
    /// A window was clicked.
    WindowFocus(WindowId),
    /// A tab was clicked, by its place counted from 0.
    TabShow(usize),
    /// A menu was opened on the tab of a window showing this.
    TabReveal(Tab),
    /// `from` shows as `to` from now on, as a draft does once it starts.
    Replace {
        from: Tab,
        to: Tab,
    },
    /// The shell by this id closed.
    ShellClosed(u128),
    /// The pointer took a border between windows at `from` along its axis,
    /// the split it divides drawn `px` long.
    WindowDragStart {
        border: Border,
        from: f64,
        px: f64,
    },
    /// The pointer moved to `pos` along the border's axis.
    WindowDragMove(f64),
    /// The pointer let go, or left the window.
    WindowDragEnd,
    /// The agent chosen for this draft, or for the next one.
    DraftAgent {
        tab: Option<Tab>,
        agent: Agent,
    },
    /// The options chosen for this draft, or for the next one.
    DraftOptions {
        tab: Option<Tab>,
        options: DraftOptions,
    },
    /// The sessions shown live here, or being attached to.
    PanesLive(BTreeSet<String>),

    // The tree.
    /// Select this row, or none.
    TreeSelect(Option<Item>),
    /// A key was pressed on the tree.
    TreeKey(tree::Key),
    /// Show the rows under `row`, or hide them.
    TreeFold {
        row: Item,
        open: bool,
    },
    TreeFoldToggle(Item),
    /// The filter now holds this.
    TreeFilter(String),
    /// The filter's field took the keys, or gave them up.
    TreeFiltering(bool),
    /// List every worktree, or only those where something runs.
    TreeEveryWorktree,

    // The details panel.
    /// What the window tells the details panel that the tree does not.
    DetailsShown(Box<details::Told>),
    /// A sheet was chosen.
    DetailsSheet(Sheet),
    /// The sheet this far along was asked for.
    DetailsSheetStep(isize),
    /// A key was pressed on the panel.
    DetailsKey(details::Key),
    /// A row of the overview was clicked.
    DetailsClick(usize),
    /// A row of the changes or the log was clicked.
    DetailsOpenRow(usize),
    /// A row of the overview was picked to open a menu on.
    DetailsSelect(usize),
    /// Back out of the row opened.
    DetailsBack,
    /// What the panel shows changed by the window's hand; read it again now.
    DetailsReadAgain,

    // The picker.
    /// Open the picker that goes to a session or starts one.
    PickerOpen,
    /// Open the list of past conversations on where the focused window
    /// works.
    ResumeOpen,
    PickerClose,
    /// Its query field now holds this.
    PickerQuery(String),
    /// A key its query field does not type with, or any key in its
    /// directory tree.
    PickerKey(Key),
    /// A row was chosen, by its place among those drawn.
    PickerChoose(usize),
    /// A directory was chosen in the directory tree.
    PickerDir(PathBuf),
    /// List every directory's conversations, or the directory's again.
    PickerEvery,

    /// Run `action`, which `key` ran or a click when none, where the keys
    /// are now, with what the window knows of its programs then.
    Perform {
        action: Action,
        key: Option<Key>,
        probe: Box<Probe>,
    },

    // The console.
    /// A key pressed where no field of the window's own takes it, with what
    /// the window knows of its programs then.
    Key {
        key: Key,
        probe: Box<Probe>,
    },
    /// Text typed, or pasted, where the keys are.
    Text {
        text: String,
        pasted: bool,
    },
    /// An image pasted, of the type `mime`, its bytes in base64.
    ImagePasted {
        mime: String,
        data: String,
    },
    /// A key went to the session shown as it is.
    ConsoleTyped,
    /// The input of the session this tab shows may hold text: a pane just
    /// attached to it, or a key may have typed there.
    InputMayHold(Tab),
    /// A sequence of keys begun is given up, as a menu bar entry runs.
    SequenceDropped,
    /// The next key goes to the session as it is.
    ConsoleSendNext,
    /// Every key goes to the session as it is, until Escape.
    ConsolePassEveryKey,
    /// The reader shows the console and takes its keys.
    ReadingStarted,
    /// The reader's Neovim quit, and the console is shown live until it
    /// runs again.
    ReaderQuit,
    /// The prompt's Neovim runs, or quit.
    PromptRuns(bool),
    /// The window answers the editor's requests, and so is handed a
    /// session's input.
    EditorAnswers,
    /// Ask the session shown for its input, if it may hold text.
    AskInput,
    /// The session in this tab was asked for its input.
    InputAsked(Tab),
    /// The request for a session's input numbered so lapsed.
    PullLapsed(u64),
    /// The session's input was taken, or the request declined.
    InputTaken,

    // The prompt.
    /// Step back through the prompts `agent` kept as sent where the shown
    /// tab's are, or forth.
    Recall {
        back: bool,
        agent: Agent,
    },
    /// A `/` was typed in the prompt: the commands it offers are asked for
    /// again if they are old.
    CommandsWanted,
    /// Neovim's mode, as the class the footer draws it with and its word.
    PromptMode(&'static str, &'static str),
    /// What is written is a shell line for the session, or no longer.
    PromptShell(bool),
    /// The images attached to the prompt shown.
    PromptImages(Vec<PathBuf>),
    /// The tabs with something written for them kept aside.
    Drafted(HashSet<Tab>),
    /// Select this image attached, or none, the keys going to the prompt.
    ImageSelect(Option<PathBuf>),
    /// Select the first image attached, where text goes to the prompt.
    ImagesChoose,
    /// Send what is written in the prompt.
    PromptSend,
    /// The prompt held nothing to send.
    PromptEmpty,
    /// This was taken from the prompt to send.
    PromptTaken(Box<Taken>),
    /// The prompt handed over to send went, or did not.
    PromptWent(bool),
    /// The host has been asked long enough to compose the prompt numbered so.
    ComposeLapsed(u64),

    // The overlays.
    /// A key while something drawn over the window holds the keys.
    HeldKey(Key),
    /// A right click opened a menu at `x`, `y` on `target`.
    MenuOpen {
        x: f64,
        y: f64,
        target: Target,
    },
    MenuClose,
    /// Run `action` on `target`, as its key would there.
    MenuRun {
        target: Target,
        action: Action,
    },
    /// Ask this, holding the keys until it is answered.
    Confirm(Box<Ask>),
    /// Ask to end the selected row's session, from the tree, or else the
    /// one shown.
    ConfirmStop {
        selection: bool,
    },
    /// The question answered yes or no.
    ConfirmAnswer(bool),
    /// Start typing a new name for the session `id` where `place` shows it.
    RenameStart {
        id: String,
        place: RenamePlace,
    },
    /// The field for the session `id` where `place` shows it now holds
    /// `text`.
    RenameText {
        id: String,
        place: RenamePlace,
        text: String,
    },
    /// The field left, the name typed kept or given up.
    RenameEnd {
        id: String,
        place: RenamePlace,
        commit: bool,
    },
    /// How the sessions' daemon stands, as the banner would say it.
    DaemonStanding(Option<String>),
    /// The daemon was restarted, and now stands so.
    DaemonRestarted(Option<String>),
    /// The banner was put away.
    BannerClose,
}
