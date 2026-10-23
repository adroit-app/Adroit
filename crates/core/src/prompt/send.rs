//! Sending what is written: taken from the prompt, composed by the host for
//! the agent it goes to, refused with a word to the user when it cannot go
//! where it was written for, and put back when it could not be sent.

use std::path::PathBuf;
use std::time::Duration;

use adroit_model::Agent;
use adroit_protocol::{Request, Typing};

use crate::chrome::status_line;
use crate::content::{Area, DraftOptions, Tab, fresh_name, label_from, named, shows_prompt};
use crate::effect::Effect;
use crate::msg::Msg;
use crate::state::ViewState;
use crate::tree::{self, nav};
use crate::update::step;

/// How long the host is given to compose a prompt before it is put back.
pub const COMPOSE_WAIT: Duration = Duration::from_secs(5);

/// A prompt on its way.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Sending {
    /// Taken from the prompt, which has not answered yet.
    Taking,
    /// Taken, and asked of the host how it is typed, by the request
    /// numbered `asked`.
    Composing { asked: u64, taken: Taken },
    /// Handed to the window to send, and put back should it fail.
    Handed(Taken),
}

/// What was taken from the prompt, as the window found it and the host
/// composed it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Taken {
    /// The tab it was written for.
    pub tab: Option<Tab>,
    pub text: String,
    pub images: Vec<PathBuf>,
    /// The agent it goes to.
    pub agent: Agent,
    /// The agent runs it as a shell command, as the host says.
    pub shell: bool,
    /// An image attached is no longer there, as the host says.
    pub image_missing: bool,
    /// How it is typed into its session, as the host composed it.
    pub typing: Vec<Typing>,
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

/// What was taken goes where it was written for once the host has composed
/// it. One for a session waiting on an answer is put back at once, with a
/// word to the user why: the Enter that ends a prompt would pick the choice
/// in front, usually Yes. The prompt waits for it to be answered, and the
/// keys go where the answer is given.
pub(crate) fn taken(state: &mut ViewState, taken: Taken) -> Vec<Effect> {
    let waits = match &taken.tab {
        Some(Tab::Session(id)) => nav::find(tree::listed(state.host.work.as_ref()), id)
            .is_some_and(|(_, s)| s.state.takes_every_key()),
        _ => false,
    };
    if waits {
        let mut effects = step(state, Msg::Focus(Area::Console));
        effects.extend(refuse(
            state,
            "Answer the session above first; your message waits here",
            taken,
        ));
        return effects;
    }
    state.prompt.asked += 1;
    let asked = state.prompt.asked;
    let request = Request::Compose {
        asked,
        agent: taken.agent,
        text: taken.text.clone(),
        images: taken.images.clone(),
    };
    state.prompt.sending = Some(Sending::Composing { asked, taken });
    vec![
        Effect::Host(request),
        Effect::LapseCompose {
            asked,
            after: COMPOSE_WAIT,
        },
    ]
}

/// The host composed the prompt on its way: it goes where it was written
/// for, a session's input or a new session from its draft. What cannot go
/// is put back, with a word to the user why.
pub(crate) fn composed(state: &mut ViewState) -> Vec<Effect> {
    let Some(Sending::Composing { asked, .. }) = &state.prompt.sending else {
        return Vec::new();
    };
    let Some(composed) = state
        .host
        .work
        .as_ref()
        .and_then(|w| w.prompt.composed.as_ref())
        .filter(|c| c.asked == *asked)
        .cloned()
    else {
        return Vec::new();
    };
    let Some(Sending::Composing { taken, .. }) = state.prompt.sending.take() else {
        return Vec::new();
    };
    go(
        state,
        Taken {
            shell: composed.shell,
            image_missing: composed.image_missing,
            typing: composed.typing,
            ..taken
        },
    )
}

/// The host has not composed the prompt numbered `asked`, if it is still on
/// its way: it is put back, with a word why.
pub(crate) fn lapsed(state: &mut ViewState, asked: u64) -> Vec<Effect> {
    match state.prompt.sending.take() {
        Some(Sending::Composing { asked: was, taken }) if was == asked => refuse(
            state,
            "The work host did not answer; your message waits here",
            taken,
        ),
        sending => {
            state.prompt.sending = sending;
            Vec::new()
        }
    }
}

/// `notice` said, and `taken` put back.
fn refuse(state: &mut ViewState, notice: &str, taken: Taken) -> Vec<Effect> {
    state.prompt.sending = None;
    let mut effects = status_line::notice(&mut state.chrome.status, notice.into());
    effects.push(restore(taken));
    effects
}

fn go(state: &mut ViewState, taken: Taken) -> Vec<Effect> {
    let has_images = !taken.images.is_empty();
    match taken.tab.clone() {
        _ if has_images && taken.shell => {
            refuse(state, "Images cannot be sent with a shell command", taken)
        }
        _ if taken.image_missing => refuse(
            state,
            "An attached image is unavailable; remove it or paste it again",
            taken,
        ),
        Some(Tab::Session(_)) => {
            state.prompt.sending = Some(Sending::Handed(taken.clone()));
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
        Some(Tab::Draft { .. }) if taken.shell => refuse(
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
            state.prompt.sending = Some(Sending::Handed(taken.clone()));
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
    let Some(Sending::Handed(taken)) = state.prompt.sending.take() else {
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
