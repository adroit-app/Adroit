//! The prompt as the window holds it: what is kept aside for the tabs not
//! shown, where Tab pressed there goes, and stepping through the prompts
//! sent before, which the work host reads.

pub mod drafts;
pub mod tab;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use adroit_model::Agent;
use adroit_protocol::Request;

use crate::content::Tab;
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::{self, nav};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PromptView {
    /// A step through the prompts sent, waiting on the history the host
    /// reads for it.
    recall: Option<Recall>,
    /// Counts the histories asked for, so an answer is taken for its own
    /// request alone.
    asked: u64,
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
