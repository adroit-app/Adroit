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
    SessionPick,
    SessionNew,
    SessionScratch,
    SessionTakeOver,
    SessionStop,
    SessionRename,
    SessionAgentNext,
    SessionAgentPrev,
    ShellOpen,
    TerminalStop,
    NeovimOpen,
    TabClose,
    WindowClose,
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
    HistoryPrev,
    HistoryNext,
    SettingsOpen,
    DaemonRestart,
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
    /// What the menu bar calls it; `None` for an action that is only a key.
    pub bar: Option<&'static str>,
    /// What a right-click menu calls it, unless it says what it does there
    /// in words of its own.
    pub row: Option<Row>,
    pub icon: Option<Icon>,
}

impl ActionSpec {
    const fn new(action: Action, name: &'static str) -> Self {
        ActionSpec {
            action,
            name,
            bar: None,
            row: None,
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
    S::new(A::None, "none"),
    S::new(A::WindowLeft, "window.left").bar("Left Window", Icon::SquareArrowLeft),
    S::new(A::WindowDown, "window.down").bar("Window Below", Icon::SquareArrowDown),
    S::new(A::WindowUp, "window.up").bar("Window Above", Icon::SquareArrowUp),
    S::new(A::WindowRight, "window.right").bar("Right Window", Icon::SquareArrowRight),
    S::new(A::WindowNext, "window.next").bar("Next Window", Icon::ArrowBarRight),
    S::new(A::WindowPrev, "window.prev").bar("Previous Window", Icon::ArrowBarLeft),
    S::new(A::WindowTop, "window.top").bar("First Window", Icon::ArrowUpLeft),
    S::new(A::WindowBottom, "window.bottom").bar("Last Window", Icon::ArrowDownRight),
    S::new(A::WindowBack, "window.back").bar("Back to Window Before", Icon::ArrowBackUp),
    S::new(A::WindowSplit, "window.split").bar("Split One Above the Other", Icon::LayoutRows),
    S::new(A::WindowVsplit, "window.vsplit").bar("Split Side by Side", Icon::LayoutColumns),
    S::new(A::WindowNew, "window.new").bar("Split for a New Session", Icon::SquarePlus),
    S::new(A::WindowQuit, "window.quit").bar("Close Split", Icon::SquareMinus),
    S::new(A::WindowOnly, "window.only").bar("Close Other Splits", Icon::Square),
    S::new(A::WindowMoveLeft, "window.move_left")
        .bar("Move Window to the Left", Icon::ArrowBarToLeft),
    S::new(A::WindowMoveDown, "window.move_down")
        .bar("Move Window to the Bottom", Icon::ArrowBarToDown),
    S::new(A::WindowMoveUp, "window.move_up").bar("Move Window to the Top", Icon::ArrowBarToUp),
    S::new(A::WindowMoveRight, "window.move_right")
        .bar("Move Window to the Right", Icon::ArrowBarToRight),
    S::new(A::WindowExchange, "window.exchange").bar("Exchange Windows", Icon::SwitchHorizontal),
    S::new(A::WindowRotate, "window.rotate").bar("Rotate Windows", Icon::RotateClockwise),
    S::new(A::WindowEqualize, "window.equalize").bar("Equal Sizes", Icon::Equal),
    S::new(A::WindowNarrower, "window.narrower").bar("Narrower", Icon::ColumnRemove),
    S::new(A::WindowWider, "window.wider").bar("Wider", Icon::ColumnInsertRight),
    S::new(A::WindowTaller, "window.taller").bar("Taller", Icon::RowInsertBottom),
    S::new(A::WindowShorter, "window.shorter").bar("Shorter", Icon::RowRemove),
    S::new(A::WindowMaximizeHeight, "window.maximize_height")
        .bar("Tallest", Icon::ArrowAutofitHeight),
    S::new(A::WindowMaximizeWidth, "window.maximize_width").bar("Widest", Icon::ArrowAutofitWidth),
    S::new(A::WindowToTab, "window.to_tab").bar("Move Window to a New Tab", Icon::BrowserPlus),
    S::new(A::TabNext, "tab.next").bar("Next Tab", Icon::ChevronRight),
    S::new(A::TabPrev, "tab.prev").bar("Previous Tab", Icon::ChevronLeft),
    S::new(A::TabLast, "tab.last").bar("Tab Before", Icon::ArrowBack),
    S::new(A::CursorDown, "cursor.down"),
    S::new(A::CursorUp, "cursor.up"),
    S::new(A::CursorFirst, "cursor.first"),
    S::new(A::CursorLast, "cursor.last"),
    S::new(A::CursorGo, "cursor.go")
        .icon(Icon::ArrowRight)
        .row("Open", Go),
    S::new(A::CursorOpen, "cursor.open")
        .icon(Icon::ArrowRight)
        .row("Open", Go),
    S::new(A::CursorSplit, "cursor.split")
        .icon(Icon::LayoutRows)
        .row("Open in a split below", Go),
    S::new(A::CursorVsplit, "cursor.vsplit")
        .icon(Icon::LayoutColumns)
        .row("Open in a split beside", Go),
    S::new(A::CursorTab, "cursor.tab")
        .icon(Icon::BrowserPlus)
        .row("Open in a new tab", Go),
    S::new(A::CursorShow, "cursor.show")
        .icon(Icon::Eye)
        .row("Show", Go),
    S::new(A::CursorFold, "cursor.fold"),
    S::new(A::CursorClose, "cursor.close")
        .icon(Icon::SquareMinus)
        .row("Close its windows", End),
    S::new(A::CursorCopy, "cursor.copy")
        .icon(Icon::Copy)
        .row("Copy", Go),
    S::new(A::CursorReveal, "cursor.reveal")
        .icon(Icon::FolderSearch)
        .row("Reveal in Finder", Go),
    S::new(A::WaitingNext, "waiting.next").bar("Next Waiting", Icon::BellDown),
    S::new(A::WaitingPrev, "waiting.prev").bar("Previous Waiting", Icon::BellUp),
    S::new(A::SessionNext, "session.next").bar("Next Session", Icon::MessageDown),
    S::new(A::SessionPrev, "session.prev").bar("Previous Session", Icon::MessageUp),
    S::new(A::SessionPick, "session.pick").bar("Go to or Start Session…", Icon::Search),
    S::new(A::SessionNew, "session.new")
        .bar("New Session", Icon::MessagePlus)
        .row("New session here", Start),
    S::new(A::SessionScratch, "session.scratch").bar("New Scratch Session", Icon::Notes),
    S::new(A::SessionTakeOver, "session.take_over")
        .bar("Take Over…", Icon::TransferIn)
        .row("Take over…", End),
    S::new(A::SessionStop, "session.stop")
        .bar("End…", Icon::PlayerStop)
        .row("End…", End),
    S::new(A::SessionRename, "session.rename")
        .bar("Rename…", Icon::Pencil)
        .row("Rename…", Go),
    S::new(A::SessionAgentNext, "session.agent_next")
        .bar("Next Agent of New Session", Icon::ArrowsExchange),
    S::new(A::SessionAgentPrev, "session.agent_prev")
        .bar("Previous Agent of New Session", Icon::ArrowsExchange2),
    S::new(A::ShellOpen, "shell.open")
        .bar("Open Shell", Icon::Terminal2)
        .row("Open a shell here", Start),
    S::new(A::TerminalStop, "terminal.stop")
        .bar("Stop Terminal", Icon::SquareX)
        .row("Stop terminal", End),
    S::new(A::NeovimOpen, "neovim.open")
        .bar("Open Neovim", Icon::Code)
        .row("Open Neovim here", Start),
    S::new(A::TabClose, "tab.close")
        .bar("Close Tab", Icon::X)
        .row("Close tab", End),
    S::new(A::WindowClose, "window.close").bar("Close Window", Icon::AppWindow),
    S::new(A::Refresh, "refresh").bar("Refresh", Icon::Refresh),
    S::new(A::Filter, "filter").bar("Filter…", Icon::Filter),
    S::new(A::DetailsOverview, "details.overview").bar("Overview", Icon::InfoCircle),
    S::new(A::DetailsChanges, "details.changes").bar("Changes", Icon::FileDiff),
    S::new(A::DetailsLog, "details.log").bar("Log", Icon::GitCommit),
    S::new(A::WorktreesToggle, "worktrees.toggle").bar("Toggle All Worktrees", Icon::ListTree),
    S::new(A::ScrollDown, "scroll.down"),
    S::new(A::ScrollUp, "scroll.up"),
    S::new(A::ScrollHalfDown, "scroll.half_down"),
    S::new(A::ScrollHalfUp, "scroll.half_up"),
    S::new(A::ScrollTop, "scroll.top"),
    S::new(A::ScrollBottom, "scroll.bottom"),
    S::new(A::ConsoleSendNext, "console.send_next"),
    S::new(A::ConsolePassEveryKey, "console.pass_every_key"),
    S::new(A::PromptFocus, "prompt.focus").bar("Prompt", Icon::CursorText),
    S::new(A::PromptWrite, "prompt.write"),
    S::new(A::PromptSend, "prompt.send").bar("Send Prompt", Icon::Send),
    S::new(A::PromptImages, "prompt.images").bar("Select Attached Image", Icon::Photo),
    S::new(A::HistoryPrev, "prompt.history_prev"),
    S::new(A::HistoryNext, "prompt.history_next"),
    S::new(A::SettingsOpen, "settings.open").bar("Settings…", Icon::Settings),
    S::new(A::DaemonRestart, "daemon.restart").bar("Restart Session Daemon…", Icon::ServerBolt),
    S::new(A::ZoomIn, "zoom.in").bar("Zoom In", Icon::ZoomIn),
    S::new(A::ZoomOut, "zoom.out").bar("Zoom Out", Icon::ZoomOut),
    S::new(A::ZoomReset, "zoom.reset").bar("Actual Size", Icon::ZoomReset),
    S::new(A::TreeToggle, "tree.toggle").bar("Toggle Tree", Icon::LayoutSidebar),
    S::new(A::DetailsToggle, "details.toggle").bar("Toggle Details", Icon::LayoutSidebarRight),
    S::new(A::ViewerZoomIn, "viewer.zoom_in"),
    S::new(A::ViewerZoomOut, "viewer.zoom_out"),
    S::new(A::ViewerZoomFit, "viewer.zoom_fit"),
    S::new(A::ViewerPageNext, "viewer.page_next"),
    S::new(A::ViewerPagePrev, "viewer.page_prev"),
    S::new(A::ViewerPlay, "viewer.play"),
    S::new(A::ViewerBack, "viewer.back"),
    S::new(A::ViewerForward, "viewer.forward"),
    S::new(A::ViewerCopyPath, "viewer.copy_path"),
    S::new(A::ConsoleSend(None), "console.send"),
    S::new(A::TabGo(1), "tab.go"),
];

impl Action {
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
            SessionPick,
            SessionNew,
            SessionScratch,
            SessionTakeOver,
            SessionStop,
            SessionRename,
            SessionAgentNext,
            SessionAgentPrev,
            ShellOpen,
            TerminalStop,
            NeovimOpen,
            TabClose,
            WindowClose,
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
            HistoryPrev,
            HistoryNext,
            SettingsOpen,
            DaemonRestart,
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
