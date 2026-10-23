//! The window's chrome: its title, how far it is zoomed, how its side panels
//! are laid out, what its status line says, and whether the keys help is
//! shown.

pub mod panels;
pub mod status_line;

use crate::effect::Effect;
use crate::keymap::Context;
use crate::zoom::{self, Step};

pub use panels::Panels;
pub use status_line::StatusLine;

#[derive(Debug, Clone, PartialEq)]
pub struct Chrome {
    /// The zoom the window is drawn at.
    pub zoom: f64,
    /// The zoom the settings name, which a reset returns to.
    setting: f64,
    pub panels: Panels,
    pub status: StatusLine,
    /// The keys help, with the areas the keys were in when it was opened.
    pub keys_help: Option<Vec<Context>>,
}

impl Default for Chrome {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            setting: 1.0,
            panels: Panels::default(),
            status: StatusLine::default(),
            keys_help: None,
        }
    }
}

/// The window's title: with what `git describe` said of the source, which
/// only a debug build is told.
pub fn title(described: Option<&str>) -> String {
    match described {
        Some(d) => format!("Adroit ({d})"),
        None => "Adroit".into(),
    }
}

/// The window opened with the settings' zoom.
pub(crate) fn open(chrome: &mut Chrome, setting: f64) -> Vec<Effect> {
    chrome.setting = setting;
    chrome.zoom = setting;
    vec![Effect::Zoom(setting)]
}

/// A key asked to zoom in, out or back.
pub(crate) fn step(chrome: &mut Chrome, step: Step) -> Vec<Effect> {
    chrome.zoom = match step {
        Step::Reset => chrome.setting,
        _ => zoom::next(chrome.zoom, step),
    };
    vec![Effect::Zoom(chrome.zoom)]
}

/// The settings were read again; a new zoom there replaces the one the keys
/// chose, and an unchanged one leaves it.
pub(crate) fn setting(chrome: &mut Chrome, setting: f64) -> Vec<Effect> {
    if setting == chrome.setting {
        return Vec::new();
    }
    open(chrome, setting)
}

/// The keys help shown for the keys of `here`, nearest first.
pub(crate) fn open_keys_help(chrome: &mut Chrome, here: Vec<Context>) -> Vec<Effect> {
    chrome.keys_help = Some(here);
    Vec::new()
}

pub(crate) fn close_keys_help(chrome: &mut Chrome) -> Vec<Effect> {
    chrome.keys_help = None;
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::title;
    use crate::effect::Effect;
    use crate::keymap::Context;
    use crate::msg::Msg;
    use crate::scenario::Scenario;
    use crate::zoom::Step;

    #[test]
    fn the_zoom_follows_the_keys_and_the_settings() {
        // (messages, zoom drawn, effects of the last message)
        let cases: Vec<(Vec<Msg>, f64, Vec<Effect>)> = vec![
            (
                vec![Msg::Opened { zoom: 1.25 }],
                1.25,
                vec![Effect::Zoom(1.25)],
            ),
            (
                vec![Msg::Opened { zoom: 1.0 }, Msg::Zoom(Step::In)],
                1.1,
                vec![Effect::Zoom(1.1)],
            ),
            (
                vec![Msg::Opened { zoom: 1.0 }, Msg::Zoom(Step::Out)],
                0.9,
                vec![Effect::Zoom(0.9)],
            ),
            (
                vec![
                    Msg::Opened { zoom: 1.25 },
                    Msg::Zoom(Step::In),
                    Msg::Zoom(Step::Reset),
                ],
                1.25,
                vec![Effect::Zoom(1.25)],
            ),
            (
                vec![
                    Msg::Opened { zoom: 1.0 },
                    Msg::Zoom(Step::In),
                    Msg::ZoomSetting(0.8),
                ],
                0.8,
                vec![Effect::Zoom(0.8)],
            ),
            (
                vec![
                    Msg::Opened { zoom: 1.0 },
                    Msg::Zoom(Step::In),
                    Msg::ZoomSetting(1.0),
                ],
                1.1,
                vec![],
            ),
        ];
        for (msgs, zoom, effects) in cases {
            let run = Scenario::new().send_all(msgs.clone());
            assert_eq!(run.state().chrome.zoom, zoom, "{msgs:?}");
            let last: Vec<_> = run
                .last_effects()
                .iter()
                .filter(|e| matches!(e, Effect::Zoom(_)))
                .cloned()
                .collect();
            assert_eq!(last, effects, "{msgs:?}");
        }
    }

    #[test]
    fn a_build_that_knows_its_source_names_it_in_the_title() {
        assert_eq!(title(Some("6b5486a-dirty")), "Adroit (6b5486a-dirty)");
        assert_eq!(title(None), "Adroit");
    }

    #[test]
    fn the_keys_help_opens_for_where_the_keys_are_and_closes() {
        let here = vec![Context::Tree, Context::Global];
        let run = Scenario::new().send(Msg::KeysHelpOpen(here.clone()));
        assert_eq!(run.state().chrome.keys_help, Some(here));
        let run = run.send(Msg::KeysHelpClose);
        assert_eq!(run.state().chrome.keys_help, None);
    }
}
