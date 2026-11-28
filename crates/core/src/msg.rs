//! What can happen to the window: what the user does, what the window is
//! told, and what the work host says.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;

use adroit_model::{Agent, EnvironmentId, Key, TerminalSession};
use adroit_protocol::Event;

use crate::actions::Action;
use crate::chrome::panels::layout::{Layout, Side};
use crate::console::Probe;
use crate::content::{Area, Border, DraftOptions, DropAt, Opening, Tab, WindowId};
use crate::details::{self, Component};
use crate::keymap::Context;
use crate::overlay::confirm::Ask;
use crate::overlay::context_menu::{Reach, Target};
use crate::overlay::rename::Place as RenamePlace;
use crate::prompt::send::Taken;
use crate::tree;
use crate::tree::Place;
use crate::tree::nav::Item;
use crate::zoom;

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    /// The window opened, with the home directory the paths it draws
    /// shorten to `~`.
    Opened {
        home: PathBuf,
        /// The environment the window's work host is.
        environment: EnvironmentId,
    },
    /// The window shows `environment` too, called `name`, or the name of one
    /// it shows changed.
    EnvironmentOpened {
        environment: EnvironmentId,
        name: String,
    },
    /// `environment` is no longer kept, or is archived, so the window no
    /// longer lists it; the environment it was started in stays.
    EnvironmentRemoved(EnvironmentId),
    /// `environment`, one the window shows, is on a work machine rather than
    /// this computer, so its files are not on this computer's disk.
    OnWorkMachine(EnvironmentId),
    /// The window was brought to the front, or sent to the back.
    WindowFocused(bool),
    /// A key asked to zoom.
    Zoom(zoom::Step),
    /// This computer's key bindings file, as the window read it now.
    KeysRead(adroit_protocol::ConfigFile),
    /// What this computer makes the details panel of, for every environment.
    PanelChosen(crate::details::DetailsLayout),

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

    /// The work host of `environment` said `event`.
    Host {
        environment: EnvironmentId,
        event: Event,
    },

    // The work.
    /// List the sessions again now, looking afresh at where they run.
    Relist,
    /// When the user last typed into each session of `environment` shown,
    /// by its terminal.
    Typed {
        environment: EnvironmentId,
        inputs: BTreeMap<u128, u64>,
        /// How many milliseconds ago they sent what they wrote to each they
        /// did since the window last said so.
        entered: BTreeMap<u128, u64>,
    },
    /// The window started this terminal in `environment`.
    TerminalOpened {
        environment: EnvironmentId,
        terminal: TerminalSession,
    },
    /// A terminal asked to open as `opening` could not be started.
    TerminalFailed {
        opening: crate::content::Opening,
    },
    /// The window ended the terminal by this id in `environment`.
    TerminalClosed {
        environment: EnvironmentId,
        id: u128,
    },

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
    /// Show the session by this id of this environment wherever it runs,
    /// and select its row.
    ShowSession(EnvironmentId, String),
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
    /// Open a new session's draft in `worktree` of `environment` with
    /// `prompt` written in it, the keys in the prompt.
    DraftWritten {
        environment: EnvironmentId,
        worktree: PathBuf,
        prompt: String,
    },
    /// Edit the file at this path in a Neovim started in its folder in the
    /// environment, or in the one already on it; the keys go to that Neovim.
    EditFile(EnvironmentId, PathBuf),
    /// Open the file at this path of the environment as its row in the tree
    /// opens, in the worktree it lies in or else its own folder: a document
    /// shown links to it.
    OpenFile(EnvironmentId, PathBuf),
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
    /// The shell by this id of `environment` closed.
    ShellClosed {
        environment: EnvironmentId,
        id: u128,
    },
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
    /// The pointer pressed a window's strip, or the tab at `tab` for its
    /// focused window, at `x`, `y`, to drag it.
    WindowMoveStart {
        window: WindowId,
        tab: Option<usize>,
        x: f64,
        y: f64,
    },
    /// The pointer moved to `x`, `y`, over where a window dragged would go,
    /// if anywhere.
    WindowMoveOver {
        x: f64,
        y: f64,
        over: Option<DropAt>,
    },
    /// The pointer let a window go, or left the window.
    WindowMoveEnd,
    /// Open a terminal running the CLI's own login where this profile runs
    /// it.
    OpenLogin {
        environment: EnvironmentId,
        profile: adroit_model::profile::ProfileId,
    },
    /// A repository chosen in the picker was cloned into `dir`, to be taken
    /// for `then` as its row was chosen.
    Cloned {
        environment: EnvironmentId,
        dir: PathBuf,
        then: crate::picker::Then,
    },
    /// The profile chosen for this draft, or for the next one.
    DraftProfile {
        tab: Option<Tab>,
        profile: adroit_model::profile::ProfileId,
    },
    /// The options chosen for this draft, or for the next one.
    DraftOptions {
        tab: Option<Tab>,
        options: DraftOptions,
    },
    /// The sessions shown live here, or being attached to, each by its
    /// environment and its id.
    PanesLive(BTreeSet<(EnvironmentId, String)>),

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
    /// The tab at this place, from 0, was chosen.
    DetailsTab(usize),
    /// The tab this far along was asked for.
    DetailsTabStep(isize),
    /// The tab that shows this component was asked for.
    DetailsShowComponent(Component),
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
    /// Open the picker on the directories alone, to add one to the tree.
    PickerAddOpen,
    /// Open the list of past conversations on where the focused window
    /// works.
    ResumeOpen,
    /// Open the list of what the terminal host of the environment took
    /// with it, to choose what comes back.
    InterruptedOpen(EnvironmentId),
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
    /// Files dropped on the focused window's console or prompt.
    FilesDropped(Vec<PathBuf>),
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
    /// A send ended while its original text stayed in the prompt.
    PromptRetained,
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
        /// Where the environment of a heading it is on works.
        reach: Reach,
    },
    MenuClose,
    /// Run `action` on `target`, as its key would there.
    MenuRun {
        target: Target,
        action: Action,
    },
    /// Look at the image `Tab` in the lightbox over the window, holding the
    /// keys until it closes.
    LightboxOpen(Tab),
    /// Run the lightbox's action.
    LightboxAction(Action),
    /// Move the lightbox's image by shares of its width and height.
    LightboxPan {
        dx: f64,
        dy: f64,
    },
    /// Open the sheet adding an environment, holding the keys until it
    /// closes.
    AddingOpen,
    /// Close the sheet adding an environment, whether one was added or not.
    AddingClose,
    /// Ask this, holding the keys until it is answered.
    Confirm(Box<Ask>),
    /// Ask to end the selected row's session, from the tree, or else the
    /// one shown.
    ConfirmStop {
        selection: bool,
    },
    /// The question answered yes or no.
    ConfirmAnswer(bool),
    /// Start typing a new name for the session `id` of `environment` where
    /// `place` shows it.
    RenameStart {
        environment: EnvironmentId,
        id: String,
        place: RenamePlace,
    },
    /// The field for the session `id` of `environment` where `place` shows it
    /// now holds `text`.
    RenameText {
        environment: EnvironmentId,
        id: String,
        place: RenamePlace,
        text: String,
    },
    /// The field left, the name typed kept or given up.
    RenameEnd {
        environment: EnvironmentId,
        id: String,
        place: RenamePlace,
        commit: bool,
    },
    /// Start typing the name of a new entry of `kind` in the tree's files,
    /// from the row `row`.
    NamingStart {
        row: crate::tree::nav::Item,
        kind: crate::overlay::naming::Kind,
    },
    /// The field naming a new entry now holds `text`.
    NamingText(String),
    /// The field naming a new entry left, the name typed kept or given up.
    NamingEnd {
        commit: bool,
    },
    /// The work host made `path` in `worktree` of `environment`, a directory
    /// when `dir`.
    EntryMade {
        environment: EnvironmentId,
        worktree: PathBuf,
        path: PathBuf,
        dir: bool,
    },
    /// The system clipboard read for pasting into `dir` of `worktree` of
    /// `environment` holds `paths`, none when it holds no paths.
    Clipped {
        environment: EnvironmentId,
        worktree: PathBuf,
        dir: PathBuf,
        paths: Vec<PathBuf>,
    },
    /// The work host of `environment` pasted into `worktree` the entries
    /// `pasted`, moving them when `moving`, and left the entries `taken`
    /// alone for their targets were there already.
    EntriesPasted {
        environment: EnvironmentId,
        worktree: PathBuf,
        pasted: Vec<adroit_protocol::PasteEntry>,
        taken: Vec<adroit_protocol::PasteEntry>,
        moving: bool,
    },
    /// How the sessions' daemon of the environment stands, as the banner
    /// would say it.
    DaemonStanding {
        environment: EnvironmentId,
        note: Option<String>,
    },
    /// The daemon of the environment was restarted, and now stands so.
    DaemonRestarted {
        environment: EnvironmentId,
        note: Option<String>,
    },
    /// The banner was put away.
    BannerClose,
    /// The work host of the environment failed with `text`, saying it is
    /// older than the state it serves.
    WorkHostOutdated {
        environment: EnvironmentId,
        text: String,
    },
    /// Put away the banner of the work host older than its state.
    OutdatedLater,
    /// Restart the work host the banner says is older than its state.
    OutdatedRestart,
    /// Bring back everything the banner offers.
    BringBackAll,
    /// Let go of everything the banner offers.
    LetGoAll,
    /// Put the banner offering what the terminal host took away until more
    /// is taken.
    InterruptedLater,
    /// Read the words the banner says a session proposes to pass on.
    RelayReview,
    /// Put the banner saying words are proposed away until more are.
    RelayLater,
    /// Pass the words read on to the session with this code too, or no
    /// longer.
    RelayToggle(adroit_model::SessionCode),
    /// What came of bringing back what the terminal host of the environment
    /// took with it.
    BroughtBack {
        environment: EnvironmentId,
        outcomes: Vec<adroit_model::BroughtBack>,
    },
}
