//! Every action a key, a menu entry or a click can run, each described once:
//! the name the file of bindings calls it by, what the menu bar and the
//! right-click menus call it, and the icon it is shown with.

use std::fmt;
use std::mem::discriminant;
use std::str::FromStr;

use crate::icons::Icon;
use crate::keymap::{Chord, Error};

/// What a key does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Nothing, in place of a binding taken away.
    None,
    WindowLeft,
    WindowDown,
    WindowUp,
    WindowRight,
    WindowNext,
    WindowPrev,
    WindowTop,
    WindowBottom,
    WindowBack,
    WindowSplit,
    WindowVsplit,
    WindowNew,
    WindowQuit,
    WindowOnly,
    WindowMoveLeft,
    WindowMoveDown,
    WindowMoveUp,
    WindowMoveRight,
    WindowExchange,
    WindowRotate,
    WindowEqualize,
    WindowNarrower,
    WindowWider,
    WindowTaller,
    WindowShorter,
    WindowMaximizeHeight,
    WindowMaximizeWidth,
    WindowToTab,
    TabNext,
    TabPrev,
    TabLast,
    CursorDown,
    CursorUp,
    CursorFirst,
    CursorLast,
    CursorGo,
    CursorOpen,
    CursorSplit,
    CursorVsplit,
    CursorTab,
    CursorShow,
    CursorFold,
    CursorClose,
    CursorCopy,
    CursorReveal,
    WaitingNext,
    WaitingPrev,
    SessionNext,
    SessionPrev,
    WorkspaceNext,
    WorkspacePrev,
    SessionPick,
    SessionNew,
    SessionScratch,
    SessionResume,
    SessionStop,
    SessionRename,
    SessionAgentNext,
    SessionAgentPrev,
    ShellOpen,
    TerminalStop,
    NeovimOpen,
    TabClose,
    WindowClose,
    WindowOpen,
    Refresh,
    Filter,
    DetailsOverview,
    DetailsChanges,
    DetailsLog,
    WorktreesToggle,
    ScrollDown,
    ScrollUp,
    ScrollHalfDown,
    ScrollHalfUp,
    ScrollTop,
    ScrollBottom,
    ConsoleSendNext,
    ConsolePassEveryKey,
    PromptFocus,
    PromptWrite,
    PromptSend,
    PromptImages,
    PromptTakeSuggestion,
    HistoryPrev,
    HistoryNext,
    SettingsOpen,
    DaemonRestart,
    KeysHelp,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    TreeToggle,
    DetailsToggle,
    ViewerZoomIn,
    ViewerZoomOut,
    ViewerZoomFit,
    ViewerPageNext,
    ViewerPagePrev,
    ViewerPlay,
    ViewerBack,
    ViewerForward,
    ViewerCopyPath,
    /// Send a key to the session: the one named, or the one pressed.
    ConsoleSend(Option<Chord>),
    /// Show the content's tab of this number, counted from 1.
    TabGo(u8),
}

/// Which of a right-click menu's groups an entry is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Going to what was clicked, or doing something with it.
    Go,
    /// Starting something beside it.
    Start,
    /// Ending or closing it.
    End,
}

/// What a right-click menu offering an action calls it, and where it draws
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub label: &'static str,
    pub group: Group,
}

/// One action, as everything that shows or runs it knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionSpec {
    /// The action; one with an argument stands for every argument.
    pub action: Action,
    /// What the file of bindings calls it.
    pub name: &'static str,
    /// What it does, in full, as the key reference says it: Markdown
    /// starting in lower case, its links naming the files of `docs/spec/`.
    pub does: &'static str,
    /// What the menu bar calls it; `None` for an action that is only a key.
    pub bar: Option<&'static str>,
    /// What a right-click menu calls it, unless it says what it does there
    /// in words of its own.
    pub row: Option<Row>,
    /// What the keys help calls it, where neither menu does, or where what
    /// they call it says too little among the keys.
    pub says: Option<&'static str>,
    pub icon: Option<Icon>,
}

impl ActionSpec {
    const fn new(action: Action, name: &'static str, does: &'static str) -> Self {
        ActionSpec {
            action,
            name,
            does,
            bar: None,
            row: None,
            says: None,
            icon: None,
        }
    }

    const fn bar(mut self, label: &'static str, icon: Icon) -> Self {
        self.bar = Some(label);
        self.icon = Some(icon);
        self
    }

    const fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    const fn says(mut self, label: &'static str) -> Self {
        self.says = Some(label);
        self
    }

    const fn row(mut self, label: &'static str, group: Group) -> Self {
        self.row = Some(Row { label, group });
        self
    }
}

use Action as A;
use ActionSpec as S;
use Group::{End, Go, Start};

/// Every action, in the order the Settings window offers them.
pub static ACTIONS: &[ActionSpec] = &[
    S::new(A::None, "none", "nothing; bound to keys, it takes them away"),
    S::new(A::WindowLeft, "window.left", "move to the window to the left: the tree lies left of the content's windows").bar("Left Window", Icon::SquareArrowLeft),
    S::new(A::WindowDown, "window.down", "move to the window below: a session's prompt lies below its console").bar("Window Below", Icon::SquareArrowDown),
    S::new(A::WindowUp, "window.up", "move to the window above: a session's console lies above its prompt").bar("Window Above", Icon::SquareArrowUp),
    S::new(A::WindowRight, "window.right", "move to the window to the right: the details panel lies right of the content's windows").bar("Right Window", Icon::SquareArrowRight),
    S::new(A::WindowNext, "window.next", "move to the window after the one the keys are in, from the tree through the content's windows, each console before its prompt, to the details panel, going round at the end").bar("Next Window", Icon::ArrowBarRight),
    S::new(A::WindowPrev, "window.prev", "move to the window before the one the keys are in, in the order `window.next` goes, going round at the start").bar("Previous Window", Icon::ArrowBarLeft),
    S::new(A::WindowTop, "window.top", "move to the first window, the tree when shown").bar("First Window", Icon::ArrowUpLeft),
    S::new(A::WindowBottom, "window.bottom", "move to the last window, the details panel when shown").bar("Last Window", Icon::ArrowDownRight),
    S::new(A::WindowBack, "window.back", "move back to the window the keys were in before").bar("Back to Window Before", Icon::ArrowBackUp),
    S::new(A::WindowSplit, "window.split", "split the focused window one above the other, the new window, below it, showing the same ([content](content.md))").bar("Split One Above the Other", Icon::LayoutRows),
    S::new(A::WindowVsplit, "window.vsplit", "split the focused window side by side, the new window, right of it, showing the same ([content](content.md))").bar("Split Side by Side", Icon::LayoutColumns),
    S::new(A::WindowNew, "window.new", "split the focused window one above the other and start writing a new session in the new one, in the worktree the focused window shows").bar("Split for a New Session", Icon::SquarePlus),
    S::new(A::WindowQuit, "window.quit", "close the focused window, and its tab when it was the last; what it showed runs on").bar("Close Split", Icon::SquareMinus),
    S::new(A::WindowOnly, "window.only", "close every other window of the tab").bar("Close Other Splits", Icon::Square),
    S::new(A::WindowMoveLeft, "window.move_left", "move the focused window to the left of its tab, as tall as the tab")
        .bar("Move Window to the Left", Icon::ArrowBarToLeft),
    S::new(A::WindowMoveDown, "window.move_down", "move the focused window to the bottom of its tab, as wide as the tab")
        .bar("Move Window to the Bottom", Icon::ArrowBarToDown),
    S::new(A::WindowMoveUp, "window.move_up", "move the focused window to the top of its tab, as wide as the tab").bar("Move Window to the Top", Icon::ArrowBarToUp),
    S::new(A::WindowMoveRight, "window.move_right", "move the focused window to the right of its tab, as tall as the tab")
        .bar("Move Window to the Right", Icon::ArrowBarToRight),
    S::new(A::WindowExchange, "window.exchange", "exchange the focused window with the next one in its split, or the one before the last").bar("Exchange Windows", Icon::SwitchHorizontal),
    S::new(A::WindowRotate, "window.rotate", "rotate the windows of the focused window's split forward, the last becoming the first").bar("Rotate Windows", Icon::RotateClockwise),
    S::new(A::WindowEqualize, "window.equalize", "give the windows of every split the same size").bar("Equal Sizes", Icon::Equal),
    S::new(A::WindowNarrower, "window.narrower", "make the focused window a step narrower").bar("Narrower", Icon::ColumnRemove),
    S::new(A::WindowWider, "window.wider", "make the focused window a step wider").bar("Wider", Icon::ColumnInsertRight),
    S::new(A::WindowTaller, "window.taller", "make the focused window a step taller").bar("Taller", Icon::RowInsertBottom),
    S::new(A::WindowShorter, "window.shorter", "make the focused window a step shorter").bar("Shorter", Icon::RowRemove),
    S::new(A::WindowMaximizeHeight, "window.maximize_height", "make the focused window as tall as the others allow")
        .bar("Tallest", Icon::ArrowAutofitHeight),
    S::new(A::WindowMaximizeWidth, "window.maximize_width", "make the focused window as wide as the others allow").bar("Widest", Icon::ArrowAutofitWidth),
    S::new(A::WindowToTab, "window.to_tab", "move the focused window to a new tab, unless it is the only one of its tab").bar("Move Window to a New Tab", Icon::BrowserPlus),
    S::new(A::TabNext, "tab.next", "show the next tab of the area the keys are in: the details panel's sheets there, elsewhere the content's tabs").bar("Next Tab", Icon::ChevronRight),
    S::new(A::TabPrev, "tab.prev", "show the previous tab of the area the keys are in: the details panel's sheets there, elsewhere the content's tabs").bar("Previous Tab", Icon::ChevronLeft),
    S::new(A::TabLast, "tab.last", "show the content's tab shown before the one shown now").bar("Tab Before", Icon::ArrowBack),
    S::new(A::CursorDown, "cursor.down", "move the selection in the tree to the next row; in the details panel, in the list it shows, or scroll what else it shows").says("Select the next row"),
    S::new(A::CursorUp, "cursor.up", "move the selection in the tree to the previous row; in the details panel, in the list it shows, or scroll what else it shows").says("Select the previous row"),
    S::new(A::CursorFirst, "cursor.first", "move the selection in the tree to the first row; in the details panel, in the list it shows, or scroll what else it shows").says("Select the first row"),
    S::new(A::CursorLast, "cursor.last", "move the selection in the tree to the last row; in the details panel, in the list it shows, or scroll what else it shows").says("Select the last row"),
    S::new(A::CursorGo, "cursor.go", "go to a window showing the selected session, shell, worktree or file, in any tab, or else open it as `cursor.open` does; in the details panel, open the selected row")
        .says("Go to it where it is shown")
        .icon(Icon::ArrowRight)
        .row("Open", Go),
    S::new(A::CursorOpen, "cursor.open", "open the selected session, shell, worktree or file in the focused window, in place of what it shows, and move to it, or to the prompt of a session not started yet; on a folded row, a worktree or a session holding others too, unfold it instead; fold or unfold a workspace, a heading or a directory; in the details panel, open the selected row")
        .says("Open it here")
        .icon(Icon::ArrowRight)
        .row("Open", Go),
    S::new(A::CursorSplit, "cursor.split", "open the selected session, shell, worktree or file as `cursor.open` does, in a new window split off the focused one, below it")
        .icon(Icon::LayoutRows)
        .row("Open in a split below", Go),
    S::new(A::CursorVsplit, "cursor.vsplit", "open the selected session, shell, worktree or file as `cursor.open` does, in a new window split off the focused one, beside it")
        .icon(Icon::LayoutColumns)
        .row("Open in a split beside", Go),
    S::new(A::CursorTab, "cursor.tab", "open the selected session, shell, worktree or file as `cursor.open` does, in a new tab after the one shown")
        .icon(Icon::BrowserPlus)
        .row("Open in a new tab", Go),
    S::new(A::CursorShow, "cursor.show", "open the selected session, shell, worktree or file in the focused window as `cursor.open` does, with the keys staying in the tree; fold or unfold a workspace, a heading or a directory; in the details panel, open the selected row")
        .icon(Icon::Eye)
        .row("Show", Go),
    S::new(A::CursorFold, "cursor.fold", "fold the selected node, or on a row that is not unfolded go up to the row it sits under; in the details panel, go back out to the list what it shows was opened from").says("Fold, or go back"),
    S::new(A::CursorClose, "cursor.close", "close every window showing the selected session, shell or Neovim, or the selected file in its viewer or its Neovim; what they showed runs on")
        .icon(Icon::SquareMinus)
        .row("Close its windows", End),
    S::new(A::CursorCopy, "cursor.copy", "copy the path of the selected file, worktree or workspace, or the name of the selected session, shell or Neovim; in the details panel, the selected link of the overview: its address, or its file's path")
        .icon(Icon::Copy)
        .row("Copy", Go),
    S::new(A::CursorReveal, "cursor.reveal", "in the details panel, show the selected file of the overview in the Finder")
        .icon(Icon::FolderSearch)
        .row("Reveal in Finder", Go),
    S::new(A::WaitingNext, "waiting.next", "open the next session waiting on you").bar("Next Waiting", Icon::BellDown),
    S::new(A::WaitingPrev, "waiting.prev", "open the previous session waiting on you").bar("Previous Waiting", Icon::BellUp),
    S::new(A::SessionNext, "session.next", "open the session after the one shown, in the tree's order over every session, folded away or filtered out too, going round at the end; the keys stay in the area they are in").bar("Next Session", Icon::MessageDown),
    S::new(A::SessionPrev, "session.prev", "open the session before the one shown, in the tree's order over every session, folded away or filtered out too, going round at the start; the keys stay in the area they are in").bar("Previous Session", Icon::MessageUp),
    S::new(A::WorkspaceNext, "workspace.next", "open a session of the workspace after the one shown, in the tree's order over every workspace holding a session, folded away or filtered out too, going round at the end: the session last shown there since the window opened, or else its first; the keys stay in the area they are in").bar("Next Workspace", Icon::FolderDown),
    S::new(A::WorkspacePrev, "workspace.prev", "open a session of the workspace before the one shown, as `workspace.next` does, going round at the start").bar("Previous Workspace", Icon::FolderUp),
    S::new(A::SessionPick, "session.pick", "open the picker, to go to a session or choose where a new one works ([details](workspaces.md#going-to-a-session-or-starting-one))").bar("Go to or Start Session…", Icon::Search),
    S::new(A::SessionNew, "session.new", "start a session, with the prompt ready to write its first message: in the selected row's worktree or directory from the tree, in the one the focused window shows from elsewhere")
        .bar("New Session", Icon::MessagePlus)
        .row("New session here", Start),
    S::new(A::SessionScratch, "session.scratch", "start a scratch session, with the prompt ready: one that runs in a new directory of its own, for talking about something no project holds ([details](workspaces.md#scratch-sessions))").bar("New Scratch Session", Icon::Notes),
    S::new(A::SessionResume, "session.resume", "list the conversations Claude Code and Codex kept, to resume one in a new session ([details](sessions.md#resuming-a-past-conversation))").bar("Resume Conversation…", Icon::TransferIn),
    S::new(A::SessionStop, "session.stop", "end a session adroit runs after asking; Claude Code keeps its conversation, for `/resume` in a session started later")
        .bar("End…", Icon::PlayerStop)
        .row("End…", End),
    S::new(A::SessionRename, "session.rename", "edit the name of a session adroit runs in place; the tree shows it, and the session is sent Claude Code's `/rename` so its own interface says it too")
        .bar("Rename…", Icon::Pencil)
        .row("Rename…", Go),
    S::new(A::SessionAgentNext, "session.agent_next", "while a session not started yet is shown, switch the agent it is to run to the next one ([details](sessions.md#agents))")
        .bar("Next Agent of New Session", Icon::ArrowsExchange),
    S::new(A::SessionAgentPrev, "session.agent_prev", "while a session not started yet is shown, switch the agent it is to run to the previous one ([details](sessions.md#agents))")
        .bar("Previous Agent of New Session", Icon::ArrowsExchange2),
    S::new(A::ShellOpen, "shell.open", "open a shell in the worktree")
        .bar("Open Shell", Icon::Terminal2)
        .row("Open a shell here", Start),
    S::new(A::TerminalStop, "terminal.stop", "end the terminal selected in the tree, or else the one shown")
        .bar("Stop Terminal", Icon::SquareX)
        .row("Stop terminal", End),
    S::new(A::NeovimOpen, "neovim.open", "open Neovim, with your own configuration, in the worktree as `shell.open` opens a shell ([shells](terminals.md))")
        .bar("Open Neovim", Icon::Code)
        .row("Open Neovim here", Start),
    S::new(A::TabClose, "tab.close", "close the content's tab shown and every window in it; agents and terminals keep running and remain reachable from the tree")
        .bar("Close Tab", Icon::X)
        .row("Close tab", End),
    S::new(A::WindowClose, "window.close", "close the window; the instance's other windows stay open, and its sessions run on").bar("Close Window", Icon::AppWindow),
    S::new(A::WindowOpen, "window.open", "open another window of the instance ([instances](sessions.md#instances))").bar("New Window", Icon::CopyPlus),
    S::new(A::Refresh, "refresh", "read the sessions, and what git says of where they run, again now").bar("Refresh", Icon::Refresh),
    S::new(A::Filter, "filter", "narrow the tree to the sessions, shells, workspaces and worktrees whose names hold what is typed next; `Enter` keeps it, `Escape` clears it").bar("Filter…", Icon::Filter),
    S::new(A::DetailsOverview, "details.overview", "show the overview sheet of the details panel ([details](details.md)), and the panel when it is hidden").bar("Overview", Icon::InfoCircle),
    S::new(A::DetailsChanges, "details.changes", "show the changes sheet of the details panel ([details](details.md)), and the panel when it is hidden").bar("Changes", Icon::FileDiff),
    S::new(A::DetailsLog, "details.log", "show the log sheet of the details panel ([details](details.md)), and the panel when it is hidden").bar("Log", Icon::GitCommit),
    S::new(A::WorktreesToggle, "worktrees.toggle", "list every worktree git knows, those without sessions too, or only those with sessions again").bar("Toggle All Worktrees", Icon::ListTree),
    S::new(A::ScrollDown, "scroll.down", "scroll the details panel down a step; bound in the console, scroll it: Claude Code and other programs that read the mouse are sent the wheel, others are scrolled back through what scrolled off; bound in a viewer, scroll what it shows").says("Scroll down"),
    S::new(A::ScrollUp, "scroll.up", "scroll up a step, what `scroll.down` scrolls").says("Scroll up"),
    S::new(A::ScrollHalfDown, "scroll.half_down", "scroll down half a page, what `scroll.down` scrolls").says("Scroll down half a page"),
    S::new(A::ScrollHalfUp, "scroll.half_up", "scroll up half a page, what `scroll.down` scrolls").says("Scroll up half a page"),
    S::new(A::ScrollTop, "scroll.top", "scroll to the top, what `scroll.down` scrolls").says("Scroll to the top"),
    S::new(A::ScrollBottom, "scroll.bottom", "scroll to the bottom, what `scroll.down` scrolls").says("Scroll to the bottom"),
    S::new(A::ConsoleSendNext, "console.send_next", "send the next key as it is, whatever it is bound to").says("Send the next key as it is"),
    S::new(A::ConsolePassEveryKey, "console.pass_every_key", "send every key as it is until `Escape`; `Ctrl+c` sends `Escape` meanwhile").says("Pass every key on"),
    S::new(A::PromptFocus, "prompt.focus", "give the prompt the keys").bar("Prompt", Icon::CursorText),
    S::new(A::PromptWrite, "prompt.write", "give the prompt the keys, and the key pressed with them").says("Write in the prompt"),
    S::new(A::PromptSend, "prompt.send", "send what the prompt holds").bar("Send Prompt", Icon::Send),
    S::new(A::PromptImages, "prompt.images", "select an attached image").bar("Select Attached Image", Icon::Photo),
    S::new(A::PromptTakeSuggestion, "prompt.take_suggestion", "on an empty prompt, send the key pressed to the session and then bring the session's input into the prompt, so a prompt Claude Code suggests is taken to edit; an agent whose input cannot be taken back gets the key alone. On a prompt holding text or images, or while Neovim shows its completions or its command line, the key is Neovim's").says("Take the suggested prompt"),
    S::new(A::HistoryPrev, "prompt.history_prev", "put the prompt sent before the one shown in the prompt, to change and send again: those Claude Code recalls with its Up arrow where the focused window's session works. In the prompt, while Neovim shows its completions or its command line, the key is Neovim's").says("Previous prompt"),
    S::new(A::HistoryNext, "prompt.history_next", "put the prompt sent after the one shown in the prompt, as `prompt.history_prev` does").says("Next prompt"),
    S::new(A::SettingsOpen, "settings.open", "open the Settings window").bar("Settings…", Icon::Settings),
    S::new(A::DaemonRestart, "daemon.restart", "restart the sessions' daemon after asking, ending every session it holds ([details](sessions.md#the-hosts))").bar("Restart Session Daemon…", Icon::ServerBolt),
    S::new(A::KeysHelp, "keys.help", "show every binding in force, area by area, the areas the keys are in first, each with what its action does; `Escape`, `q` or the key that showed it closes it, and no other key acts behind it").bar("Keyboard Shortcuts", Icon::Keyboard),
    S::new(A::ZoomIn, "zoom.in", "draw the window a step larger").bar("Zoom In", Icon::ZoomIn),
    S::new(A::ZoomOut, "zoom.out", "draw the window a step smaller").bar("Zoom Out", Icon::ZoomOut),
    S::new(A::ZoomReset, "zoom.reset", "draw the window at the zoom the settings give").bar("Actual Size", Icon::ZoomReset),
    S::new(A::TreeToggle, "tree.toggle", "show or hide the tree").bar("Toggle Tree", Icon::LayoutSidebar),
    S::new(A::DetailsToggle, "details.toggle", "show or hide the details panel").bar("Toggle Details", Icon::LayoutSidebarRight),
    S::new(A::ViewerZoomIn, "viewer.zoom_in", "in a viewer, draw an image, a PDF or an HTML page larger ([viewers](viewers.md))").says("Zoom in"),
    S::new(A::ViewerZoomOut, "viewer.zoom_out", "in a viewer, draw an image, a PDF or an HTML page smaller").says("Zoom out"),
    S::new(A::ViewerZoomFit, "viewer.zoom_fit", "in a viewer, draw an image, a PDF or an HTML page fitting the window").says("Fit to the window"),
    S::new(A::ViewerPageNext, "viewer.page_next", "in a viewer, show a PDF's next page").says("Next page"),
    S::new(A::ViewerPagePrev, "viewer.page_prev", "in a viewer, show a PDF's previous page").says("Previous page"),
    S::new(A::ViewerPlay, "viewer.play", "in a viewer, play or pause a movie").says("Play or pause"),
    S::new(A::ViewerBack, "viewer.back", "in a viewer, seek a movie back").says("Back"),
    S::new(A::ViewerForward, "viewer.forward", "in a viewer, seek a movie forward").says("Forward"),
    S::new(A::ViewerCopyPath, "viewer.copy_path", "in a viewer, copy the path of the file it shows").says("Copy the path"),
    S::new(A::ConsoleSend(None), "console.send", "send the key pressed to the session; with a key named after it, as in `console.send Escape`, send that key instead").says("Send the key to the console"),
    S::new(A::TabGo(1), "tab.go", "show the content's tab numbered `n`, from 1 to 9, or the last when there are fewer").says("Go to tab"),
];

impl Action {
    /// What the action does, in a few words: what the keys help lists it
    /// as. Empty for [`Action::None`].
    pub fn label(&self) -> String {
        let spec = self.spec();
        let label = spec
            .says
            .or(spec.bar)
            .or(spec.row.map(|r| r.label))
            .unwrap_or_default();
        match self {
            Action::ConsoleSend(Some(c)) => format!("Send {c} to the console"),
            Action::TabGo(n) => format!("{label} {n}"),
            _ => label.to_string(),
        }
    }

    /// The row of [`ACTIONS`] that describes this action.
    pub fn spec(&self) -> &'static ActionSpec {
        ACTIONS
            .iter()
            .find(|s| discriminant(&s.action) == discriminant(self))
            .expect("every action has a row in ACTIONS")
    }

    /// The icon the action is shown with; `None` for an action that is only
    /// a key.
    pub fn icon(&self) -> Option<Icon> {
        self.spec().icon
    }

    /// Every action's name that a binding can name alone, as the Settings
    /// window offers them: not `none`, nor `tab.go`, which needs a number.
    pub fn names() -> impl Iterator<Item = &'static str> {
        ACTIONS
            .iter()
            .filter(|s| !matches!(s.action, Action::None | Action::TabGo(_)))
            .map(|s| s.name)
    }
}

impl FromStr for Action {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        let (name, arg) = match text.split_once(' ') {
            Some((n, a)) => (n, Some(a.trim())),
            None => (text, None),
        };
        let unknown = || Error(format!("{text:?} is not an action"));
        let spec = ACTIONS
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(unknown)?;
        Ok(match (&spec.action, arg) {
            (Action::ConsoleSend(_), Some(k)) => Action::ConsoleSend(Some(Chord::parse(k)?)),
            (Action::TabGo(_), Some(n)) => match n.parse::<u8>() {
                Ok(n @ 1..=9) => Action::TabGo(n),
                _ => return Err(Error(format!("{text:?} names no tab from 1 to 9"))),
            },
            (Action::TabGo(_), None) => return Err(unknown()),
            (Action::ConsoleSend(_), None) => Action::ConsoleSend(None),
            (action, None) => action.clone(),
            (_, Some(_)) => return Err(unknown()),
        })
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.spec().name;
        match self {
            Action::ConsoleSend(Some(c)) => write!(f, "{name} {c}"),
            Action::TabGo(n) => write!(f, "{name} {n}"),
            _ => f.write_str(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Every variant of [`Action`], one value of each. The match makes a
    /// new variant fail to build here until it is listed.
    macro_rules! every_action {
        ($($variant:ident),* $(,)?) => {{
            fn _listed(a: &Action) {
                match a {
                    Action::ConsoleSend(_) | Action::TabGo(_) => {}
                    $(Action::$variant => {})*
                }
            }
            vec![$(Action::$variant,)* Action::ConsoleSend(None), Action::TabGo(1)]
        }};
    }

    fn every_action() -> Vec<Action> {
        every_action![
            None,
            WindowLeft,
            WindowDown,
            WindowUp,
            WindowRight,
            WindowNext,
            WindowPrev,
            WindowTop,
            WindowBottom,
            WindowBack,
            WindowSplit,
            WindowVsplit,
            WindowNew,
            WindowQuit,
            WindowOnly,
            WindowMoveLeft,
            WindowMoveDown,
            WindowMoveUp,
            WindowMoveRight,
            WindowExchange,
            WindowRotate,
            WindowEqualize,
            WindowNarrower,
            WindowWider,
            WindowTaller,
            WindowShorter,
            WindowMaximizeHeight,
            WindowMaximizeWidth,
            WindowToTab,
            TabNext,
            TabPrev,
            TabLast,
            CursorDown,
            CursorUp,
            CursorFirst,
            CursorLast,
            CursorGo,
            CursorOpen,
            CursorSplit,
            CursorVsplit,
            CursorTab,
            CursorShow,
            CursorFold,
            CursorClose,
            CursorCopy,
            CursorReveal,
            WaitingNext,
            WaitingPrev,
            SessionNext,
            SessionPrev,
            WorkspaceNext,
            WorkspacePrev,
            SessionPick,
            SessionNew,
            SessionScratch,
            SessionResume,
            SessionStop,
            SessionRename,
            SessionAgentNext,
            SessionAgentPrev,
            ShellOpen,
            TerminalStop,
            NeovimOpen,
            TabClose,
            WindowClose,
            WindowOpen,
            Refresh,
            Filter,
            DetailsOverview,
            DetailsChanges,
            DetailsLog,
            WorktreesToggle,
            ScrollDown,
            ScrollUp,
            ScrollHalfDown,
            ScrollHalfUp,
            ScrollTop,
            ScrollBottom,
            ConsoleSendNext,
            ConsolePassEveryKey,
            PromptFocus,
            PromptWrite,
            PromptSend,
            PromptImages,
            PromptTakeSuggestion,
            HistoryPrev,
            HistoryNext,
            SettingsOpen,
            DaemonRestart,
            KeysHelp,
            ZoomIn,
            ZoomOut,
            ZoomReset,
            TreeToggle,
            DetailsToggle,
            ViewerZoomIn,
            ViewerZoomOut,
            ViewerZoomFit,
            ViewerPageNext,
            ViewerPagePrev,
            ViewerPlay,
            ViewerBack,
            ViewerForward,
            ViewerCopyPath,
        ]
    }

    #[test]
    fn every_action_has_one_row_in_the_table() {
        for action in every_action() {
            let rows = ACTIONS
                .iter()
                .filter(|s| discriminant(&s.action) == discriminant(&action))
                .count();
            assert_eq!(rows, 1, "{action:?}");
        }
        assert_eq!(ACTIONS.len(), every_action().len());
    }

    #[test]
    fn no_two_actions_share_a_name() {
        let mut seen = HashSet::new();
        for spec in ACTIONS {
            assert!(seen.insert(spec.name), "{}", spec.name);
        }
    }

    #[test]
    fn every_action_reads_from_the_name_its_row_gives_and_back() {
        for spec in ACTIONS {
            let action = match spec.action {
                Action::TabGo(_) => Action::TabGo(3),
                ref a => a.clone(),
            };
            assert_eq!(action.to_string().parse::<Action>(), Ok(action.clone()));
            assert!(action.to_string().starts_with(spec.name), "{}", spec.name);
        }
    }

    #[test]
    fn an_action_shown_in_a_menu_has_an_icon_and_one_only_a_key_has_none() {
        for spec in ACTIONS {
            let shown = spec.bar.is_some() || spec.row.is_some();
            assert_eq!(spec.icon.is_some(), shown, "{}", spec.name);
        }
    }

    #[test]
    fn every_action_but_none_says_what_it_does() {
        for action in every_action() {
            let said = action.label();
            assert_eq!(said.is_empty(), action == Action::None, "{action:?}");
        }
    }

    #[test]
    fn every_action_says_in_full_what_it_does_starting_in_lower_case() {
        for spec in ACTIONS {
            let first = spec.does.chars().next();
            assert!(first.is_some_and(|c| !c.is_uppercase()), "{}", spec.name);
            assert!(!spec.does.ends_with('.'), "{}", spec.name);
        }
    }

    #[test]
    fn an_action_on_the_menu_bar_says_what_the_menu_bar_calls_it() {
        assert_eq!(Action::SettingsOpen.label(), "Settings…");
        assert_eq!(Action::KeysHelp.label(), "Keyboard Shortcuts");
    }

    #[test]
    fn an_action_with_an_argument_says_the_argument() {
        let send: Action = "console.send Escape".parse().unwrap();
        assert!(send.label().contains("Escape"), "{}", send.label());
        assert!(Action::TabGo(3).label().contains('3'));
    }

    #[test]
    fn every_action_is_offered_by_its_name() {
        let names: Vec<&str> = Action::names().collect();
        assert!(names.contains(&"cursor.down"));
        assert!(names.contains(&"console.send"));
        assert!(!names.contains(&"none"));
        assert!(!names.contains(&"tab.go"));
        for n in names {
            assert!(n.parse::<Action>().is_ok(), "{n}");
        }
    }

    #[test]
    fn an_action_reads_from_its_name_and_back() {
        let a: Action = "cursor.down".parse().unwrap();
        assert_eq!(a, Action::CursorDown);
        assert_eq!(a.to_string(), "cursor.down");
        assert!("cursor.sideways".parse::<Action>().is_err());
    }

    #[test]
    fn an_action_takes_an_argument_only_where_it_needs_one() {
        assert!("cursor.down 3".parse::<Action>().is_err());
        assert!("none x".parse::<Action>().is_err());
        assert!("tab.go".parse::<Action>().is_err());
        assert!("tab.go 0".parse::<Action>().is_err());
        assert_eq!("tab.go 9".parse::<Action>(), Ok(Action::TabGo(9)));
        assert_eq!("none".parse::<Action>(), Ok(Action::None));
    }

    #[test]
    fn sending_a_key_names_the_key_to_send() {
        let a: Action = "console.send Escape".parse().unwrap();
        assert_eq!(
            a,
            Action::ConsoleSend(Some(Chord::parse("Escape").unwrap()))
        );
        assert_eq!(a.to_string(), "console.send Escape");
        assert_eq!(
            "console.send".parse::<Action>().unwrap(),
            Action::ConsoleSend(None)
        );
    }
}
