//! How a message changes the window's state, and what it asks done.

use adroit_protocol::Request;

use crate::effect::Effect;
use crate::msg::Msg;
use crate::settle::settle;
use crate::state::ViewState;
use crate::{chrome, details, host};

/// `msg` applied to `state`, and the state settled after it.
pub fn step(state: &mut ViewState, msg: Msg) -> Vec<Effect> {
    let mut effects = update(state, msg);
    effects.extend(settle(state));
    effects
}

/// `msg` applied to the slices it concerns.
pub fn update(state: &mut ViewState, msg: Msg) -> Vec<Effect> {
    match msg {
        Msg::Opened { zoom } => {
            let mut effects = chrome::open(&mut state.chrome, zoom);
            effects.push(host::hello());
            effects
        }
        Msg::Zoom(step) => chrome::step(&mut state.chrome, step),
        Msg::ZoomSetting(zoom) => chrome::setting(&mut state.chrome, zoom),
        Msg::Host(event) => host::receive(&mut state.host, event),

        Msg::DetailsShown(shown) => details::show(&mut state.details, *shown),
        Msg::DetailsSheet(sheet) => details::sheet(&mut state.details, sheet),
        Msg::DetailsSheetStep(delta) => details::step_sheet(&mut state.details, delta),
        Msg::DetailsKey(key) => details::press(&mut state.details, state.host.work.as_ref(), key),
        Msg::DetailsClick(at) => details::click(&mut state.details, state.host.work.as_ref(), at),
        Msg::DetailsOpenRow(at) => {
            details::open_row(&mut state.details, state.host.work.as_ref(), at)
        }
        Msg::DetailsSelect(at) => details::select(&mut state.details, state.host.work.as_ref(), at),
        Msg::DetailsBack => details::back(&mut state.details),
        Msg::DetailsReadAgain => vec![Effect::Host(Request::ReadAgain)],
    }
}
