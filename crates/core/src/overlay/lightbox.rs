//! The lightbox: a session's images looked at one at a time over the window.

use crate::actions::Action;
use crate::content::Tab;
use crate::cursor;
use crate::details;
use crate::effect::Effect;
use crate::state::ViewState;
use crate::viewers::Zoom;

/// How far a key moves the image, as a share of the lightbox's size.
const PAN_STEP: f64 = 0.1;

/// The images the lightbox steps through, the one shown, and how it is
/// drawn: its zoom, and how far it is moved, as shares of the lightbox's
/// width and height.
#[derive(Debug, Clone, PartialEq)]
pub struct Lightbox {
    pub images: Vec<Tab>,
    pub at: usize,
    pub zoom: Zoom,
    pub pan: (f64, f64),
}

impl Lightbox {
    fn of(images: Vec<Tab>, at: usize) -> Self {
        Lightbox {
            images,
            at,
            zoom: Zoom::FIT,
            pan: (0.0, 0.0),
        }
    }

    /// The image shown.
    pub fn shown(&self) -> &Tab {
        &self.images[self.at]
    }

    fn step(&mut self, forward: bool) {
        let delta = if forward { 1 } else { -1 };
        self.at = cursor::wrap(self.at, delta, self.images.len());
        self.zoom = Zoom::FIT;
        self.pan = (0.0, 0.0);
    }
}

/// Look at `thing` in the lightbox, among the images of Resources of the
/// session the details panel shows when it is one of them, or else alone.
pub(crate) fn open(state: &mut ViewState, thing: Tab) -> Vec<Effect> {
    let environment = state.details.shown.environment.clone();
    let local = state.hosts.is_local(&environment);
    let images = details::images(&state.details, state.hosts.work(&environment), local);
    state.overlays.lightbox = Some(match images.iter().position(|t| *t == thing) {
        Some(at) => Lightbox::of(images, at),
        None => Lightbox::of(vec![thing], 0),
    });
    Vec::new()
}

/// Run the lightbox's `action` on the lightbox open.
pub(crate) fn act(state: &mut ViewState, action: Action) -> Vec<Effect> {
    let Some(g) = state.overlays.lightbox.as_mut() else {
        return Vec::new();
    };
    match action {
        Action::LightboxNext => g.step(true),
        Action::LightboxPrev => g.step(false),
        Action::LightboxZoomIn => g.zoom = g.zoom.zoom_in(),
        Action::LightboxZoomOut => g.zoom = g.zoom.zoom_out(),
        Action::LightboxZoomFit => {
            g.zoom = Zoom::FIT;
            g.pan = (0.0, 0.0);
        }
        Action::LightboxPanLeft => g.pan.0 += PAN_STEP,
        Action::LightboxPanRight => g.pan.0 -= PAN_STEP,
        Action::LightboxPanUp => g.pan.1 += PAN_STEP,
        Action::LightboxPanDown => g.pan.1 -= PAN_STEP,
        Action::LightboxClose => state.overlays.lightbox = None,
        _ => {}
    }
    Vec::new()
}

/// Move the image shown by `dx` and `dy`, shares of the lightbox's width and
/// height, as dragging it does.
pub(crate) fn pan(state: &mut ViewState, dx: f64, dy: f64) -> Vec<Effect> {
    if let Some(g) = state.overlays.lightbox.as_mut() {
        g.pan.0 += dx;
        g.pan.1 += dy;
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::details::panel::tests::{drawn_tab, with_images};
    use crate::msg::Msg;
    use crate::overlay::Holder;
    use crate::scenario::Scenario;
    use crate::testing::here;
    use adroit_model::Key;

    fn plot() -> Tab {
        Tab::Viewer(here(), "/w/plot.png".into())
    }

    fn lightbox(run: &Scenario) -> Option<&Lightbox> {
        run.state().overlays.lightbox.as_ref()
    }

    fn held(name: &str) -> Msg {
        Msg::HeldKey(Key::new(name))
    }

    #[test]
    fn an_image_of_resources_is_looked_at_among_the_others_where_it_is() {
        let run = with_images().send(Msg::LightboxOpen(plot()));
        let g = lightbox(&run).unwrap();
        assert_eq!((g.images.clone(), g.at), (vec![drawn_tab(), plot()], 1));
        assert_eq!(crate::overlay::holder(run.state()), Some(Holder::Lightbox));
    }

    #[test]
    fn an_image_not_among_resources_is_looked_at_alone() {
        let other = Tab::Viewer(here(), "/w/other.png".into());
        let run = with_images().send(Msg::LightboxOpen(other.clone()));
        assert_eq!(lightbox(&run).unwrap().images, [other]);
    }

    #[test]
    fn the_lightbox_steps_round_its_images_each_drawn_whole_again() {
        let run = with_images()
            .send(Msg::LightboxOpen(drawn_tab()))
            .send(Msg::LightboxAction(Action::LightboxZoomIn))
            .send(Msg::LightboxPan { dx: 0.2, dy: 0.0 })
            .send(Msg::LightboxAction(Action::LightboxNext));
        let g = lightbox(&run).unwrap();
        assert_eq!((g.shown(), g.zoom, g.pan), (&plot(), Zoom::FIT, (0.0, 0.0)));
        let run = run.send(Msg::LightboxAction(Action::LightboxNext));
        assert_eq!(lightbox(&run).unwrap().shown(), &drawn_tab());
        let run = run.send(Msg::LightboxAction(Action::LightboxPrev));
        assert_eq!(lightbox(&run).unwrap().shown(), &plot());
    }

    #[test]
    fn fitting_the_image_again_puts_it_back_in_the_middle() {
        let run = with_images()
            .send(Msg::LightboxOpen(drawn_tab()))
            .send(Msg::LightboxAction(Action::LightboxZoomIn))
            .send(Msg::LightboxAction(Action::LightboxPanLeft))
            .send(Msg::LightboxAction(Action::LightboxPanUp));
        let g = lightbox(&run).unwrap();
        assert_eq!((g.zoom, g.pan), (Zoom::FIT.zoom_in(), (0.1, 0.1)));
        let run = run.send(Msg::LightboxAction(Action::LightboxZoomFit));
        let g = lightbox(&run).unwrap();
        assert_eq!((g.zoom, g.pan), (Zoom::FIT, (0.0, 0.0)));
    }

    #[test]
    fn the_lightbox_takes_its_keys_from_its_own_area_and_closes() {
        let run = with_images()
            .send(Msg::LightboxOpen(drawn_tab()))
            .send(held("l"));
        assert_eq!(lightbox(&run).unwrap().shown(), &plot());
        // A key the lightbox has no binding for does nothing behind it.
        let run = run.send(held("x"));
        assert!(lightbox(&run).is_some());
        let run = run.send(held("Escape"));
        assert_eq!(lightbox(&run), None);
        assert_eq!(crate::overlay::holder(run.state()), None);
    }
}
