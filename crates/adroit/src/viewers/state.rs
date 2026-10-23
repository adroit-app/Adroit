//! What a viewer holds between keys: zoom, page and playback.

use std::fmt;
use std::str::FromStr;

use super::kind::FileKind;

macro_rules! actions {
    ($($variant:ident => $name:literal,)*) => {
        /// What a key does in a viewer.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum ViewerAction {
            $($variant,)*
        }

        impl ViewerAction {
            /// Every action's name, as the keys file names them.
            pub const NAMES: &[&str] = &[$($name,)*];

            /// The action's name, as the keys file names it.
            pub fn name(self) -> &'static str {
                match self {
                    $(ViewerAction::$variant => $name,)*
                }
            }
        }

        impl FromStr for ViewerAction {
            type Err = UnknownAction;

            fn from_str(text: &str) -> Result<Self, UnknownAction> {
                match text {
                    $($name => Ok(ViewerAction::$variant),)*
                    _ => Err(UnknownAction(text.to_string())),
                }
            }
        }
    };
}

actions! {
    ScrollDown => "scroll.down",
    ScrollUp => "scroll.up",
    ScrollHalfDown => "scroll.half_down",
    ScrollHalfUp => "scroll.half_up",
    ScrollTop => "scroll.top",
    ScrollBottom => "scroll.bottom",
    ZoomIn => "viewer.zoom_in",
    ZoomOut => "viewer.zoom_out",
    ZoomFit => "viewer.zoom_fit",
    PageNext => "viewer.page_next",
    PagePrev => "viewer.page_prev",
    PlayToggle => "viewer.play",
    SeekBack => "viewer.back",
    SeekForward => "viewer.forward",
    CopyPath => "viewer.copy_path",
    Close => "window.close",
}

impl fmt::Display for ViewerAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A name no viewer action has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownAction(pub String);

impl fmt::Display for UnknownAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} is not a viewer action", self.0)
    }
}

impl std::error::Error for UnknownAction {}

/// The keys a viewer answers to unless the user binds others, as the keys
/// file writes them.
pub const DEFAULT_BINDINGS: &[(&str, &str)] = &[
    ("j", "scroll.down"),
    ("ArrowDown", "scroll.down"),
    ("k", "scroll.up"),
    ("ArrowUp", "scroll.up"),
    ("Ctrl+d", "scroll.half_down"),
    ("Ctrl+u", "scroll.half_up"),
    ("g g", "scroll.top"),
    ("G", "scroll.bottom"),
    ("+", "viewer.zoom_in"),
    ("=", "viewer.zoom_in"),
    ("-", "viewer.zoom_out"),
    ("0", "viewer.zoom_fit"),
    ("J", "viewer.page_next"),
    ("K", "viewer.page_prev"),
    ("Space", "viewer.play"),
    ("h", "viewer.back"),
    ("l", "viewer.forward"),
    ("y", "viewer.copy_path"),
    ("q", "window.close"),
];

/// How large a picture is drawn, as steps from fitting the window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Zoom(i32);

impl Zoom {
    const STEP: f64 = 1.25;
    const LEAST: i32 = -6;
    const MOST: i32 = 10;

    /// Drawn whole within the window.
    pub const FIT: Zoom = Zoom(0);

    pub fn zoom_in(self) -> Zoom {
        Zoom((self.0 + 1).min(Self::MOST))
    }

    pub fn zoom_out(self) -> Zoom {
        Zoom((self.0 - 1).max(Self::LEAST))
    }

    /// The size against the size that fits the window.
    pub fn factor(self) -> f64 {
        Self::STEP.powi(self.0)
    }

    /// The factor in whole percent, as the viewer shows it.
    pub fn percent(self) -> u32 {
        (self.factor() * 100.0).round() as u32
    }
}

/// A scroll a key asks of the viewer's page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scroll {
    Lines(i32),
    HalfPages(i32),
    Top,
    Bottom,
}

/// What a key asks of a movie being played.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Media {
    Play,
    Pause,
    SeekTo(f64),
}

/// What the viewer does once an action has changed its state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    /// The action means nothing to this viewer.
    Nothing,
    /// Draw the state anew.
    Redraw,
    Scroll(Scroll),
    Media(Media),
    CopyPath,
    Close,
}

/// A picture's state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ImageState {
    pub zoom: Zoom,
}

/// A PDF's state: the page shown, of how many, and how large.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PdfState {
    /// From 0.
    pub page: usize,
    /// None until the document is read, and when it cannot be.
    pub pages: Option<usize>,
    pub zoom: Zoom,
}

impl PdfState {
    /// Whether the page changed.
    fn turn(&mut self, by: isize) -> bool {
        let Some(pages) = self.pages.filter(|&p| p > 0) else {
            return false;
        };
        let to = self.page.saturating_add_signed(by).min(pages - 1);
        let turned = to != self.page;
        self.page = to;
        turned
    }

    /// Learn the document's page count, keeping the page within it.
    pub fn read(&mut self, pages: Option<usize>) {
        self.pages = pages;
        self.page = self.page.min(pages.unwrap_or(1).max(1) - 1);
    }
}

/// A movie's state, as the player last told it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MovieState {
    pub playing: bool,
    /// Seconds from the start.
    pub position: f64,
    /// Seconds; None until the player knows.
    pub duration: Option<f64>,
}

impl MovieState {
    /// How far `h` and `l` move, in seconds.
    pub const SEEK: f64 = 5.0;

    /// Learn where the player is.
    pub fn observe(&mut self, playing: bool, position: f64, duration: Option<f64>) {
        self.playing = playing;
        self.position = position.max(0.0);
        self.duration = duration.filter(|d| d.is_finite() && *d > 0.0);
    }

    fn seek(&mut self, by: f64) -> Effect {
        let to = (self.position + by).max(0.0);
        let to = self.duration.map_or(to, |d| to.min(d));
        self.position = to;
        Effect::Media(Media::SeekTo(to))
    }
}

/// An HTML page's state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HtmlState {
    pub zoom: Zoom,
}

/// One viewer's state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewerState {
    Image(ImageState),
    Movie(MovieState),
    Html(HtmlState),
    Pdf(PdfState),
}

impl ViewerState {
    /// A fresh state for a file of `kind`; None for a kind Neovim opens.
    pub fn new(kind: FileKind) -> Option<ViewerState> {
        Some(match kind {
            FileKind::Text => return None,
            FileKind::Image => ViewerState::Image(ImageState::default()),
            FileKind::Movie => ViewerState::Movie(MovieState::default()),
            FileKind::Html => ViewerState::Html(HtmlState::default()),
            FileKind::Pdf => ViewerState::Pdf(PdfState::default()),
        })
    }

    fn zoom_mut(&mut self) -> Option<&mut Zoom> {
        match self {
            ViewerState::Image(s) => Some(&mut s.zoom),
            ViewerState::Html(s) => Some(&mut s.zoom),
            ViewerState::Pdf(s) => Some(&mut s.zoom),
            ViewerState::Movie(_) => None,
        }
    }

    /// Run `action`, and say what the viewer is to do about it.
    pub fn apply(&mut self, action: ViewerAction) -> Effect {
        use ViewerAction as A;
        match action {
            A::ScrollDown => Effect::Scroll(Scroll::Lines(1)),
            A::ScrollUp => Effect::Scroll(Scroll::Lines(-1)),
            A::ScrollHalfDown => Effect::Scroll(Scroll::HalfPages(1)),
            A::ScrollHalfUp => Effect::Scroll(Scroll::HalfPages(-1)),
            A::ScrollTop => Effect::Scroll(Scroll::Top),
            A::ScrollBottom => Effect::Scroll(Scroll::Bottom),
            A::ZoomIn | A::ZoomOut | A::ZoomFit => {
                let Some(zoom) = self.zoom_mut() else {
                    return Effect::Nothing;
                };
                let to = match action {
                    A::ZoomIn => zoom.zoom_in(),
                    A::ZoomOut => zoom.zoom_out(),
                    _ => Zoom::FIT,
                };
                if to == *zoom {
                    return Effect::Nothing;
                }
                *zoom = to;
                Effect::Redraw
            }
            A::PageNext | A::PagePrev => match self {
                ViewerState::Pdf(s) => {
                    if s.turn(if action == A::PageNext { 1 } else { -1 }) {
                        Effect::Redraw
                    } else {
                        Effect::Nothing
                    }
                }
                _ => Effect::Nothing,
            },
            A::PlayToggle => match self {
                ViewerState::Movie(s) => {
                    s.playing = !s.playing;
                    Effect::Media(if s.playing { Media::Play } else { Media::Pause })
                }
                _ => Effect::Nothing,
            },
            A::SeekBack | A::SeekForward => match self {
                ViewerState::Movie(s) => s.seek(if action == A::SeekBack {
                    -MovieState::SEEK
                } else {
                    MovieState::SEEK
                }),
                _ => Effect::Nothing,
            },
            A::CopyPath => Effect::CopyPath,
            A::Close => Effect::Close,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pdf(page: usize, pages: usize) -> ViewerState {
        ViewerState::Pdf(PdfState {
            page,
            pages: Some(pages),
            zoom: Zoom::FIT,
        })
    }

    fn movie(position: f64, duration: Option<f64>, playing: bool) -> ViewerState {
        ViewerState::Movie(MovieState {
            playing,
            position,
            duration,
        })
    }

    #[test]
    fn every_action_is_found_again_by_its_name() {
        for name in ViewerAction::NAMES {
            let action: ViewerAction = name.parse().unwrap();
            assert_eq!(action.name(), *name);
        }
        assert!("viewer.nothing".parse::<ViewerAction>().is_err());
    }

    #[test]
    fn every_default_binding_names_a_viewer_action() {
        for (keys, name) in DEFAULT_BINDINGS {
            assert!(name.parse::<ViewerAction>().is_ok(), "{keys} = {name}");
        }
    }

    #[test]
    fn a_viewer_opens_for_every_kind_but_text() {
        assert_eq!(ViewerState::new(FileKind::Text), None);
        for kind in [
            FileKind::Image,
            FileKind::Movie,
            FileKind::Html,
            FileKind::Pdf,
        ] {
            assert!(ViewerState::new(kind).is_some(), "{kind:?}");
        }
    }

    #[test]
    fn a_picture_first_fits_the_window() {
        assert_eq!(Zoom::default(), Zoom::FIT);
        assert_eq!(Zoom::FIT.percent(), 100);
    }

    #[test]
    fn zooming_in_and_out_steps_by_a_quarter() {
        assert_eq!(Zoom::FIT.zoom_in().percent(), 125);
        assert_eq!(Zoom::FIT.zoom_in().zoom_in().percent(), 156);
        assert_eq!(Zoom::FIT.zoom_out().percent(), 80);
        assert_eq!(Zoom::FIT.zoom_in().zoom_out(), Zoom::FIT);
    }

    #[test]
    fn zoom_stops_at_its_least_and_most() {
        let most = (0..100).fold(Zoom::FIT, |z, _| z.zoom_in());
        assert_eq!(most.zoom_in(), most);
        assert_eq!(most.percent(), 931);
        let least = (0..100).fold(Zoom::FIT, |z, _| z.zoom_out());
        assert_eq!(least.zoom_out(), least);
        assert_eq!(least.percent(), 26);
    }

    #[test]
    fn zoom_keys_change_the_picture_and_zero_fits_it_again() {
        let mut s = ViewerState::new(FileKind::Image).unwrap();
        assert_eq!(s.apply(ViewerAction::ZoomIn), Effect::Redraw);
        assert_eq!(s.apply(ViewerAction::ZoomIn), Effect::Redraw);
        assert_eq!(s.apply(ViewerAction::ZoomFit), Effect::Redraw);
        assert_eq!(s, ViewerState::Image(ImageState { zoom: Zoom::FIT }));
        assert_eq!(s.apply(ViewerAction::ZoomFit), Effect::Nothing);
    }

    #[test]
    fn html_pages_and_pdfs_zoom_too_but_movies_do_not() {
        for kind in [FileKind::Html, FileKind::Pdf] {
            let mut s = ViewerState::new(kind).unwrap();
            assert_eq!(s.apply(ViewerAction::ZoomOut), Effect::Redraw, "{kind:?}");
        }
        let mut s = ViewerState::new(FileKind::Movie).unwrap();
        assert_eq!(s.apply(ViewerAction::ZoomIn), Effect::Nothing);
    }

    #[test]
    fn scroll_keys_scroll_whatever_the_viewer() {
        let cases = [
            (ViewerAction::ScrollDown, Scroll::Lines(1)),
            (ViewerAction::ScrollUp, Scroll::Lines(-1)),
            (ViewerAction::ScrollHalfDown, Scroll::HalfPages(1)),
            (ViewerAction::ScrollHalfUp, Scroll::HalfPages(-1)),
            (ViewerAction::ScrollTop, Scroll::Top),
            (ViewerAction::ScrollBottom, Scroll::Bottom),
        ];
        for kind in [
            FileKind::Image,
            FileKind::Movie,
            FileKind::Html,
            FileKind::Pdf,
        ] {
            let mut s = ViewerState::new(kind).unwrap();
            for (action, scroll) in cases {
                assert_eq!(s.apply(action), Effect::Scroll(scroll), "{kind:?} {action}");
            }
        }
    }

    #[test]
    fn page_keys_turn_a_pdf_s_pages_within_the_document() {
        let mut s = pdf(0, 3);
        assert_eq!(s.apply(ViewerAction::PagePrev), Effect::Nothing);
        assert_eq!(s.apply(ViewerAction::PageNext), Effect::Redraw);
        assert_eq!(s.apply(ViewerAction::PageNext), Effect::Redraw);
        assert_eq!(s.apply(ViewerAction::PageNext), Effect::Nothing);
        assert_eq!(s, pdf(2, 3));
        assert_eq!(s.apply(ViewerAction::PagePrev), Effect::Redraw);
        assert_eq!(s, pdf(1, 3));
    }

    #[test]
    fn pages_do_not_turn_before_the_document_is_read() {
        let mut s = ViewerState::new(FileKind::Pdf).unwrap();
        assert_eq!(s.apply(ViewerAction::PageNext), Effect::Nothing);
    }

    #[test]
    fn page_keys_mean_nothing_outside_a_pdf() {
        let mut s = ViewerState::new(FileKind::Image).unwrap();
        assert_eq!(s.apply(ViewerAction::PageNext), Effect::Nothing);
    }

    #[test]
    fn a_document_read_shorter_than_the_page_shown_shows_its_last_page() {
        let mut s = PdfState {
            page: 9,
            ..PdfState::default()
        };
        s.read(Some(4));
        assert_eq!(s.page, 3);
        s.read(None);
        assert_eq!(s.page, 0);
    }

    #[test]
    fn space_plays_a_paused_movie_and_pauses_a_playing_one() {
        let mut s = movie(0.0, None, false);
        assert_eq!(
            s.apply(ViewerAction::PlayToggle),
            Effect::Media(Media::Play)
        );
        assert_eq!(
            s.apply(ViewerAction::PlayToggle),
            Effect::Media(Media::Pause)
        );
    }

    #[test]
    fn seeking_moves_five_seconds_within_the_movie() {
        let mut s = movie(12.0, Some(15.0), true);
        assert_eq!(
            s.apply(ViewerAction::SeekBack),
            Effect::Media(Media::SeekTo(7.0))
        );
        assert_eq!(
            s.apply(ViewerAction::SeekForward),
            Effect::Media(Media::SeekTo(12.0))
        );
        assert_eq!(
            s.apply(ViewerAction::SeekForward),
            Effect::Media(Media::SeekTo(15.0))
        );
        let mut s = movie(3.0, Some(15.0), true);
        assert_eq!(
            s.apply(ViewerAction::SeekBack),
            Effect::Media(Media::SeekTo(0.0))
        );
    }

    #[test]
    fn seeking_forward_is_not_held_back_before_the_length_is_known() {
        let mut s = movie(12.0, None, false);
        assert_eq!(
            s.apply(ViewerAction::SeekForward),
            Effect::Media(Media::SeekTo(17.0))
        );
    }

    #[test]
    fn the_player_s_report_replaces_what_the_movie_state_held() {
        let mut s = MovieState::default();
        s.observe(true, 4.5, Some(60.0));
        assert_eq!(
            s,
            MovieState {
                playing: true,
                position: 4.5,
                duration: Some(60.0)
            }
        );
        s.observe(false, 1.0, Some(f64::INFINITY));
        assert_eq!(s.duration, None);
    }

    #[test]
    fn copying_the_path_and_closing_are_asked_of_the_viewer_not_its_state() {
        let mut s = ViewerState::new(FileKind::Pdf).unwrap();
        assert_eq!(s.apply(ViewerAction::CopyPath), Effect::CopyPath);
        assert_eq!(s.apply(ViewerAction::Close), Effect::Close);
    }
}
