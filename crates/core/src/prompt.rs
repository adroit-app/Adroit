//! The prompt as the window holds it: what is kept aside for the tabs not
//! shown, where Tab pressed there goes, and stepping through the prompts
//! sent before, which the work host reads.

pub mod drafts;
pub mod images;
pub mod send;
pub mod tab;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::path::PathBuf;

use adroit_model::{Agent, Command, Key};
use adroit_protocol::{CommandsSight, Request};

use crate::complete::{Offer, candidates, commands_dir};
use crate::console::{self, route::Typed};
use crate::content::{Area, Tab};
use crate::effect::Effect;
use crate::msg::Msg;
use crate::state::ViewState;
use crate::tree::{self, nav};
use crate::update::step;
use images::ImageKey;

#[derive(Debug, Clone, PartialEq)]
pub struct PromptView {
    /// Neovim's mode, as the class the footer draws it with and its word.
    pub mode: (&'static str, &'static str),
    /// Whether what is written is a shell line for the session.
    pub shell: bool,
    /// The images attached to the prompt shown.
    pub images: Vec<PathBuf>,
    /// The image selected among them, which takes the keys.
    pub selected: Option<PathBuf>,
    /// The tabs with something written for them kept aside.
    pub drafted: HashSet<Tab>,
    /// The images attached when last looked at.
    images_seen: Vec<PathBuf>,
    /// What the prompt was last told to show; it starts out showing what
    /// is written for nothing open.
    shown_for: Option<Tab>,
    /// The prompt on its way, while one is.
    pub(crate) sending: Option<send::Sending>,
    /// A step through the prompts sent, waiting on the history the host
    /// reads for it.
    recall: Option<Recall>,
    /// Counts the histories asked for, so an answer is taken for its own
    /// request alone.
    asked: u64,
    /// What the running Neovim was last offered after `@`, and the
    /// worktree it walks paths from; nothing while none runs.
    offered: Option<(Vec<Offer>, Option<PathBuf>)>,
    /// What the running Neovim was last offered after `/`; nothing while
    /// none runs.
    commands: Option<Vec<Command>>,
}

impl Default for PromptView {
    fn default() -> Self {
        Self {
            mode: ("normal", "…"),
            shell: false,
            images: Vec::new(),
            selected: None,
            drafted: HashSet::new(),
            images_seen: Vec::new(),
            shown_for: None,
            sending: None,
            recall: None,
            asked: 0,
            offered: None,
            commands: None,
        }
    }
}

impl PromptView {
    pub fn is_sending(&self) -> bool {
        self.sending.is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Recall {
    asked: u64,
    /// The tab it was asked for, which the prompt must still show.
    tab: Option<Tab>,
    back: bool,
}

/// The project a tab's prompts were sent in: the directory its session
/// runs in, or its draft's worktree.
fn project(state: &ViewState, tab: Option<&Tab>) -> Option<PathBuf> {
    match tab {
        Some(Tab::Session(_, id)) => nav::find(tree::listed(state.host.work.as_ref()), id)
            .map(|(_, s)| PathBuf::from(&s.cwd)),
        Some(Tab::Draft { worktree, .. }) => Some(worktree.clone()),
        _ => None,
    }
}

/// Step back through the prompts `agent` kept as sent where the shown tab's
/// are, or forth: read afresh, as each prompt sent adds to them.
pub(crate) fn recall(state: &mut ViewState, back: bool, agent: Agent) -> Vec<Effect> {
    let tab = state.content.active().cloned();
    let Some(project) = project(state, tab.as_ref()) else {
        return vec![Effect::PromptRecall {
            tab,
            back,
            sent: Vec::new(),
        }];
    };
    let home = crate::content::home_of(state, tab.as_ref());
    let view = &mut state.prompt;
    view.asked += 1;
    view.recall = Some(Recall {
        asked: view.asked,
        tab,
        back,
    });
    vec![Effect::Host(Request::History {
        asked: view.asked,
        agent,
        home,
        project,
    })]
}

/// The history read for the step waiting on it puts the prompt it steps to
/// in the prompt.
pub(crate) fn recalled(state: &mut ViewState) -> Vec<Effect> {
    let Some(read) = state
        .host
        .work
        .as_ref()
        .and_then(|w| w.prompt.history.as_ref())
    else {
        return Vec::new();
    };
    let view = &mut state.prompt;
    if view.recall.as_ref().is_none_or(|r| r.asked != read.asked) {
        return Vec::new();
    }
    let Some(Recall { tab, back, .. }) = view.recall.take() else {
        return Vec::new();
    };
    vec![Effect::PromptRecall {
        tab,
        back,
        sent: read.sent.clone(),
    }]
}

/// What the prompt is told of itself.
pub(crate) fn update(state: &mut ViewState, msg: Msg) -> Vec<Effect> {
    match msg {
        Msg::Recall { back, agent } => recall(state, back, agent),
        Msg::PromptSend => send::send(state),
        Msg::PromptEmpty => send::nothing_taken(state),
        Msg::PromptTaken(taken) => send::taken(state, *taken),
        Msg::PromptWent(ok) => send::sent(state, ok),
        Msg::PromptRetained => send::retained(state),
        Msg::ComposeLapsed(asked) => send::lapsed(state, asked),
        Msg::ImageSelect(path) => {
            let effects = step(state, Msg::Focus(Area::Prompt));
            state.prompt.selected = path;
            effects
        }
        Msg::ImagesChoose => {
            if console::text_goes(state) != Typed::Prompt {
                return Vec::new();
            }
            let effects = step(state, Msg::Focus(Area::Prompt));
            state.prompt.selected = state.prompt.images.first().cloned();
            effects
        }
        msg => {
            set(&mut state.prompt, msg);
            Vec::new()
        }
    }
}

fn set(view: &mut PromptView, msg: Msg) {
    match msg {
        Msg::PromptMode(class, word) => view.mode = (class, word),
        Msg::PromptShell(on) => view.shell = on,
        Msg::PromptImages(images) => view.images = images,
        Msg::Drafted(tabs) => view.drafted = tabs,
        msg => unreachable!("not the prompt's: {msg:?}"),
    }
}

/// A key while an image is selected moves among the images or removes one;
/// any other key in the prompt leaves the images for the text.
pub(crate) fn image_key(state: &mut ViewState, key: &Key) -> Option<Vec<Effect>> {
    if state.content.focus != Area::Prompt || console::text_goes(state) != Typed::Prompt {
        return None;
    }
    let view = &mut state.prompt;
    match images::image_key(&view.images, view.selected.as_ref(), key) {
        Some(ImageKey::Select(path)) => {
            view.selected = path;
            Some(Vec::new())
        }
        Some(ImageKey::Remove(path)) => Some(vec![Effect::RemoveImage(path)]),
        None => {
            view.selected = None;
            None
        }
    }
}

/// Images attached or taken away leave none selected.
pub(crate) fn keep_selection(view: &mut PromptView) {
    if view.images_seen == view.images {
        return;
    }
    view.images_seen = view.images.clone();
    view.selected = None;
}

/// The prompt shows what is written for what the focused window shows.
pub(crate) fn follow_shown(state: &mut ViewState) -> Vec<Effect> {
    let now = state.content.active().cloned();
    let view = &mut state.prompt;
    if view.shown_for == now {
        return Vec::new();
    }
    view.shown_for = now.clone();
    vec![Effect::PromptShow(now)]
}

/// The prompt's Neovim, while it runs, is offered after `@` the sessions
/// and the files of the worktree in the middle, which it walks paths from,
/// each time they change.
pub(crate) fn offer(state: &mut ViewState) -> Vec<Effect> {
    if !state.console.prompt_runs {
        state.prompt.offered = None;
        return Vec::new();
    }
    let work = state.host.work.as_ref();
    let place = state.content.place.as_ref();
    let files = match (work, place) {
        (Some(work), Some(p)) => work
            .place
            .of(&p.worktree)
            .map(|r| r.files.as_slice())
            .unwrap_or_default(),
        _ => &[],
    };
    let offers = candidates(tree::listed(work), place, files);
    let root = place.map(|p| p.worktree.clone());
    let (was_offers, was_root) = match state.prompt.offered.take() {
        Some((offers, root)) => (Some(offers), Some(root)),
        None => (None, None),
    };
    let mut effects = Vec::new();
    if was_offers.as_ref() != Some(&offers) {
        effects.push(Effect::PromptOffer(offers.clone()));
    }
    if was_root.as_ref() != Some(&root) {
        effects.push(Effect::PromptRoot(root.clone()));
    }
    state.prompt.offered = Some((offers, root));
    effects
}

/// The agent and the directory whose commands `/` offers: those of what the
/// focused window shows, or of the worktree in the middle with nothing open.
/// None for a shell or a viewer.
pub(crate) fn commands_sight(state: &ViewState) -> Option<CommandsSight> {
    let tab = state.content.active();
    let tree = tree::listed(state.host.work.as_ref());
    let dir = commands_dir(tree, tab, state.content.place.as_ref())?;
    let agent = crate::content::agent_of(state, tab);
    let home = crate::content::home_of(state, tab);
    Some(CommandsSight { agent, home, dir })
}

/// The prompt's Neovim is offered after `/` what the host read for what is
/// shown, each time it changes; what was offered stays until the host has
/// read for something newly shown.
pub(crate) fn offer_commands(state: &mut ViewState) -> Vec<Effect> {
    if !state.console.prompt_runs {
        state.prompt.commands = None;
        return Vec::new();
    }
    let now = match commands_sight(state) {
        None => Some(Vec::new()),
        Some(sight) => state
            .host
            .work
            .as_ref()
            .and_then(|w| w.prompt.commands.as_ref())
            .filter(|read| read.is_for(&sight))
            .map(|read| read.commands.clone()),
    };
    match now {
        Some(now) if state.prompt.commands.as_ref() != Some(&now) => {
            state.prompt.commands = Some(now.clone());
            vec![Effect::PromptCommands(now)]
        }
        _ => Vec::new(),
    }
}
