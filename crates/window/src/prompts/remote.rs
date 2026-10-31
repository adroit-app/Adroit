//! Retained prompt contexts outlive the UI task that initiated the send.

use std::collections::BTreeMap;

use super::Op;
use crate::link::RemoteStatus;
use crate::shell::Ui;
use adroit_connect::journal::{PromptContext, PromptTarget, Record};
use adroit_core::content::{Tab, kept_name};
use adroit_core::msg::Msg;
use adroit_core::prompt::send::Taken;
use adroit_model::EnvironmentId;
use adroit_protocol::{Answer, Ask, MutationState};
use dioxus::prelude::*;
use tokio::sync::oneshot;

pub(crate) fn context(taken: &Taken) -> Option<PromptContext> {
    let target = match taken.tab.as_ref()? {
        Tab::Session(_, id) => PromptTarget::Session { id: id.clone() },
        Tab::Draft { worktree, id, .. } => PromptTarget::Draft {
            worktree: worktree.clone(),
            id: *id,
        },
        _ => return None,
    };
    Some(PromptContext {
        target,
        text: taken.text.clone(),
        images: taken.images.clone(),
    })
}

/// The tab `context` was written in, of `environment`, the one the journal
/// that kept it is of.
pub(crate) fn target(context: &PromptContext, environment: &EnvironmentId) -> Option<Tab> {
    match &context.target {
        PromptTarget::Nothing => None,
        PromptTarget::Session { id } => Some(Tab::Session(environment.clone(), id.clone())),
        PromptTarget::Draft { worktree, id } => Some(Tab::Draft {
            environment: environment.clone(),
            worktree: worktree.clone(),
            id: *id,
        }),
    }
}

/// The change pending in `environment`'s journal that sent the prompt
/// `context`, as `remotes` tell how the window stands with each environment
/// on a work machine.
pub(crate) fn pending_prompt(
    remotes: &BTreeMap<EnvironmentId, RemoteStatus>,
    environment: &EnvironmentId,
    context: &PromptContext,
) -> Option<Record> {
    remotes
        .get(environment)?
        .pending
        .iter()
        .find(|r| r.intent.prompt.as_ref() == Some(context))
        .cloned()
}

pub(crate) fn confirmed(outcome: &Option<MutationState>) -> Option<bool> {
    match outcome.as_ref()? {
        MutationState::Result { answer } => match answer.as_ref() {
            Answer::Done | Answer::Session { .. } => Some(true),
            Answer::Failed { .. } => Some(false),
            _ => None,
        },
        MutationState::Rejected { .. } => Some(false),
        _ => None,
    }
}

impl Ui {
    /// See the prompt `context` sent in `environment` through, as it was
    /// `sent` or not.
    pub(crate) async fn finish_remote_prompt(
        &self,
        environment: &EnvironmentId,
        context: &PromptContext,
        sent: bool,
    ) -> Result<(), String> {
        let record = pending_prompt(&self.app.remote_statuses(), environment, context);
        let Some(record) = record else {
            return Ok(());
        };
        self.handle_remote_prompt(environment, record, sent, false, false)
            .await
    }

    /// Clear the draft of the prompt `record` in `environment`'s journal
    /// kept once it was `sent`, or else put it back, with the outcome
    /// `checked` by the user.
    pub(crate) async fn handle_remote_prompt(
        &self,
        environment: &EnvironmentId,
        record: Record,
        sent: bool,
        checked: bool,
        restore: bool,
    ) -> Result<(), String> {
        let context = record
            .intent
            .prompt
            .as_ref()
            .ok_or("the pending change has no draft")?;
        let tab = target(context, environment);
        if sent {
            if let Some(key) = tab.as_ref().and_then(kept_name) {
                match self
                    .app
                    .ask(Ask::ConsumeDraft {
                        key,
                        text: context.text.clone(),
                        images: context.images.clone(),
                    })
                    .await
                {
                    Answer::DraftConsumed { .. } => {}
                    answer => {
                        return Err(format!(
                            "The sent draft is retained until cleanup is confirmed: {answer:?}"
                        ));
                    }
                }
            }
            let (give, given) = oneshot::channel();
            self.consoles
                .prompts
                .send(Op::Consume(
                    environment.clone(),
                    Box::new(context.clone()),
                    give,
                ))
                .map_err(|_| "the prompt keeper stopped")?;
            if !given.await.unwrap_or(false) {
                return Err(
                    "The sent draft could not be cleared; its recovery context is retained".into(),
                );
            }
        } else if restore {
            self.app.dispatch(Msg::Show {
                thing: tab.clone().ok_or("the retained prompt has no tab")?,
                place: None,
                how: None,
            });
            let (give, given) = oneshot::channel();
            self.consoles
                .prompts
                .send(Op::Show(tab))
                .map_err(|_| "the prompt keeper stopped")?;
            self.consoles
                .prompts
                .send(Op::Recover(
                    environment.clone(),
                    Box::new(context.clone()),
                    give,
                ))
                .map_err(|_| "the prompt keeper stopped")?;
            if !given.await.unwrap_or(false) {
                self.app
                    .remote_draft(environment, record.intent.stamp, false, checked)
                    .await?;
                return Err("This tab holds a different draft. The recovered prompt remains in saved drafts.".into());
            }
        }
        if sent {
            self.app
                .resolve_remote(environment, record.intent.stamp, checked)
                .await
        } else {
            self.app
                .remote_draft(environment, record.intent.stamp, false, checked)
                .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_connect::journal::PromptTarget;
    use adroit_core::content::Tab;
    use adroit_core::prompt::send::Taken;

    #[test]
    fn the_context_preserves_the_named_draft_and_its_full_original_text() {
        let tab = Tab::draft(EnvironmentId::unbound(), "/remote/work");
        let taken = Taken {
            tab: Some(tab.clone()),
            text: "# Name\n日本語の本文".into(),
            ..Default::default()
        };
        let saved = context(&taken).unwrap();
        assert_eq!(saved.text, taken.text);
        assert_eq!(target(&saved, &EnvironmentId::unbound()), Some(tab));
        assert!(matches!(saved.target, PromptTarget::Draft { .. }));
    }

    #[test]
    fn a_prompt_is_looked_for_in_the_journal_of_the_environment_it_was_sent_in() {
        use crate::link::RemoteStatus;
        use adroit_connect::journal::Intent;
        use adroit_protocol::{MutationStamp, Request};
        use std::collections::BTreeMap;
        use std::sync::Arc;

        let (here, there) = (EnvironmentId::unbound(), EnvironmentId::new("there"));
        let context = PromptContext {
            target: PromptTarget::Session { id: "a".into() },
            text: "hello".into(),
            images: Vec::new(),
        };
        let record = Record {
            intent: Intent {
                stamp: MutationStamp {
                    instance: "i".into(),
                    work: "w".into(),
                    terminal: "t".into(),
                    epoch: 1,
                    id: "1".into(),
                },
                request: Request::Snapshot,
                prompt: Some(context.clone()),
            },
            outcome: None,
        };
        let mut remote = RemoteStatus::connected("box");
        remote.pending = Arc::new(vec![record.clone()]);
        let remotes = BTreeMap::from([(there.clone(), remote)]);
        assert_eq!(pending_prompt(&remotes, &there, &context), Some(record));
        assert_eq!(pending_prompt(&remotes, &here, &context), None);
    }

    #[test]
    fn confirmed_failures_keep_the_draft_and_only_success_consumes_it() {
        use adroit_protocol::{Answer, MutationState};
        assert_eq!(
            confirmed(&Some(MutationState::Result {
                answer: Box::new(Answer::Done)
            })),
            Some(true)
        );
        assert_eq!(
            confirmed(&Some(MutationState::Result {
                answer: Box::new(Answer::Failed {
                    text: "refused".into()
                })
            })),
            Some(false)
        );
        assert_eq!(confirmed(&Some(MutationState::Unknown)), None);
        assert_eq!(confirmed(&None), None);
    }
}
