//! Mutations cross the wire only after their exact intent is durable.

use super::{Shared, Waiting, lock};
use adroit_connect::journal::PromptContext;
use adroit_protocol::{Answer, Ask, Event, MutationStamp, MutationState, Request, Told};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::{mpsc, oneshot};

pub(super) enum Command {
    Work {
        request: Request,
        prompt: Option<PromptContext>,
    },
    Query(MutationStamp),
    Draft {
        stamp: MutationStamp,
        forget: bool,
        checked: bool,
        answered: oneshot::Sender<Result<(), String>>,
    },
    Resolve {
        stamp: MutationStamp,
        checked: bool,
        answered: oneshot::Sender<Result<(), String>>,
    },
}

pub(super) fn needs_prompt(request: &Request) -> bool {
    matches!(request, Request::Ask { ask, .. } if matches!(ask.as_ref(), Ask::SubmitPrompt { .. }) || matches!(ask.as_ref(), Ask::StartSession(start) if !start.prompt.is_empty() || !start.images.is_empty()))
}

pub(super) fn consumes_draft(request: &Request) -> bool {
    matches!(request, Request::Ask { ask, .. } if matches!(ask.as_ref(), Ask::ConsumeDraft { .. }))
}

impl Shared {
    pub(super) fn refresh_changes(&self) {
        let journal = lock(&self.journal).clone();
        let records = journal
            .as_ref()
            .map_or(Ok((Vec::new(), Vec::new())), |journal| {
                Ok((journal.records()?, journal.drafts()?))
            });
        let mut status = lock(&self.status);
        match records {
            Ok((records, drafts)) => {
                status.writable = self.ready.load(Ordering::Acquire)
                    && lock(&self.work).is_some()
                    && records.is_empty();
                status.pending = Arc::new(records);
                status.drafts = Arc::new(drafts);
            }
            Err(text) => {
                status.writable = false;
                status.problem = Some(text);
                self.storage_failed.store(true, Ordering::Release);
            }
        }
    }

    pub(super) fn change(
        self: &Arc<Self>,
        request: Request,
        prompt: Option<PromptContext>,
        waiting: Waiting,
    ) {
        let work = lock(&self.work).clone();
        let Some(work) = work else {
            self.answer(
                &request,
                &waiting,
                Answer::Failed {
                    text: "This window cannot change the remote work".into(),
                },
            );
            return;
        };
        if !self.ready.load(Ordering::Acquire) || self.storage_failed.load(Ordering::Acquire) {
            self.answer(
                &request,
                &waiting,
                Answer::Failed {
                    text: "Remote changes are unavailable; the request was not sent".into(),
                },
            );
            return;
        }
        if !consumes_draft(&request) && !lock(&self.status).pending.is_empty() {
            self.answer(
                &request,
                &waiting,
                Answer::Failed {
                    text: "Resolve the pending remote outcome before another change".into(),
                },
            );
            return;
        }
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.answer(
                &request,
                &waiting,
                Answer::Failed {
                    text: "Another remote change is being confirmed; this request was not sent"
                        .into(),
                },
            );
            return;
        }
        let shared = self.clone();
        tokio::spawn(async move {
            let original = request.clone();
            let has_prompt = prompt.is_some();
            let changing = tokio::spawn(async move {
                match prompt {
                    Some(prompt) => work.mutate_prompt(request, prompt).await,
                    None => work.mutate(request).await,
                }
            })
            .await;
            let answer = match changing {
                Ok(Ok(outcome)) => {
                    let state = outcome.state.clone();
                    if !has_prompt
                        && matches!(
                            state,
                            MutationState::Result { .. } | MutationState::Rejected { .. }
                        )
                        && outcome.problem.is_none()
                        && let Some(journal) = lock(&shared.journal).clone()
                        && let Err(text) = journal.acknowledge(&outcome.intent.stamp)
                    {
                        let _ = shared.told.try_send(Event::Told(Told::Failure { text }));
                    }
                    match state {
                        MutationState::Result { answer } if outcome.problem.is_none() => *answer,
                        MutationState::Rejected { text } => Answer::Failed { text },
                        _ => Answer::Unconfirmed { text: outcome.problem.unwrap_or_else(|| "The change's outcome is unconfirmed. Check it before initiating another change.".into()) },
                    }
                }
                Ok(Err(text)) => Answer::Failed { text },
                Err(error) => Answer::Unconfirmed {
                    text: format!("The remote change could not be confirmed: {error}"),
                },
            };
            shared.refresh_changes();
            shared.busy.store(false, Ordering::Release);
            shared.answer(&original, &waiting, answer);
        });
    }

    fn answer(&self, request: &Request, waiting: &Waiting, answer: Answer) {
        if let Request::Ask { asked, .. } = request
            && let Some(waiting) = lock(waiting).remove(asked)
        {
            let _ = waiting.send(answer);
            return;
        }
        let text = match answer {
            Answer::Done => return,
            Answer::Failed { text } | Answer::Unconfirmed { text } => text,
            _ => return,
        };
        let _ = self.told.try_send(Event::Told(Told::Failure { text }));
    }

    pub(super) fn control(self: &Arc<Self>, command: Command) {
        match command {
            Command::Draft {
                stamp,
                forget,
                checked,
                answered,
            } => {
                let result = if self.busy.load(Ordering::Acquire) {
                    Err("A remote change is still being confirmed".into())
                } else if let Some(journal) = lock(&self.journal).clone() {
                    if forget {
                        journal.forget_draft(&stamp)
                    } else {
                        journal.retain_draft(&stamp, checked)
                    }
                } else {
                    Err("The remote journal is unavailable".into())
                };
                self.refresh_changes();
                let _ = answered.send(result);
            }
            Command::Query(stamp) => {
                let Some(work) = lock(&self.work).clone() else {
                    return;
                };
                if self
                    .busy
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
                {
                    return;
                }
                let shared = self.clone();
                tokio::spawn(async move {
                    if let Err(text) = work.recover(&stamp).await {
                        let _ = shared.told.try_send(Event::Told(Told::Failure { text }));
                    }
                    shared.refresh_changes();
                    shared.busy.store(false, Ordering::Release);
                });
            }
            Command::Resolve {
                stamp,
                checked,
                answered,
            } => {
                let result = if self.busy.load(Ordering::Acquire) {
                    Err("A remote change is still being confirmed".into())
                } else if let Some(journal) = lock(&self.journal).clone() {
                    if checked {
                        journal.discard_after_check(&stamp)
                    } else {
                        journal.acknowledge(&stamp)
                    }
                } else {
                    Err("The remote journal is unavailable".into())
                };
                self.refresh_changes();
                let _ = answered.send(result);
            }
            Command::Work { .. } => {}
        }
    }

    pub(super) fn recover_pending(self: &Arc<Self>) {
        let Some(work) = lock(&self.work).clone() else {
            return;
        };
        let records = lock(&self.status).pending.clone();
        if records.is_empty()
            || self
                .busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return;
        }
        let shared = self.clone();
        tokio::spawn(async move {
            for record in records.iter() {
                match work.recover(&record.intent.stamp).await {
                    Ok(outcome) => {
                        if record.intent.prompt.is_none()
                            && matches!(
                                outcome.state,
                                MutationState::Result { .. } | MutationState::Rejected { .. }
                            )
                        {
                            let _ = work.journal().acknowledge(&record.intent.stamp);
                        }
                    }
                    Err(text) => {
                        let _ = shared.told.try_send(Event::Told(Told::Failure { text }));
                    }
                }
            }
            shared.refresh_changes();
            shared.busy.store(false, Ordering::Release);
        });
    }
}

pub(super) async fn resolve(
    sender: &mpsc::Sender<Command>,
    stamp: MutationStamp,
    checked: bool,
) -> Result<(), String> {
    let (answer, answered) = oneshot::channel();
    sender
        .try_send(Command::Resolve {
            stamp,
            checked,
            answered: answer,
        })
        .map_err(|_| "The remote connection is busy or closed".to_string())?;
    tokio::time::timeout(std::time::Duration::from_secs(10), answered)
        .await
        .map_err(|_| "Confirming the local journal timed out".to_string())?
        .map_err(|_| "The remote connection closed".to_string())?
}

pub(super) async fn draft(
    sender: &mpsc::Sender<Command>,
    stamp: MutationStamp,
    forget: bool,
    checked: bool,
) -> Result<(), String> {
    let (answer, answered) = oneshot::channel();
    sender
        .try_send(Command::Draft {
            stamp,
            forget,
            checked,
            answered: answer,
        })
        .map_err(|_| "The remote connection is busy or closed".to_string())?;
    tokio::time::timeout(std::time::Duration::from_secs(10), answered)
        .await
        .map_err(|_| "Saving the retained draft timed out".to_string())?
        .map_err(|_| "The remote connection closed".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_connect::journal::Journal;

    #[tokio::test]
    async fn uncertain_records_require_an_explicit_check_and_resolving_offline_never_enables_changes()
     {
        let dir = tempfile::tempdir().unwrap();
        let journal =
            Arc::new(Journal::open(&dir.path().join("journal"), &"a".repeat(32)).unwrap());
        let intent = adroit_connect::journal::Intent {
            stamp: MutationStamp {
                instance: "a".repeat(32),
                work: "b".repeat(32),
                terminal: "c".repeat(32),
                epoch: 1,
                id: "d".repeat(32),
            },
            request: Request::Release { id: 7 },
            prompt: None,
        };
        journal.prepare(&intent).unwrap();
        let (told, _events) = mpsc::channel(8);
        let shared = Shared::new("work".into(), told, true);
        *lock(&shared.journal) = Some(journal.clone());
        shared.refresh_changes();
        let (answer, answered) = oneshot::channel();
        shared.control(Command::Resolve {
            stamp: intent.stamp.clone(),
            checked: false,
            answered: answer,
        });
        assert!(answered.await.unwrap().is_err());
        assert_eq!(lock(&shared.status).pending.len(), 1);
        shared.busy.store(true, Ordering::Release);
        let (answer, answered) = oneshot::channel();
        shared.control(Command::Resolve {
            stamp: intent.stamp.clone(),
            checked: true,
            answered: answer,
        });
        assert!(answered.await.unwrap().is_err());
        shared.busy.store(false, Ordering::Release);
        let (answer, answered) = oneshot::channel();
        shared.control(Command::Resolve {
            stamp: intent.stamp,
            checked: true,
            answered: answer,
        });
        assert!(answered.await.unwrap().is_ok());
        assert!(journal.pending().unwrap().is_empty());
        assert!(lock(&shared.status).pending.is_empty());
        assert!(!lock(&shared.status).writable);
    }
}
