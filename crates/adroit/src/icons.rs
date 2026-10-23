//! The window's icons, and the one icon each action is shown with wherever
//! it appears: the menu bar, the right-click menu and the buttons.
//!
//! They are Tabler Icons 3.49.0, outline style, copied unchanged into
//! `icons/tabler/` with Tabler's MIT license beside them.

use dioxus::prelude::*;

use crate::keymap::Action;

/// How thick every icon's lines are drawn, in the 24-unit grid Tabler draws
/// in.
pub const STROKE: &str = "1.75";

macro_rules! icons {
    ($($variant:ident => $file:literal,)*) => {
        /// An icon of the set.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Icon {
            $($variant,)*
        }

        impl Icon {
            #[cfg(test)]
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
    FolderOpen => "folder-open",
    FolderSearch => "folder-search",
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

impl Icon {
    /// The icon `action` is shown with; `None` for an action that is only a
    /// key.
    pub fn of(action: &Action) -> Option<Icon> {
        use Action as A;
        Some(match action {
            A::SettingsOpen => Icon::Settings,
            A::DaemonRestart => Icon::ServerBolt,
            A::SessionNew => Icon::MessagePlus,
            A::SessionScratch => Icon::Notes,
            A::SessionPick => Icon::Search,
            A::SessionAgentNext => Icon::ArrowsExchange,
            A::SessionAgentPrev => Icon::ArrowsExchange2,
            A::PromptSend => Icon::Send,
            A::PromptImages => Icon::Photo,
            A::ShellOpen => Icon::Terminal2,
            A::NeovimOpen => Icon::Code,
            A::TerminalStop => Icon::SquareX,
            A::TabClose => Icon::X,
            A::WindowClose => Icon::AppWindow,
            A::SessionRename => Icon::Pencil,
            A::SessionTakeOver => Icon::TransferIn,
            A::SessionStop => Icon::PlayerStop,
            A::TreeToggle => Icon::LayoutSidebar,
            A::DetailsToggle => Icon::LayoutSidebarRight,
            A::DetailsOverview => Icon::InfoCircle,
            A::DetailsChanges => Icon::FileDiff,
            A::DetailsLog => Icon::GitCommit,
            A::Filter => Icon::Filter,
            A::WorktreesToggle => Icon::ListTree,
            A::Refresh => Icon::Refresh,
            A::ZoomIn => Icon::ZoomIn,
            A::ZoomOut => Icon::ZoomOut,
            A::ZoomReset => Icon::ZoomReset,
            A::TabNext => Icon::ChevronRight,
            A::TabPrev => Icon::ChevronLeft,
            A::SessionNext => Icon::MessageDown,
            A::SessionPrev => Icon::MessageUp,
            A::WaitingNext => Icon::BellDown,
            A::WaitingPrev => Icon::BellUp,
            A::WindowLeft => Icon::SquareArrowLeft,
            A::WindowDown => Icon::SquareArrowDown,
            A::WindowUp => Icon::SquareArrowUp,
            A::WindowRight => Icon::SquareArrowRight,
            A::WindowNext => Icon::ArrowBarRight,
            A::WindowPrev => Icon::ArrowBarLeft,
            A::WindowTop => Icon::ArrowUpLeft,
            A::WindowBottom => Icon::ArrowDownRight,
            A::WindowBack => Icon::ArrowBackUp,
            A::WindowVsplit => Icon::LayoutColumns,
            A::WindowSplit => Icon::LayoutRows,
            A::WindowNew => Icon::SquarePlus,
            A::WindowQuit => Icon::SquareMinus,
            A::WindowOnly => Icon::Square,
            A::WindowMoveLeft => Icon::ArrowBarToLeft,
            A::WindowMoveDown => Icon::ArrowBarToDown,
            A::WindowMoveUp => Icon::ArrowBarToUp,
            A::WindowMoveRight => Icon::ArrowBarToRight,
            A::WindowExchange => Icon::SwitchHorizontal,
            A::WindowRotate => Icon::RotateClockwise,
            A::WindowEqualize => Icon::Equal,
            A::WindowNarrower => Icon::ColumnRemove,
            A::WindowWider => Icon::ColumnInsertRight,
            A::WindowTaller => Icon::RowInsertBottom,
            A::WindowShorter => Icon::RowRemove,
            A::WindowMaximizeHeight => Icon::ArrowAutofitHeight,
            A::WindowMaximizeWidth => Icon::ArrowAutofitWidth,
            A::WindowToTab => Icon::BrowserPlus,
            A::TabLast => Icon::ArrowBack,
            A::PromptFocus => Icon::CursorText,
            A::CursorOpen => Icon::ArrowRight,
            A::CursorShow => Icon::Eye,
            A::CursorCopy => Icon::Copy,
            A::CursorReveal => Icon::FolderSearch,
            _ => return None,
        })
    }

    /// The icon drawn `px` pixels square as RGBA, black on clear: the shape
    /// alone, for macOS to colour as its menus are.
    pub fn rgba(self, px: u32) -> Option<Vec<u8>> {
        use resvg::{tiny_skia, usvg};
        let svg = self
            .svg()
            .replace("stroke-width=\"2\"", &format!("stroke-width=\"{STROKE}\""));
        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).ok()?;
        let mut pixmap = tiny_skia::Pixmap::new(px, px)?;
        let scale = px as f32 / tree.size().width();
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        Some(pixmap.take())
    }
}

/// An icon drawn in the text's colour, `size` pixels square.
#[component]
pub fn IconView(icon: Icon, #[props(default = 14)] size: u32) -> Element {
    rsx! {
        span {
            class: "icon icon-{icon.name()}",
            style: "--icon:{size}px",
            "aria-hidden": "true",
            dangerous_inner_html: icon.svg(),
        }
    }
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

    #[test]
    fn an_icon_is_drawn_as_many_pixels_square_as_asked() {
        let rgba = Icon::Settings.rgba(36).unwrap();
        assert_eq!(rgba.len(), 36 * 36 * 4);
    }

    #[test]
    fn a_drawn_icon_is_its_shape_in_black_on_clear() {
        let rgba = Icon::Plus.rgba(36).unwrap();
        let pixels: Vec<&[u8]> = rgba.chunks(4).collect();
        assert!(pixels.iter().any(|p| p[3] > 0), "something is drawn");
        assert!(pixels.iter().any(|p| p[3] == 0), "around it is clear");
        assert!(pixels.iter().all(|p| p[..3] == [0, 0, 0]));
        // The plus crosses at the centre and leaves the corners clear.
        assert!(pixels[18 * 36 + 18][3] > 0);
        assert_eq!(pixels[0][3], 0);
    }
}
