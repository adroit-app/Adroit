//! The side panels as the window draws them: their layout, the room they
//! share with the console, and an edge being dragged.

use crate::effect::Effect;
use crate::layout::{Layout, SIDE, Side};

#[derive(Debug, Clone, PartialEq)]
pub struct Panels {
    /// The widths and which panels are shown, kept across runs.
    pub layout: Layout,
    /// How wide the window's body is drawn, which the panels share with the
    /// console.
    pub body: f64,
    /// A side panel's edge being dragged.
    pub drag: Option<Drag>,
}

impl Default for Panels {
    fn default() -> Self {
        Self {
            layout: Layout::default(),
            body: 1200.0,
            drag: None,
        }
    }
}

impl Panels {
    /// The widths the side panels are drawn at now.
    pub fn drawn(&self) -> (f64, f64) {
        self.layout.drawn(self.body)
    }
}

/// An edge taken by the pointer: whose, and where the pointer and the width
/// were when it was taken.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drag {
    pub side: Side,
    from: f64,
    width: f64,
}

fn keep(panels: &Panels) -> Vec<Effect> {
    vec![Effect::KeepLayout(panels.layout)]
}

/// The layout kept from the last run.
pub(crate) fn kept(panels: &mut Panels, layout: Layout) -> Vec<Effect> {
    panels.layout = layout;
    Vec::new()
}

/// The window's body was drawn `body` pixels wide.
pub(crate) fn resized(panels: &mut Panels, body: f64) -> Vec<Effect> {
    panels.body = body;
    Vec::new()
}

pub(crate) fn toggle(panels: &mut Panels, side: Side) -> Vec<Effect> {
    panels.layout = panels.layout.toggled(side);
    keep(panels)
}

/// `side` shown, if it is hidden.
pub(crate) fn show(panels: &mut Panels, side: Side) -> Vec<Effect> {
    let shown = match side {
        Side::Sidebar => panels.layout.show_sidebar,
        Side::Details => panels.layout.show_details,
    };
    if shown {
        return Vec::new();
    }
    toggle(panels, side)
}

/// `side` back at its width as designed.
pub(crate) fn reset(panels: &mut Panels, side: Side) -> Vec<Effect> {
    match side {
        Side::Sidebar => panels.layout.sidebar = SIDE,
        Side::Details => panels.layout.details = SIDE,
    }
    keep(panels)
}

/// The pointer took `side`'s edge at `x`.
pub(crate) fn take(panels: &mut Panels, side: Side, x: f64) -> Vec<Effect> {
    let (sidebar, details) = panels.drawn();
    let width = match side {
        Side::Sidebar => sidebar,
        Side::Details => details,
    };
    panels.drag = Some(Drag {
        side,
        from: x,
        width,
    });
    Vec::new()
}

/// The pointer moved to `x`, carrying the edge it took, if any.
pub(crate) fn drag(panels: &mut Panels, x: f64) -> Vec<Effect> {
    let Some(Drag { side, from, width }) = panels.drag else {
        return Vec::new();
    };
    let moved = match side {
        Side::Sidebar => x - from,
        Side::Details => from - x,
    };
    panels.layout = panels.layout.dragged(side, width + moved, panels.body);
    Vec::new()
}

/// The pointer let go, or left the window; an edge it held keeps where it
/// was left.
pub(crate) fn drop(panels: &mut Panels) -> Vec<Effect> {
    if panels.drag.take().is_none() {
        return Vec::new();
    }
    keep(panels)
}

#[cfg(test)]
mod tests {
    use crate::effect::Effect;
    use crate::layout::{Layout, MIN_SIDE, SIDE, Side};
    use crate::msg::Msg;
    use crate::scenario::Scenario;

    fn kept(sidebar: f64, details: f64) -> Msg {
        Msg::PanelsKept(Layout {
            sidebar,
            details,
            ..Layout::default()
        })
    }

    fn keeps(run: &Scenario) -> Vec<Layout> {
        run.last_effects()
            .iter()
            .filter_map(|e| match e {
                Effect::KeepLayout(l) => Some(*l),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn an_edge_follows_the_pointer_and_is_kept_once_let_go() {
        // (messages after a layout of 300 and 300 in a body 1400 wide,
        // tree width, details width, still dragging)
        let cases: Vec<(Vec<Msg>, f64, f64, bool)> = vec![
            (
                vec![
                    Msg::PanelDragStart {
                        side: Side::Sidebar,
                        x: 300.0,
                    },
                    Msg::PanelDragMove { x: 420.0 },
                ],
                420.0,
                300.0,
                true,
            ),
            (
                vec![
                    Msg::PanelDragStart {
                        side: Side::Details,
                        x: 1100.0,
                    },
                    Msg::PanelDragMove { x: 1000.0 },
                ],
                300.0,
                400.0,
                true,
            ),
            (
                vec![
                    Msg::PanelDragStart {
                        side: Side::Sidebar,
                        x: 300.0,
                    },
                    Msg::PanelDragMove { x: 0.0 },
                    Msg::PanelDragEnd,
                ],
                MIN_SIDE,
                300.0,
                false,
            ),
            (vec![Msg::PanelDragMove { x: 900.0 }], 300.0, 300.0, false),
        ];
        for (msgs, sidebar, details, dragging) in cases {
            let run = Scenario::new()
                .send(kept(300.0, 300.0))
                .send(Msg::BodyResized(1400.0))
                .send_all(msgs.clone());
            let panels = &run.state().chrome.panels;
            assert_eq!(panels.layout.sidebar, sidebar, "{msgs:?}");
            assert_eq!(panels.layout.details, details, "{msgs:?}");
            assert_eq!(panels.drag.is_some(), dragging, "{msgs:?}");
        }
    }

    #[test]
    fn only_a_change_the_user_made_is_kept() {
        // (message after a layout of 250 and 350, layouts kept)
        let cases: Vec<(Msg, Vec<Layout>)> = vec![
            (kept(200.0, 200.0), vec![]),
            (Msg::BodyResized(900.0), vec![]),
            (Msg::PanelDragEnd, vec![]),
            (
                Msg::PanelToggle(Side::Details),
                vec![Layout {
                    sidebar: 250.0,
                    details: 350.0,
                    show_details: false,
                    ..Layout::default()
                }],
            ),
            (Msg::PanelShow(Side::Details), vec![]),
            (
                Msg::PanelReset(Side::Sidebar),
                vec![Layout {
                    sidebar: SIDE,
                    details: 350.0,
                    ..Layout::default()
                }],
            ),
        ];
        for (msg, kept_now) in cases {
            let run = Scenario::new().send(kept(250.0, 350.0)).send(msg.clone());
            assert_eq!(keeps(&run), kept_now, "{msg:?}");
        }
    }

    #[test]
    fn showing_a_hidden_panel_brings_it_back_and_a_shown_one_stays() {
        let run = Scenario::new()
            .send(Msg::PanelToggle(Side::Details))
            .send(Msg::PanelShow(Side::Details));
        assert!(run.state().chrome.panels.layout.show_details);
        assert_eq!(keeps(&run).len(), 1);
        let run = run.send(Msg::PanelShow(Side::Details));
        assert!(run.state().chrome.panels.layout.show_details);
        assert!(keeps(&run).is_empty());
    }

    #[test]
    fn an_edge_is_taken_at_the_width_it_is_drawn_at() {
        // A body of 944 draws both panels at 232 to leave the console its
        // floor; the drag starts from there, not from the kept 300.
        let run = Scenario::new()
            .send(Msg::BodyResized(944.0))
            .send(Msg::PanelDragStart {
                side: Side::Sidebar,
                x: 232.0,
            })
            .send(Msg::PanelDragMove { x: 222.0 });
        assert_eq!(run.state().chrome.panels.layout.sidebar, 222.0);
    }
}
