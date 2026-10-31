//! How a message changes the window's state, and what it asks done.

use adroit_protocol::Request;

use crate::chrome::{panels, status_line};
use crate::content::Opening;
use crate::effect::Effect;
use crate::msg::Msg;
use crate::overlay::{self, Menu};
use crate::settle::settle;
use crate::state::ViewState;
use crate::{chrome, console, content, details, host, picker, prompt, tree};

/// `msg` applied to `state`, and the state settled after it.
pub fn step(state: &mut ViewState, msg: Msg) -> Vec<Effect> {
    let mut effects = update(state, msg);
    effects.extend(settle(state));
    content::keep_worked_in(&mut state.content, &effects);
    effects
}

/// `msg` applied to the slices it concerns.
pub fn update(state: &mut ViewState, msg: Msg) -> Vec<Effect> {
    match msg {
        Msg::Opened { home, environment } => {
            state.picker.home = home;
            state.hosts.open(environment.clone());
            vec![host::hello(environment)]
        }
        Msg::WindowFocused(on) => chrome::window_focused(&mut state.chrome, on),
        Msg::Zoom(step) => chrome::step(&mut state.chrome, step),

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

        Msg::Host { environment, event } => host::told(state, environment, event),

        Msg::Relist => vec![state.hosts.ask_own(Request::Relist)],
        Msg::Typed(inputs) => vec![state.hosts.ask_own(Request::Typed { inputs })],
        Msg::TerminalOpened(terminal) => {
            vec![state.hosts.ask_own(Request::TerminalOpened { terminal })]
        }
        Msg::TerminalClosed(id) => vec![state.hosts.ask_own(Request::TerminalClosed { id })],

        Msg::ContentOpened { kept, scratch } => content::opened(&mut state.content, kept, scratch),
        Msg::Focus(area) => content::focus(&mut state.content, area),
        Msg::Show { thing, place, how } => content::show(&mut state.content, thing, place, how),
        Msg::ShowSession(id) => content::show_session(state, id),
        Msg::Draft { worktree, write } => content::draft(
            &mut state.content,
            state.hosts.environment.clone(),
            worktree,
            write,
        ),
        Msg::OpenRow {
            item,
            how,
            take_keys,
        } => content::open_row(state, item, how, take_keys),
        Msg::EditFile(path) => content::edit_file(state, path),
        Msg::RowClicked(item) => content::open_row(state, item, Some(Opening::Existing), true),
        Msg::Window(action) => content::act(state, action),
        Msg::WindowFocus(id) => content::focus_window(state, id),
        Msg::TabShow(at) => content::show_tab(state, at),
        Msg::TabReveal(tab) => content::reveal(state, &tab),
        Msg::Replace { from, to } => content::replace(&mut state.content, &from, to),
        Msg::ShellClosed(id) => {
            let mut effects = vec![state.hosts.ask_own(Request::TerminalClosed { id })];
            effects.extend(content::shell_closed(
                &mut state.content,
                &state.hosts.environment,
                state.hosts.own().work.as_ref(),
                id,
            ));
            effects
        }
        Msg::WindowDragStart { border, from, px } => {
            content::drag_start(&mut state.content, border, from, px)
        }
        Msg::WindowDragMove(pos) => content::drag_move(&mut state.content, pos),
        Msg::WindowDragEnd => content::drag_end(&mut state.content),
        Msg::OpenLogin { profile } => vec![Effect::OpenLogin { profile }],
        Msg::DraftProfile { tab, profile } => {
            let found = state
                .config
                .settings
                .find_profile(&profile)
                .map(std::borrow::Cow::into_owned);
            let Some(chosen) = found else {
                return status_line::notice(
                    &mut state.chrome.status,
                    format!(
                        "Profile `{profile}` is no longer defined; choose another in the draft or under Settings → Agents"
                    ),
                );
            };
            let from = content::agent_of(state, tab.as_ref());
            match content::choose_profile(&mut state.content, tab, &chosen, from) {
                Some(told) => status_line::notice(&mut state.chrome.status, told),
                None => Vec::new(),
            }
        }
        Msg::DraftOptions { tab, options } => {
            content::choose_options(&mut state.content, tab, options)
        }
        Msg::PanesLive(live) => content::tell_live(&mut state.content, live),

        Msg::TreeSelect(row) => tree::select(&mut state.tree, row),
        Msg::TreeKey(key) => tree::press(&mut state.tree, key),
        Msg::TreeFold { row, open } => tree::fold(&mut state.tree, &row, open),
        Msg::TreeFoldToggle(row) => tree::toggle(&mut state.tree, &row),
        Msg::TreeFilter(text) => tree::filter(&mut state.tree, text),
        Msg::TreeFiltering(on) => tree::filtering(&mut state.tree, on),
        Msg::TreeEveryWorktree => tree::every_worktree(&mut state.tree, &state.hosts),

        Msg::DetailsShown(told) => details::tell(&mut state.details, *told),
        Msg::DetailsSheet(sheet) => details::sheet(&mut state.details, sheet),
        Msg::DetailsSheetStep(delta) => details::step_sheet(&mut state.details, delta),
        Msg::DetailsKey(key) => {
            details::press(&mut state.details, state.hosts.own().work.as_ref(), key)
        }
        Msg::DetailsClick(at) => {
            details::click(&mut state.details, state.hosts.own().work.as_ref(), at)
        }
        Msg::DetailsOpenRow(at) => {
            details::open_row(&mut state.details, state.hosts.own().work.as_ref(), at)
        }
        Msg::DetailsSelect(at) => {
            details::select(&mut state.details, state.hosts.own().work.as_ref(), at)
        }
        Msg::DetailsBack => details::back(&mut state.details),
        Msg::DetailsReadAgain => vec![state.hosts.ask_own(Request::ReadAgain)],
        Msg::CommandsWanted => vec![state.hosts.ask_own(Request::CommandsWanted)],

        Msg::PickerOpen => picker::open(state),
        Msg::ResumeOpen => picker::open_resume(state),
        Msg::PickerClose => picker::close(&mut state.picker),
        Msg::PickerQuery(text) => picker::query(&mut state.picker, text),
        Msg::PickerKey(key) => picker::press(state, &key),
        Msg::PickerChoose(at) => picker::choose(state, at),
        Msg::PickerDir(dir) => picker::start_in(state, dir),
        Msg::PickerEvery => picker::every(&mut state.picker),

        msg @ (Msg::Key { .. }
        | Msg::Text { .. }
        | Msg::ImagePasted { .. }
        | Msg::FilesDropped(_)
        | Msg::ConsoleTyped
        | Msg::InputMayHold(_)
        | Msg::SequenceDropped
        | Msg::ConsoleSendNext
        | Msg::ConsolePassEveryKey
        | Msg::ReadingStarted
        | Msg::ReaderQuit
        | Msg::PromptRuns(_)
        | Msg::EditorAnswers
        | Msg::AskInput
        | Msg::InputAsked(_)
        | Msg::PullLapsed(_)
        | Msg::InputTaken) => console::update(state, msg),

        msg @ (Msg::Recall { .. }
        | Msg::PromptMode(..)
        | Msg::PromptShell(_)
        | Msg::PromptImages(_)
        | Msg::Drafted(_)
        | Msg::ImageSelect(_)
        | Msg::ImagesChoose
        | Msg::PromptSend
        | Msg::PromptEmpty
        | Msg::PromptTaken(_)
        | Msg::PromptWent(_)
        | Msg::PromptRetained
        | Msg::ComposeLapsed(_)) => prompt::update(state, msg),

        Msg::Perform { action, key, probe } => crate::perform::perform(state, action, key, &probe),

        Msg::HeldKey(key) => overlay::press(state, key),
        Msg::MenuOpen { x, y, target } => overlay::open_menu(state, Menu { x, y, target }),
        Msg::MenuClose => overlay::close_menu(state),
        Msg::MenuRun { target, action } => overlay::run_menu(state, target, action),
        Msg::Confirm(ask) => overlay::ask(state, *ask),
        Msg::ConfirmStop { selection } => overlay::ask_stop(state, selection),
        Msg::ConfirmAnswer(yes) => overlay::answer(state, yes),
        Msg::RenameStart { id, place } => overlay::start_rename(state, &id, place),
        Msg::RenameText { id, place, text } => overlay::type_name(state, &id, place, text),
        Msg::RenameEnd { id, place, commit } => overlay::end_rename(state, &id, place, commit),
        Msg::DaemonStanding(note) => overlay::daemon_standing(state, note),
        Msg::DaemonRestarted(note) => overlay::daemon_restarted(state, note),
        Msg::BannerClose => overlay::close_banner(state),
    }
}
