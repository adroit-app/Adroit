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
    ArrowRight => "arrow-right",
    Bell => "bell",
    BellDown => "bell-down",
    BellUp => "bell-up",
    Book => "book",
    Check => "check",
    ChevronDown => "chevron-down",
    ChevronLeft => "chevron-left",
    ChevronRight => "chevron-right",
    Circle => "circle",
    CursorText => "cursor-text",
    FileDiff => "file-diff",
    Filter => "filter",
    Folder => "folder",
    FolderOpen => "folder-open",
    GitBranch => "git-branch",
    GitCommit => "git-commit",
    InfoCircle => "info-circle",
    Keyboard => "keyboard",
    LayoutDashboard => "layout-dashboard",
    LayoutSidebar => "layout-sidebar",
    LayoutSidebarRight => "layout-sidebar-right",
    ListTree => "list-tree",
    Loader => "loader-2",
    Message => "message",
    MessageDown => "message-down",
    MessagePlus => "message-plus",
    MessageUp => "message-up",
    Pencil => "pencil",
    PlayerStop => "player-stop",
    Plus => "plus",
    Refresh => "refresh",
    Search => "search",
    Send => "send",
    Settings => "settings",
    SquareArrowDown => "square-arrow-down",
    SquareArrowLeft => "square-arrow-left",
    SquareArrowRight => "square-arrow-right",
    SquareArrowUp => "square-arrow-up",
    SquareX => "square-x",
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
            A::SessionNew => Icon::MessagePlus,
            A::SessionNewReader => Icon::Book,
            A::SessionPick => Icon::Search,
            A::PromptSend => Icon::Send,
            A::ShellOpen => Icon::Terminal2,
            A::ShellClose => Icon::SquareX,
            A::TabClose => Icon::X,
            A::SessionRename => Icon::Pencil,
            A::SessionTakeOver => Icon::TransferIn,
            A::SessionStop => Icon::PlayerStop,
            A::SidebarToggle => Icon::LayoutSidebar,
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
            A::PromptFocus => Icon::CursorText,
            A::CursorOpen => Icon::ArrowRight,
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
