//! How a message changes the window's state, and what it asks done.

use adroit_protocol::Request;

use crate::chrome::{panels, status_line};
use crate::effect::Effect;
use crate::msg::Msg;
use crate::settle::settle;
use crate::state::ViewState;
use crate::{chrome, details, host, tree};

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

        Msg::PanelsKept(layout) => panels::kept(&mut state.chrome.panels, layout),
        Msg::BodyResized(body) => panels::resized(&mut state.chrome.panels, body),
        Msg::PanelToggle(side) => panels::toggle(&mut state.chrome.panels, side),
        Msg::PanelShow(side) => panels::show(&mut state.chrome.panels, side),
        Msg::PanelReset(side) => panels::reset(&mut state.chrome.panels, side),
        Msg::PanelDragStart { side, x } => panels::take(&mut state.chrome.panels, side, x),
        Msg::PanelDragMove { x } => panels::drag(&mut state.chrome.panels, x),
        Msg::PanelDragEnd => panels::drop(&mut state.chrome.panels),
        Msg::Status(line) => status_line::set(&mut state.chrome.status, line),
        Msg::Notice(text) => status_line::notice(&mut state.chrome.status, text),
        Msg::NoticeLapsed { notice, text } => {
            status_line::lapsed(&mut state.chrome.status, notice, &text)
        }
        Msg::Failing { last, now } => {
            status_line::failing(&mut state.chrome.status, last.as_deref(), now.as_deref())
        }
        Msg::KeysHelpOpen(here) => chrome::open_keys_help(&mut state.chrome, here),
        Msg::KeysHelpClose => chrome::close_keys_help(&mut state.chrome),

        Msg::Host(event) => host::receive(&mut state.host, event),

        Msg::Relist => vec![Effect::Host(Request::Relist)],
        Msg::Typed(inputs) => vec![Effect::Host(Request::Typed { inputs })],
        Msg::TerminalOpened(terminal) => vec![Effect::Host(Request::TerminalOpened { terminal })],
        Msg::TerminalClosed(id) => vec![Effect::Host(Request::TerminalClosed { id })],

        Msg::Showing(showing) => tree::showing(&mut state.tree, *showing),
        Msg::TreeSelect(row) => tree::select(&mut state.tree, row),
        Msg::TreeKey(key) => tree::press(&mut state.tree, key),
        Msg::TreeFold { row, open } => tree::fold(&mut state.tree, &row, open),
        Msg::TreeFoldToggle(row) => tree::toggle(&mut state.tree, &row),
        Msg::TreeFilter(text) => tree::filter(&mut state.tree, text),
        Msg::TreeFiltering(on) => tree::filtering(&mut state.tree, on),
        Msg::TreeEveryWorktree => tree::every_worktree(&mut state.tree),

        Msg::DetailsShown(told) => details::tell(&mut state.details, *told),
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
