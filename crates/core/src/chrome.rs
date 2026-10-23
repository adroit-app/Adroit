//! The window's chrome: how far it is zoomed.

use crate::effect::Effect;
use crate::zoom::{self, Step};

#[derive(Debug, Clone, PartialEq)]
pub struct Chrome {
    /// The zoom the window is drawn at.
    pub zoom: f64,
    /// The zoom the settings name, which a reset returns to.
    setting: f64,
}

impl Default for Chrome {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            setting: 1.0,
        }
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

#[cfg(test)]
mod tests {
    use crate::effect::Effect;
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
}
