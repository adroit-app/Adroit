//! Sending what is written: taken from the prompt, refused with a word to the
//! user when it cannot go where it was written for, and put back when it
//! could not be sent.

use std::path::PathBuf;

use adroit_model::Agent;

use crate::chrome::status_line;
use crate::content::{Area, DraftOptions, Tab, fresh_name, label_from, named, shows_prompt};
use crate::effect::Effect;
use crate::msg::Msg;
use crate::state::ViewState;
use crate::tree::{self, nav};
use crate::update::step;

/// A prompt on its way.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Sending {
    /// Taken from the prompt, which has not answered yet.
    Taking,
    /// Handed to the window to send, and put back should it fail.
    Sending(Taken),
}

/// What was taken from the prompt, as the window found it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Taken {
    /// The tab it was written for.
    pub tab: Option<Tab>,
    pub text: String,
    pub images: Vec<PathBuf>,
    /// The agent it goes to.
    pub agent: Agent,
    /// That agent runs a line starting with `!` as a shell command.
    pub shell_bang: bool,
    /// The text is such a line.
    pub shell_line: bool,
    /// An image attached is no longer there.
    pub image_missing: bool,
}

/// Send what is written, unless a prompt is on its way already or the
/// focused window shows no prompt.
pub(crate) fn send(state: &mut ViewState) -> Vec<Effect> {
    if state.prompt.sending.is_some() || !shows_prompt(state.content.active()) {
        return Vec::new();
    }
    state.prompt.sending = Some(Sending::Taking);
    vec![Effect::TakePrompt]
}

/// The prompt held nothing to send.
pub(crate) fn nothing_taken(state: &mut ViewState) -> Vec<Effect> {
    state.prompt.sending = None;
    Vec::new()
}

/// What was taken goes where it was written for: a session's input, or a
/// new session from its draft. What cannot go is put back, with a word to
/// the user why.
pub(crate) fn taken(state: &mut ViewState, taken: Taken) -> Vec<Effect> {
    let waiting = |state: &ViewState, id: &str| {
        nav::find(tree::listed(state.host.work.as_ref()), id)
            .is_some_and(|(_, s)| s.state.takes_every_key())
    };
    let refuse = |state: &mut ViewState, notice: &str, taken: Taken| {
        state.prompt.sending = None;
        let mut effects = status_line::notice(&mut state.chrome.status, notice.into());
        effects.push(restore(taken));
        effects
    };
    let has_images = !taken.images.is_empty();
    match taken.tab.clone() {
        // The Enter that ends a prompt would pick the choice in front,
        // usually Yes; the prompt waits for it to be answered, and the keys
        // go where the answer is given.
        Some(Tab::Session(id)) if waiting(state, &id) => {
            let mut effects = step(state, Msg::Focus(Area::Console));
            effects.extend(refuse(
                state,
                "Answer the session above first; your message waits here",
                taken,
            ));
            effects
        }
        _ if has_images && taken.shell_bang && taken.shell_line => {
            refuse(state, "Images cannot be sent with a shell command", taken)
        }
        _ if taken.image_missing => refuse(
            state,
            "An attached image is unavailable; remove it or paste it again",
            taken,
        ),
        Some(Tab::Session(_)) => {
            state.prompt.sending = Some(Sending::Sending(taken.clone()));
            vec![Effect::SendPrompt(Box::new(taken))]
        }
        Some(Tab::Draft { .. })
            if named(&taken.text).0.is_some()
                && named(&taken.text).1.trim().is_empty()
                && !has_images =>
        {
            refuse(
                state,
                "Write what the session is to do below its name",
                taken,
            )
        }
        // Given as the first prompt, a shell line is not known to run as a
        // command; it could reach the model as a message.
        Some(Tab::Draft { .. }) if taken.shell_bang && taken.shell_line => refuse(
            state,
            "A shell line runs in a session: start it with a message first",
            taken,
        ),
        Some(draft @ Tab::Draft { .. }) => {
            let (given, text) = named(&taken.text);
            let label = given.unwrap_or_else(|| label_from(&text));
            let listed = tree::listed(state.host.work.as_ref());
            let names: Vec<&str> = listed.sessions().map(|(_, s)| s.name.as_str()).collect();
            let name = fresh_name(&label, &names);
            let options = state
                .content
                .drafts
                .options
                .get(&draft)
                .cloned()
                .unwrap_or_default();
            state.prompt.sending = Some(Sending::Sending(taken.clone()));
            vec![Effect::StartDraft(Box::new(Start {
                draft,
                name,
                text,
                options,
                taken,
            }))]
        }
        Some(Tab::Terminal(_) | Tab::Viewer(_)) | None => {
            state.prompt.sending = None;
            vec![restore(taken)]
        }
    }
}

/// A new session to start from a draft with its first prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct Start {
    pub draft: Tab,
    /// Its name: the one given above the prompt, or one made from it.
    pub name: String,
    /// The prompt, without the name given above it.
    pub text: String,
    pub options: DraftOptions,
    pub taken: Taken,
}

/// The prompt handed to the window went, or did not: one that did not is
/// put back where it was written for.
pub(crate) fn sent(state: &mut ViewState, ok: bool) -> Vec<Effect> {
    let Some(Sending::Sending(taken)) = state.prompt.sending.take() else {
        return Vec::new();
    };
    if ok {
        vec![Effect::PromptSent]
    } else {
        vec![restore(taken)]
    }
}

fn restore(taken: Taken) -> Effect {
    Effect::PromptRestore {
        tab: taken.tab,
        text: taken.text,
        images: taken.images,
    }
}
