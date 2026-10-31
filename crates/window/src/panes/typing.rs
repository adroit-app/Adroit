//! What is typed into the panes: a prompt taken from the prompt and sent to
//! its session, what the host composes, Tab for the prompt a session
//! suggests, and Ctrl-g, which asks a session to hand its input over.

use std::time::Duration;

use adroit_connect::journal::{PromptContext, PromptTarget};
use adroit_core::console::reader;
use adroit_core::console::route::passes_enter;
use adroit_core::content::Tab;
use adroit_core::msg::Msg;
use adroit_core::prompt::send::Taken;
use adroit_core::prompt::tab::{PromptTab, TabGoes};
use adroit_model::Key;
use adroit_nvim::notation;
use adroit_protocol::Typing;
use adroit_term::{Pane, Screen};
use dioxus::prelude::*;
use tokio::sync::oneshot;

use super::{ATTACH_WAIT_STEPS, pane_for};
use crate::prompts::Op;
use crate::shell::Ui;

/// How long a session is given to repaint after Tab before its input is
/// asked for anyway.
const TAB_APPLIED_WAIT: Duration = Duration::from_millis(500);

/// Wait until `pane` shows something other than `before`, or
/// [`TAB_APPLIED_WAIT`] passes.
async fn repainted(pane: &Pane, before: &Screen) {
    let until = tokio::time::Instant::now() + TAB_APPLIED_WAIT;
    while tokio::time::Instant::now() < until && pane.screen() == *before {
        tokio::time::sleep(Duration::from_millis(16)).await;
    }
}

/// Type `typing` into `pane`, step by step.
async fn type_into(pane: &Pane, typing: &[Typing]) {
    for step in typing {
        match step {
            Typing::Text { text } => pane.type_text(text),
            Typing::Paste { text } => pane.paste(text),
            Typing::Key { name } => {
                pane.key(&Key::new(name));
            }
            Typing::Pause { ms } => tokio::time::sleep(Duration::from_millis(*ms)).await,
        }
    }
}

/// Where a prompt sent through a remote connection goes.
#[derive(Debug, PartialEq, Eq)]
enum RemoteSend {
    /// To the session or draft shown.
    Here,
    /// Nothing is open: to a scratch draft made for it.
    Scratch,
    /// Nowhere; it stays in the prompt.
    Kept,
}

fn remote_send(shown: Option<&Tab>) -> RemoteSend {
    match shown {
        Some(Tab::Session(_, _) | Tab::Draft { .. }) => RemoteSend::Here,
        None => RemoteSend::Scratch,
        Some(_) => RemoteSend::Kept,
    }
}

impl Ui {
    /// Take what is written from the prompt to send. Taken now, so what is
    /// typed while it is sent stays for the next prompt; it goes to the tab
    /// it was written for, though another be shown by now.
    pub(crate) fn take_prompt(&self) {
        let (app, ui) = (self.app, self.clone());
        spawn(async move {
            if let Some(status) = app.remote_status_of(&app.active_environment()) {
                let refused = if !status.writable || !*app.drafts_ready.peek() {
                    Some("Wait until the work machine is connected to send the prompt")
                } else {
                    match remote_send(app.active().as_ref()) {
                        RemoteSend::Here => None,
                        RemoteSend::Scratch => ui
                            .move_to_scratch()
                            .await
                            .err()
                            .map(|_| "The prompt could not be moved into a scratch session; it stays here"),
                        RemoteSend::Kept => {
                            Some("Open a session or draft before sending a remote prompt")
                        }
                    }
                };
                if let Some(why) = refused {
                    app.status(Some(why.into()));
                    app.dispatch(Msg::PromptRetained);
                    return;
                }
                let nv = ui.consoles.nvim.peek().clone();
                if let Some(nv) = nv {
                    let _ = nv.freeze_prompt(true).await;
                }
            }
            let (give, given) = oneshot::channel();
            let taken = match ui.consoles.prompts.send(Op::Take(give)) {
                Ok(()) => given.await.ok(),
                Err(_) => None,
            };
            // Sent empty, the key itself goes to the session, as it would
            // in Claude Code's own input.
            if let Some(id) = taken
                .as_ref()
                .filter(|(_, _, images)| images.is_empty())
                .and_then(|(tab, text, _)| passes_enter(tab.as_ref(), text))
            {
                let tab = Tab::Session(app.environment(), id);
                let size = ui.local.console_size_for(&tab);
                if let Some(p) = pane_for(app, &ui.hub, &tab, size).await {
                    p.key(&Key::new("Enter").ctrl());
                }
            }
            let Some((shown, text, images)) =
                taken.filter(|(_, text, images)| !text.trim().is_empty() || !images.is_empty())
            else {
                app.dispatch(Msg::PromptEmpty);
                return;
            };
            let nv = ui.consoles.nvim.peek().clone();
            if let Some(nv) = nv {
                let _ = nv.stop_writing().await;
            }
            // Sent with nothing open, a prompt starts a scratch session, as
            // a draft there would.
            let tab = match shown {
                None => {
                    let scratch = app.config.peek().scratch.clone();
                    app.dispatch(Msg::Draft {
                        worktree: scratch.clone(),
                        write: false,
                    });
                    app.active()
                        .unwrap_or_else(|| Tab::draft(app.environment(), scratch))
                }
                Some(shown) => shown,
            };
            let agent = app.agent_of(Some(&tab));
            let taken = Taken {
                tab: Some(tab),
                text,
                images,
                agent,
                ..Default::default()
            };
            app.dispatch(Msg::PromptTaken(Box::new(taken)));
        });
    }

    /// Move what is written with nothing open into a scratch draft opened
    /// for it, so a remote prompt is journalled against the draft its
    /// session starts from, as one written in a draft is.
    async fn move_to_scratch(&self) -> Result<(), ()> {
        let (app, prompts) = (self.app, &self.consoles.prompts);
        let (give, given) = oneshot::channel();
        prompts.send(Op::Take(give)).map_err(drop)?;
        let (shown, text, images) = given.await.map_err(drop)?;
        // A tab opened since the send began holds its own draft, not the
        // prompt written with nothing open.
        if shown.is_some() {
            return Err(());
        }
        if text.trim().is_empty() && images.is_empty() {
            return Ok(());
        }
        let scratch = app.config.peek().scratch.clone();
        app.dispatch(Msg::Draft {
            worktree: scratch.clone(),
            write: false,
        });
        let draft = app
            .active()
            .unwrap_or_else(|| Tab::draft(app.environment(), scratch));
        let written = |target| PromptContext {
            target,
            text: text.clone(),
            images: images.clone(),
        };
        let draft_target = match &draft {
            Tab::Draft { worktree, id, .. } => PromptTarget::Draft {
                worktree: worktree.clone(),
                id: *id,
            },
            _ => return Err(()),
        };
        prompts.send(Op::Show(Some(draft))).map_err(drop)?;
        let (give, given) = oneshot::channel();
        prompts
            .send(Op::Recover(written(draft_target), give))
            .map_err(drop)?;
        if !given.await.unwrap_or(false) {
            return Err(());
        }
        let (give, given) = oneshot::channel();
        prompts
            .send(Op::Consume(written(PromptTarget::Nothing), give))
            .map_err(drop)?;
        let _ = given.await;
        Ok(())
    }

    /// Send a prompt to the session it was written for, as the host composed
    /// it, once the session is ready for input.
    pub(crate) fn send_prompt(&self, taken: Taken) {
        let (app, ui) = (self.app, self.clone());
        spawn(async move {
            let environment = app.hosts.peek().environment_of(taken.tab.as_ref()).clone();
            if app.remote_status_of(&environment).is_some() {
                let Some(context) = crate::prompts::remote::context(&taken) else {
                    app.dispatch(Msg::PromptRetained);
                    return;
                };
                let Some(Tab::Session(_, session)) = taken.tab.clone() else {
                    app.dispatch(Msg::PromptRetained);
                    return;
                };
                let answer = app
                    .ask_prompt(
                        &environment,
                        adroit_protocol::Ask::SubmitPrompt {
                            session,
                            agent: taken.agent,
                            text: taken.text,
                            images: taken.images,
                        },
                        context.clone(),
                    )
                    .await;
                let sent = matches!(answer, adroit_protocol::Answer::Done);
                if (sent || matches!(answer, adroit_protocol::Answer::Failed { .. }))
                    && let Err(text) = ui.finish_remote_prompt(&context, sent).await
                {
                    app.status(Some(text));
                }
                if sent {
                    app.dispatch(Msg::PromptWent(true));
                } else {
                    app.status(Some(format!("The prompt is retained: {answer:?}")));
                    app.dispatch(Msg::PromptRetained);
                }
                return;
            }
            let Some(tab) = taken.tab.clone() else {
                app.dispatch(Msg::PromptWent(false));
                return;
            };
            let size = ui.local.console_size_for(&tab);
            let sent = match pane_for(app, &ui.hub, &tab, size).await {
                Some(p) => {
                    // Claude Code turns on bracketed paste once it is ready
                    // for input.
                    for _ in 0..ATTACH_WAIT_STEPS {
                        if p.bracketed_paste() || p.exited() {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    if p.exited() {
                        app.status(Some("The session ended before it took the prompt".into()));
                        false
                    } else {
                        type_into(&p, &taken.typing).await;
                        true
                    }
                }
                None => {
                    app.status(Some("This session cannot be reached from here".into()));
                    false
                }
            };
            app.dispatch(Msg::PromptWent(sent));
        });
    }

    /// Type what the host asks into the pane of the session `tab` shows,
    /// attaching to it if need be.
    pub(crate) fn type_into_tab(&self, tab: Tab, typing: Vec<Typing>) {
        let (app, ui) = (self.app, self.clone());
        spawn(async move {
            let size = ui.local.console_size_for(&tab);
            if let Some(p) = pane_for(app, &ui.hub, &tab, size).await {
                type_into(&p, &typing).await;
            }
        });
    }

    /// Tab pressed in the prompt as `key`: to Neovim, or to the session for
    /// the prompt it suggests once the prompt is found empty, and then the
    /// session's input back to the prompt when the session hands it over.
    pub(crate) fn take_suggestion(
        &self,
        key: Key,
        session: bool,
        hands_back: bool,
        no_images: bool,
        tab: Option<Tab>,
    ) {
        let app = self.app;
        let Some(nv) = self.consoles.nvim.peek().clone() else {
            return;
        };
        let pane = self.active_pane().filter(|_| session);
        spawn(async move {
            let empty = no_images && nv.text().await.is_ok_and(|t| t.trim().is_empty());
            let goes = PromptTab {
                session: pane.is_some(),
                empty,
                hands_back,
            }
            .goes();
            let (TabGoes::Session | TabGoes::SessionThenBack, Some(p)) = (&goes, pane) else {
                // Neovim's own key: the prompt's mapping asked for this, and
                // must not be asked again.
                if let Some(n) = notation(&key) {
                    let _ = nv.press(&n).await;
                }
                return;
            };
            // Tab may type into Claude Code's input: an accepted suggestion
            // lands there.
            if let Some(tab @ Tab::Session(_, _)) = tab {
                app.dispatch(Msg::InputMayHold(tab));
            }
            let before = p.screen();
            p.key(&key);
            if goes == TabGoes::SessionThenBack {
                // Claude Code repaints the input once it takes the
                // suggestion, from faint to plain, or shows a hint when it
                // has none. Keys reach it in order, but whether Ctrl-g sees
                // the text Tab put in rests on how it batches its updates;
                // the repaint does not.
                repainted(&p, &before).await;
                app.dispatch(Msg::AskInput);
            }
        });
    }

    /// Ask the session in this tab to hand its input over: Ctrl-g opens its
    /// external editor, which is `adroit editor`.
    pub(crate) fn ask_input(&self, tab: Tab) {
        let pane = self.hub.panes.borrow().get(&tab).cloned();
        if let Some(p) = pane.filter(|p| !p.exited()) {
            p.type_text("\x07");
            self.app.dispatch(Msg::InputAsked(tab));
        }
    }

    /// Show the console live again: a session left scrolled back would show
    /// its jump-to-bottom line live, moving with its layout.
    pub(crate) fn console_live(&self) {
        if let Some(p) = self.active_pane().filter(|p| p.takes_mouse()) {
            p.key(&reader::newest());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_model::EnvironmentId;

    #[test]
    fn a_remote_prompt_goes_to_the_session_or_draft_it_was_written_for() {
        assert_eq!(
            remote_send(Some(&Tab::Session(EnvironmentId::unbound(), "s".into()))),
            RemoteSend::Here
        );
        assert_eq!(
            remote_send(Some(&Tab::draft(
                EnvironmentId::unbound(),
                std::path::PathBuf::from("/w")
            ))),
            RemoteSend::Here
        );
    }

    #[test]
    fn a_remote_prompt_sent_with_nothing_open_starts_a_scratch_session() {
        assert_eq!(remote_send(None), RemoteSend::Scratch);
    }

    #[test]
    fn a_remote_prompt_is_kept_while_neither_a_session_nor_a_draft_is_shown() {
        assert_eq!(
            remote_send(Some(&Tab::Terminal(EnvironmentId::unbound(), 1))),
            RemoteSend::Kept
        );
    }
}
