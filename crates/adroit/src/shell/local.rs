//! What the window keeps of its own outside the view state: what its
//! consoles are measured and drawn at, the images they drew, the field the
//! filter is typed into and the latest command for a viewer.

use std::collections::HashMap;
use std::rc::Rc;

use adroit_term::Size;
use dioxus::prelude::*;

use crate::content::WindowId;
use crate::pictures::Gallery;
use crate::style::{CellHeight, DEFAULT_CELL_HEIGHT};
use crate::viewers;

#[derive(Clone, Copy)]
pub(crate) struct Local {
    /// Moves when a pane on screen has new output.
    pub frame: Signal<u64>,
    /// Moves when a pane is started, for what the console says around it.
    pub attached: Signal<u64>,
    /// The width of one cell, measured in the console's own font.
    pub cell_width: Signal<f64>,
    /// The height of a line measured in the console's own font.
    pub cell_height: Signal<f64>,
    /// The focused window's console's terminal, in cells.
    pub console_size: Signal<Size>,
    /// How large the focused window's console is drawn.
    pub console_box: Signal<(f64, f64)>,
    pub composer_height: Signal<f64>,
    pub prompt_width: Signal<f64>,
    /// How large each window of the content that is not focused is drawn,
    /// whose program is sized to it.
    pub win_boxes: Signal<HashMap<WindowId, (f64, f64)>>,
    /// The images the consoles drew.
    pub gallery: Signal<Gallery>,
    /// The latest action for a viewer, with the window whose viewer it is
    /// for.
    pub viewer_command: Signal<Option<(WindowId, viewers::Command)>>,
    /// The filter's field, which takes the keys while it is typed into.
    pub filter_field: Signal<Option<Rc<MountedData>>>,
}

/// The window's own handles, made once for the component that calls it,
/// the height of a line given to the components below it.
pub(crate) fn use_local() -> Local {
    let local = Local {
        frame: use_signal(|| 0),
        attached: use_signal(|| 0),
        cell_width: use_signal(|| 7.8),
        cell_height: use_signal(|| DEFAULT_CELL_HEIGHT),
        console_size: use_signal(|| Size::new(100, 30)),
        console_box: use_signal(|| (0.0, 0.0)),
        composer_height: use_signal(|| 0.0),
        prompt_width: use_signal(|| 0.0),
        win_boxes: use_signal(HashMap::new),
        gallery: use_signal(Gallery::default),
        viewer_command: use_signal(|| None),
        filter_field: use_signal(|| None),
    };
    use_context_provider(|| CellHeight(local.cell_height));
    local
}

impl Local {
    /// Give the filter's field the keys, or take them from it.
    pub(crate) fn focus_filter(&self, on: bool) {
        if let Some(field) = self.filter_field.peek().clone() {
            spawn(async move {
                let _ = field.set_focus(on).await;
            });
        }
    }
}
