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

use adroit_model::{Agent, Key};
use adroit_protocol::Request;

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
    /// The prompt on its way, while one is.
    pub(crate) sending: Option<send::Sending>,
    /// A step through the prompts sent, waiting on the history the host
    /// reads for it.
    recall: Option<Recall>,
    /// Counts the histories asked for, so an answer is taken for its own
    /// request alone.
    asked: u64,
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
            sending: None,
            recall: None,
            asked: 0,
        }
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
        Some(Tab::Session(id)) => nav::find(tree::listed(state.host.work.as_ref()), id)
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
