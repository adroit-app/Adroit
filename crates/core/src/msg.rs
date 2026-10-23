//! What can happen to the window: what the user does, what the window is
//! told, and what the work host says.

use std::collections::BTreeMap;

use adroit_model::TerminalSession;
use adroit_protocol::Event;

use crate::details::{self, Sheet};
use crate::keymap::Context;
use crate::layout::{Layout, Side};
use crate::tree::nav::Item;
use crate::tree::{self, Showing};
use crate::zoom;

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    /// The window opened, with the zoom the settings name.
    Opened {
        zoom: f64,
    },
    /// A key asked to zoom.
    Zoom(zoom::Step),
    /// The settings were read again, naming this zoom.
    ZoomSetting(f64),

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

    // The tree.
    /// What the content's focused window shows, and where.
    Showing(Box<Showing>),
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
}
