//! The window's icons. Which one each action is shown with, wherever it
//! appears, is told in `crate::actions`; how they are drawn is the window's.
//!
//! They are Tabler Icons 3.49.0, outline style, copied unchanged into
//! `icons/tabler/` with Tabler's MIT license beside them.

macro_rules! icons {
    ($($variant:ident => $file:literal,)*) => {
        /// An icon of the set.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Icon {
            $($variant,)*
        }

        impl Icon {
            pub const ALL: &[Icon] = &[$(Icon::$variant,)*];

            /// The icon's name in Tabler.
            pub fn name(self) -> &'static str {
                match self {
                    $(Icon::$variant => $file,)*
                }
            }

            /// The icon as Tabler ships it.
            pub fn svg(self) -> &'static str {
                match self {
                    $(Icon::$variant => include_str!(concat!("../icons/tabler/", $file, ".svg")),)*
                }
            }
        }
    };
}

icons! {
    AdjustmentsHorizontal => "adjustments-horizontal",
    AlertTriangle => "alert-triangle",
    AppWindow => "app-window",
    ArrowAutofitHeight => "arrow-autofit-height",
    ArrowAutofitWidth => "arrow-autofit-width",
    ArrowBack => "arrow-back",
    ArrowBackUp => "arrow-back-up",
    ArrowBarToDown => "arrow-bar-to-down",
    ArrowBarToLeft => "arrow-bar-to-left",
    ArrowBarToRight => "arrow-bar-to-right",
    ArrowBarToUp => "arrow-bar-to-up",
    ArrowBarLeft => "arrow-bar-left",
    ArrowBarRight => "arrow-bar-right",
    ArrowDownRight => "arrow-down-right",
    ArrowRight => "arrow-right",
    ArrowUpLeft => "arrow-up-left",
    ArrowsExchange => "arrows-exchange",
    ArrowsExchange2 => "arrows-exchange-2",
    Bell => "bell",
    BellDown => "bell-down",
    BellUp => "bell-up",
    Book => "book",
    BrowserPlus => "browser-plus",
    Check => "check",
    ChevronDown => "chevron-down",
    ChevronLeft => "chevron-left",
    ChevronRight => "chevron-right",
    Circle => "circle",
    Code => "code",
    ColumnInsertRight => "column-insert-right",
    ColumnRemove => "column-remove",
    Copy => "copy",
    Cpu => "cpu",
    Database => "database",
    Dots => "dots",
    CursorText => "cursor-text",
    Equal => "equal",
    Eye => "eye",
    File => "file",
    FileDiff => "file-diff",
    FilePencil => "file-pencil",
    Filter => "filter",
    Folder => "folder",
    FolderDown => "folder-down",
    FolderOpen => "folder-open",
    FolderSearch => "folder-search",
    FolderUp => "folder-up",
    GitBranch => "git-branch",
    GitCommit => "git-commit",
    Gpu => "gpu",
    InfoCircle => "info-circle",
    Keyboard => "keyboard",
    LayoutColumns => "layout-columns",
    LayoutDashboard => "layout-dashboard",
    LayoutRows => "layout-rows",
    LayoutSidebar => "layout-sidebar",
    LayoutSidebarRight => "layout-sidebar-right",
    Link => "link",
    ListTree => "list-tree",
    Loader => "loader-2",
    Memory => "memory",
    Message => "message",
    MessageDown => "message-down",
    MessagePlus => "message-plus",
    MessageUp => "message-up",
    Notes => "notes",
    Pencil => "pencil",
    Photo => "photo",
    PlayerStop => "player-stop",
    Plus => "plus",
    Refresh => "refresh",
    RotateClockwise => "rotate-clockwise",
    RowInsertBottom => "row-insert-bottom",
    RowRemove => "row-remove",
    Search => "search",
    Send => "send",
    ServerBolt => "server-bolt",
    Settings => "settings",
    Square => "square",
    SquareArrowDown => "square-arrow-down",
    SquareArrowLeft => "square-arrow-left",
    SquareArrowRight => "square-arrow-right",
    SquareArrowUp => "square-arrow-up",
    SquareMinus => "square-minus",
    SquarePlus => "square-plus",
    SquareX => "square-x",
    SwitchHorizontal => "switch-horizontal",
    Terminal => "terminal",
    Terminal2 => "terminal-2",
    TransferIn => "transfer-in",
    X => "x",
    ZoomIn => "zoom-in",
    ZoomOut => "zoom-out",
    ZoomReset => "zoom-reset",
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_a_tabler_outline_svg_drawn_in_the_text_s_colour() {
        for icon in Icon::ALL {
            let svg = icon.svg();
            assert!(svg.contains("viewBox=\"0 0 24 24\""), "{}", icon.name());
            assert!(svg.contains("stroke=\"currentColor\""), "{}", icon.name());
            assert!(svg.contains("fill=\"none\""), "{}", icon.name());
        }
    }
}
