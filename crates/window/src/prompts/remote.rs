//! Retained prompt contexts outlive the UI task that initiated the send.

use super::Op;
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
    pub(crate) async fn finish_remote_prompt(
        &self,
        context: &PromptContext,
        sent: bool,
    ) -> Result<(), String> {
        let record = self.app.remote_status().and_then(|status| {
            status
                .pending
                .iter()
                .find(|r| r.intent.prompt.as_ref() == Some(context))
                .cloned()
        });
        let Some(record) = record else {
            return Ok(());
        };
        self.handle_remote_prompt(record, sent, false, false).await
    }

    pub(crate) async fn handle_remote_prompt(
        &self,
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
        let tab = target(context, &self.app.environment());
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
                .send(Op::Consume(context.clone(), give))
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
                .send(Op::Recover(context.clone(), give))
                .map_err(|_| "the prompt keeper stopped")?;
            if !given.await.unwrap_or(false) {
                self.app
                    .remote_draft(record.intent.stamp, false, checked)
                    .await?;
                return Err("This tab holds a different draft. The recovered prompt remains in saved drafts.".into());
            }
        }
        if sent {
            self.app.resolve_remote(record.intent.stamp, checked).await
        } else {
            self.app
                .remote_draft(record.intent.stamp, false, checked)
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
