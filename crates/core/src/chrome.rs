//! The window's chrome: its title, how far it is zoomed, how its side panels
//! are laid out, what its status line says, the key bindings in force, and
//! whether the keys help is shown.

pub mod panels;
pub mod status_line;

use crate::effect::Effect;
use crate::keymap::{Context, Keymap};
use crate::zoom::{self, Step};

pub use panels::Panels;
pub use status_line::StatusLine;

#[derive(Debug, Clone, PartialEq)]
pub struct Chrome {
    /// Whether the window is the one in front, which the user is in.
    pub focused: bool,
    /// The zoom the window is drawn at.
    pub zoom: f64,
    /// The zoom the settings name, which a reset returns to.
    setting: f64,
    pub panels: Panels,
    pub status: StatusLine,
    /// The keys help, with the areas the keys were in when it was opened.
    pub keys_help: Option<Vec<Context>>,
    /// Every binding in force: the defaults, with the user's file over them.
    pub keymap: Keymap,
}

impl Default for Chrome {
    fn default() -> Self {
        Self {
            focused: true,
            zoom: 1.0,
            setting: 1.0,
            panels: Panels::default(),
            status: StatusLine::default(),
            keys_help: None,
            keymap: Keymap::defaults(),
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

/// A key asked to zoom in, out or back.
pub(crate) fn step(chrome: &mut Chrome, step: Step) -> Vec<Effect> {
    chrome.zoom = match step {
        Step::Reset => chrome.setting,
        _ => zoom::next(chrome.zoom, step),
    };
    vec![Effect::Zoom(chrome.zoom)]
}

/// The window is drawn at the zoom the settings name: a new zoom there
/// replaces the one the keys chose, and an unchanged one leaves it.
pub(crate) fn follow_setting(chrome: &mut Chrome, setting: f64) -> Vec<Effect> {
    if setting == chrome.setting {
        return Vec::new();
    }
    chrome.setting = setting;
    chrome.zoom = setting;
    vec![Effect::Zoom(setting)]
}

/// The keys help shown for the keys of `here`, nearest first.
pub(crate) fn open_keys_help(chrome: &mut Chrome, here: Vec<Context>) -> Vec<Effect> {
    chrome.keys_help = Some(here);
    Vec::new()
}

pub(crate) fn window_focused(chrome: &mut Chrome, on: bool) -> Vec<Effect> {
    chrome.focused = on;
    Vec::new()
}

pub(crate) fn close_keys_help(chrome: &mut Chrome) -> Vec<Effect> {
    chrome.keys_help = None;
    Vec::new()
}

#[cfg(test)]
mod tests {
    use crate::testing::here;
    use adroit_protocol::ConfigChange;

    use super::title;
    use crate::config::testing::{configured, reconfigured, settings_file};
    use crate::effect::Effect;
    use crate::keymap::Context;
    use crate::msg::Msg;
    use crate::scenario::Scenario;
    use crate::zoom::Step;

    #[test]
    fn the_zoom_follows_the_keys_and_the_settings() {
        let opened = |zoom: &str| {
            Scenario::new()
                .send(Msg::Opened {
                    environment: here(),
                    home: Default::default(),
                })
                .event(configured(&format!("zoom = {zoom}\n")))
        };
        let set = |zoom: &str| {
            reconfigured(
                1,
                ConfigChange::Settings {
                    file: settings_file(&format!("zoom = {zoom}\n")),
                },
            )
        };
        // (run, zoom drawn, zoom effects of the last message)
        let cases: Vec<(Scenario, f64, Vec<Effect>)> = vec![
            (opened("1.25"), 1.25, vec![Effect::Zoom(1.25)]),
            (opened("1.0"), 1.0, vec![]),
            (
                opened("1.0").send(Msg::Zoom(Step::In)),
                1.1,
                vec![Effect::Zoom(1.1)],
            ),
            (
                opened("1.0").send(Msg::Zoom(Step::Out)),
                0.9,
                vec![Effect::Zoom(0.9)],
            ),
            (
                opened("1.25")
                    .send(Msg::Zoom(Step::In))
                    .send(Msg::Zoom(Step::Reset)),
                1.25,
                vec![Effect::Zoom(1.25)],
            ),
            (
                opened("1.0").send(Msg::Zoom(Step::In)).event(set("0.8")),
                0.8,
                vec![Effect::Zoom(0.8)],
            ),
            (
                opened("1.0").send(Msg::Zoom(Step::In)).event(set("1.0")),
                1.1,
                vec![],
            ),
        ];
        for (at, (run, zoom, effects)) in cases.into_iter().enumerate() {
            assert_eq!(run.state().chrome.zoom, zoom, "case {at}");
            let last: Vec<_> = run
                .last_effects()
                .iter()
                .filter(|e| matches!(e, Effect::Zoom(_)))
                .cloned()
                .collect();
            assert_eq!(last, effects, "case {at}");
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
