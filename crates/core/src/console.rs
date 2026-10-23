//! The console as the window holds it: where its keys go, whether they pass
//! on to the program or are read in the reader, and asking the session shown
//! for what was typed into its own input.

pub mod keys;
pub mod reader;
pub mod route;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::time::Duration;

use adroit_model::Key;
use adroit_protocol::Listed;

use crate::actions::Action;
use crate::content::{Area, Tab};
use crate::effect::Effect;
use crate::keymap::{Context, Step};
use crate::msg::Msg;
use crate::overlay;
use crate::state::ViewState;
use crate::tree::{self, nav};
use crate::update::step;
use reader::{Edge, Exit};
use route::Typed;

/// How long a session is given to ask for its input after `Ctrl-g`. The
/// answer clears it, so it only has to outlast a slow start: a request
/// arriving after it is taken for the user's own and opens their editor.
pub const PULL_WAIT: Duration = Duration::from_secs(10);

/// How long the session is left to put its input line back once keys stop
/// going to it as they are, before it is asked for what was typed there.
pub const PULL_AFTER_FREE: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConsoleView {
    /// The keys pressed so far of a binding not complete yet.
    pub pending: Vec<Key>,
    /// Whether the next key goes to the session as it is.
    pub send_next: bool,
    /// Whether the console passes every key on to the session it shows.
    pub direct: bool,
    /// Whether the console is read in its reader.
    pub reading: bool,
    /// The key the reader took last, for the second key of `gg`.
    pub read_last: Option<Key>,
    /// Whether the session shown shows a dialog of its agent's own, which,
    /// as a choice it waits on, takes every key.
    pub dialog: bool,
    /// The sessions whose input may hold text since it was last taken: keys
    /// went to them as they are, or they were just attached.
    pub stale: HashSet<Tab>,
    /// The session asked to hand its input over through the external editor.
    pub pull: Option<Tab>,
    /// Counts the requests, so one lapsing does not take a later one with it.
    pulls: u64,
    /// Whether the prompt's Neovim runs, which takes what is handed over.
    pub prompt_runs: bool,
    /// Whether this window answers the editor's requests, and so is handed
    /// a session's input.
    pub answers: bool,
    /// Where the keys were and what was shown when last looked at.
    seen: Option<(Area, Option<Tab>)>,
    /// What was shown when the reading was last looked at.
    read_for: Option<Option<Tab>>,
    /// The listing the sessions' inputs were last kept by.
    listed_seen: Option<Listed>,
    /// Whether keys went to the session as they are when last looked at.
    free: bool,
}

/// What the window knows of its programs when a key is pressed, which the
/// view state does not hold.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Probe {
    /// The focused window's program has a pane here.
    pub pane: bool,
    /// That program scrolls what it shows itself, as it takes the mouse.
    pub scrolls_itself: bool,
    /// The console's reader, while it runs.
    pub reader: Option<ReaderAt>,
    /// The prompt's Neovim is inserting.
    pub inserting: bool,
}

/// Where the console's reader is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReaderAt {
    /// It is in its normal mode.
    pub normal: bool,
    pub edge: Edge,
}

/// Whether the session the console shows waits on a choice, which every key
/// answers.
pub fn waits(state: &ViewState) -> bool {
    match state.content.active() {
        Some(Tab::Session(id)) => {
            state.console.dialog
                || nav::find(tree::listed(state.host.work.as_ref()), id)
                    .is_some_and(|(_, s)| s.state.takes_every_key())
        }
        _ => false,
    }
}

/// Where text typed or pasted now goes. A dialog open types in its own
/// field, or takes no text.
pub fn text_goes(state: &ViewState) -> Typed {
    if overlay::takes_text_away(&overlay::holding(
        &state.overlays,
        &state.picker,
        &state.chrome,
    )) {
        return Typed::Nowhere;
    }
    let session = matches!(state.content.active(), Some(Tab::Session(_)));
    route::typed_into(
        state.content.focus,
        session,
        state.console.direct,
        waits(state),
    )
}

/// A key goes to the session shown as it is: it may type into its input.
fn typed(state: &mut ViewState) {
    if let Some(tab @ Tab::Session(_)) = state.content.active() {
        state.console.stale.insert(tab.clone());
    }
}

/// A key pressed, where the keys are now.
pub(crate) fn press(state: &mut ViewState, key: Key, probe: Probe) -> Vec<Effect> {
    if overlay::holder(state).is_some() {
        return overlay::press(state, key);
    }
    if let Some(effects) = crate::prompt::image_key(state, &key) {
        return effects;
    }
    let area = state.content.focus;
    let pane = probe.pane && area == Area::Console;
    let console = &mut state.console;
    // A Ctrl-g among keys sent as they are is the user's own: a request of
    // adroit's still waiting would take its answer.
    if console.direct || console.send_next {
        console.pull = None;
    }
    // Passing every key on holds the keyboard, Ctrl-q included, until
    // Escape.
    if console.direct && !route::app_key(&key) {
        if pane {
            return match keys::direct_key(&key) {
                Some(k) => {
                    typed(state);
                    vec![Effect::PaneKey(k)]
                }
                None => {
                    state.console.direct = false;
                    Vec::new()
                }
            };
        }
        console.direct = false;
    }
    if console.send_next && !route::app_key(&key) {
        console.send_next = false;
        if !pane {
            return Vec::new();
        }
        typed(state);
        return vec![Effect::PaneKey(key)];
    }
    // Reading, the keys are the reader's, but for those that begin a
    // global binding and those that leave it from its normal mode: Escape,
    // q and G for the live console, i, a and o for the prompt. A key taken
    // as global is only global: the console's binding of it is for the live
    // console.
    let mut windowing = false;
    if area == Area::Console
        && state.console.reading
        && let Some(reader) = probe.reader
    {
        let console = &state.console;
        windowing = !console.pending.is_empty()
            || route::app_key(&key)
            || state.chrome.keymap.resolve(&[Context::Global], &[], &key) != Step::Nothing;
        if !windowing {
            match reader::exit(&key, reader.normal) {
                Some(Exit::Live) => return leave_reading(&mut state.console),
                Some(Exit::Prompt) => {}
                None => return read(&mut state.console, key, reader, probe.scrolls_itself),
            }
        }
    }
    let session = matches!(state.content.active(), Some(Tab::Session(_)));
    // A session waiting on a choice takes every key but the window's own as
    // its answer; a shell takes every key but those.
    let pane = pane || (probe.pane && area == Area::Prompt);
    let passing = pane && route::passes(area, session, waits(state));
    let contexts = route::contexts(
        area,
        state.content.active(),
        passing,
        windowing,
        probe.inserting,
    );
    let seq = std::mem::take(&mut state.console.pending);
    let action = match state.chrome.keymap.resolve(&contexts, &seq, &key) {
        Step::Wait => {
            state.console.pending = seq.into_iter().chain(Some(key)).collect();
            return Vec::new();
        }
        Step::Run(action) => action,
        Step::Nothing => {
            // In a session's console, a key adroit does not bind is a Vim
            // key: it starts reading the console with it, and with a
            // sequence it broke off, such as `g` before `g`.
            if route::starts_reading(area, session, passing, state.console.reading) && pane {
                return vec![Effect::ReadConsole(
                    seq.into_iter().chain(Some(key)).collect(),
                )];
            }
            // A key that ends no binding is the area's to take, unless it
            // broke off a sequence.
            if !seq.is_empty() {
                return Vec::new();
            }
            return match area {
                _ if passing => {
                    typed(state);
                    vec![Effect::PaneKey(key)]
                }
                Area::Prompt => vec![Effect::PromptKey(key)],
                _ => Vec::new(),
            };
        }
    };
    if action == Action::KeysHelp {
        return crate::chrome::open_keys_help(&mut state.chrome, contexts);
    }
    vec![Effect::PerformKey(action, key)]
}

/// A key the reader takes. A program that scrolls what it shows itself, as
/// Claude Code on the alternate screen does, leaves nothing scrolled off to
/// read: moving past the reader's first or last line scrolls the program,
/// and the reader follows what it then shows. `gg` scrolls it to its start,
/// and moves there too.
fn read(console: &mut ConsoleView, key: Key, at: ReaderAt, scrolls_itself: bool) -> Vec<Effect> {
    let previous = console.read_last.replace(key.clone());
    let program = at.normal && scrolls_itself;
    if program && let Some(to) = reader::to_start(&key, previous.as_ref()) {
        return vec![Effect::PaneScrollKey(to), Effect::ReaderKey(key)];
    }
    if program && let Some(by) = reader::past_edge(&key, at.edge) {
        return vec![Effect::PaneWheel(by)];
    }
    vec![Effect::ReaderKey(key)]
}

/// The console no longer read. A session left scrolled back would show its
/// jump-to-bottom line live, moving with its layout: it is returned to its
/// latest output.
fn leave_reading(console: &mut ConsoleView) -> Vec<Effect> {
    console.reading = false;
    console.read_last = None;
    vec![Effect::ConsoleLive]
}

/// Text typed, or pasted, where the keys are now; it breaks off a sequence
/// of keys begun.
pub(crate) fn text(state: &mut ViewState, text: String, pasted: bool) -> Vec<Effect> {
    state.console.pending.clear();
    match text_goes(state) {
        Typed::Prompt => {
            let mut effects = step(state, Msg::Focus(Area::Prompt));
            effects.push(Effect::PromptText { text, pasted });
            effects
        }
        Typed::Program => vec![Effect::PaneText { text, pasted }],
        Typed::Nowhere => Vec::new(),
    }
}

/// An image pasted: kept for the prompt, or handed to the console's program
/// as an empty paste, as a terminal hands on an image paste, for it to read
/// the image off the clipboard itself.
pub(crate) fn image(state: &mut ViewState, mime: String, data: String) -> Vec<Effect> {
    state.console.pending.clear();
    match text_goes(state) {
        Typed::Prompt => vec![Effect::KeepImage { mime, data }],
        Typed::Program => vec![Effect::PaneText {
            text: String::new(),
            pasted: true,
        }],
        Typed::Nowhere => Vec::new(),
    }
}

/// The keys came into the prompt from elsewhere: the console stops being
/// read, and the session there is asked for what was typed into it.
pub(crate) fn prompt_entered(state: &mut ViewState) -> Vec<Effect> {
    let mut effects = leave_reading(&mut state.console);
    effects.extend(ask_input(state));
    effects
}

/// Ask the session shown to hand its input over, when it may hold text:
/// `Ctrl-g` opens its external editor, which is `adroit editor`. A session
/// whose editor is not adroit's would open the user's own editor in the
/// pane, which then waits on keys nobody sends; one waiting on an answer
/// never calls it, so the request would lapse.
pub(crate) fn ask_input(state: &ViewState) -> Vec<Effect> {
    let Some(tab @ Tab::Session(id)) = state.content.active() else {
        return Vec::new();
    };
    let console = &state.console;
    let hands_back =
        nav::find(tree::listed(state.host.work.as_ref()), id).is_some_and(|(_, s)| s.adroit_editor);
    if waits(state)
        || console.direct
        || !console.stale.contains(tab)
        || !console.prompt_runs
        || !console.answers
        || !hands_back
    {
        return Vec::new();
    }
    vec![Effect::AskInput(tab.clone())]
}

/// The session in `tab` was asked for its input.
pub(crate) fn input_asked(console: &mut ConsoleView, tab: Tab) -> Vec<Effect> {
    console.stale.remove(&tab);
    console.pulls += 1;
    console.pull = Some(tab);
    vec![Effect::LapsePull {
        pull: console.pulls,
        after: PULL_WAIT,
    }]
}

/// The request numbered `pull` lapsed, unless a later one was made since.
pub(crate) fn pull_lapsed(console: &mut ConsoleView, pull: u64) -> Vec<Effect> {
    if console.pulls == pull {
        console.pull = None;
    }
    Vec::new()
}

pub(crate) fn update(state: &mut ViewState, msg: Msg) -> Vec<Effect> {
    match msg {
        Msg::Key { key, probe } => press(state, key, *probe),
        Msg::Text { text, pasted } => self::text(state, text, pasted),
        Msg::ImagePasted { mime, data } => image(state, mime, data),
        Msg::ConsoleTyped => {
            typed(state);
            Vec::new()
        }
        Msg::AskInput => ask_input(state),
        msg => set(&mut state.console, msg),
    }
}

/// What the console is told of itself.
fn set(console: &mut ConsoleView, msg: Msg) -> Vec<Effect> {
    match msg {
        Msg::InputMayHold(tab) => {
            if matches!(tab, Tab::Session(_)) {
                console.stale.insert(tab);
            }
        }
        Msg::SequenceDropped => console.pending.clear(),
        Msg::ConsoleSendNext => console.send_next = true,
        Msg::ConsolePassEveryKey => console.direct = true,
        Msg::ReadingStarted => console.reading = true,
        Msg::Dialog(open) => console.dialog = open,
        Msg::PromptRuns(runs) => console.prompt_runs = runs,
        Msg::EditorAnswers => console.answers = true,
        Msg::InputAsked(tab) => return input_asked(console, tab),
        Msg::PullLapsed(pull) => return pull_lapsed(console, pull),
        Msg::InputTaken => console.pull = None,
        msg => unreachable!("not the console's: {msg:?}"),
    }
    Vec::new()
}

/// Keys meant for one area do not carry over to another: a sequence begun
/// in the sidebar would otherwise complete as a console binding after a
/// click there.
pub(crate) fn keep_to_area(state: &mut ViewState) {
    let now = (state.content.focus, state.content.active().cloned());
    let console = &mut state.console;
    if console.seen.as_ref() == Some(&now) {
        return;
    }
    if now.0 != Area::Console {
        console.direct = false;
    }
    console.pending.clear();
    console.send_next = false;
    console.seen = Some(now);
}

/// A list of the sessions drops the inputs kept for those no longer listed.
pub(crate) fn keep_listed(state: &mut ViewState) {
    let Some(now) = state
        .host
        .work
        .as_ref()
        .and_then(|w| w.tree.listed.as_deref())
    else {
        return;
    };
    let console = &mut state.console;
    if console.listed_seen.as_ref() == Some(now) {
        return;
    }
    console.stale = nav::followed(&now.sessions, &console.stale);
    console.listed_seen = Some(now.clone());
}

/// Another tab shown is shown live.
pub(crate) fn live_when_shown(state: &mut ViewState) {
    let now = state.content.active().cloned();
    let console = &mut state.console;
    if console.read_for.as_ref() == Some(&now) {
        return;
    }
    console.read_for = Some(now);
    console.reading = false;
}

/// While every key went to the session, some may have landed in its input
/// rather than a choice; they are taken back once the keys stop going
/// there, after the session puts its input line back.
pub(crate) fn ask_when_freed(state: &mut ViewState) -> Vec<Effect> {
    let free = answering_or_passing(state);
    let console = &mut state.console;
    let was = std::mem::replace(&mut console.free, free);
    if was && !free {
        return vec![Effect::AskInputAfter(PULL_AFTER_FREE)];
    }
    Vec::new()
}

/// Whether keys go to the session shown as they are: it waits on a choice,
/// shows a dialog, or the console passes every key on.
fn answering_or_passing(state: &ViewState) -> bool {
    let waiting = match state.content.active() {
        Some(Tab::Session(id)) => nav::find(tree::listed(state.host.work.as_ref()), id)
            .is_some_and(|(_, s)| s.state.takes_every_key()),
        _ => false,
    };
    waiting || state.console.dialog || state.console.direct
}
