//! What is written in the prompt for each tab. The prompt holds the shown
//! tab's text; the others' is kept aside until their tab is shown again.

pub(crate) mod remote;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use adroit_nvim::Embedded;
use dioxus::prelude::*;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;

use adroit_core::msg::Msg;
use adroit_protocol::{Answer, Ask, Drafts as KeptDrafts};

use crate::bridge::AppCtx;
use crate::content::Tab;
use crate::link::GONE;

use adroit_core::prompt::drafts::History;
pub use adroit_core::prompt::drafts::*;

pub type Taken = (Option<Tab>, String, Vec<PathBuf>);

/// What the prompt is asked to do, one after another.
pub enum Op {
    Show(Option<Tab>),
    Close(Option<Tab>),
    /// Lines for a tab; answered with whether the prompt took them.
    Add(Option<Tab>, Vec<String>, Option<oneshot::Sender<bool>>),
    /// The shown tab's text, with the tab it was written for, leaving the
    /// prompt empty.
    Take(oneshot::Sender<Taken>),
    Consume(
        adroit_connect::journal::PromptContext,
        oneshot::Sender<bool>,
    ),
    Recover(
        adroit_connect::journal::PromptContext,
        oneshot::Sender<bool>,
    ),
    Attach(PathBuf),
    RemoveImage(PathBuf),
    RestoreImages(Option<Tab>, Vec<PathBuf>),
    /// A prompt was sent: recalling starts again from the newest.
    Sent,
    /// Put the prompt sent one before, or one after when false, among those
    /// given oldest first, in the prompt, if it still shows the tab they were
    /// read for.
    Recall(Option<Tab>, bool, Vec<String>),
    /// Keep what is written, the shown tab's too, past the window.
    Save,
    /// The work host kept these drafts, or could not when none.
    Saved(Option<KeptDrafts>),
}

async fn put_draft<F, Fut>(drafts: &mut Drafts, opened: &mut bool, text: String, mut put: F)
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    if !put(text.clone()).await {
        drafts.put_back(text);
        *opened = false;
    }
}

async fn restore_before_op<F, Fut>(
    drafts: &mut Drafts,
    opened: &mut bool,
    op: Op,
    mut put: F,
) -> Result<Op, Op>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    if !*opened {
        if let Some(text) = drafts.first()
            && !put(text.clone()).await
        {
            drafts.put_back(text);
            return Err(op);
        }
        *opened = true;
    }
    Ok(op)
}

/// What is written as the work host keeps it, asked until a work host
/// answers; none when the one that answers cannot read what it keeps, so
/// that nothing is written over it.
async fn kept_drafts(app: AppCtx) -> Option<KeptDrafts> {
    loop {
        match app.ask(Ask::Drafts).await {
            Answer::Drafts(drafts) => return Some(*drafts),
            answer if retry_drafts(app.remote_status().is_some(), &answer) => {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Answer::Failed { text } => {
                eprintln!("adroit: {text}; what is written is not kept, to keep what is there");
                return None;
            }
            answer => {
                eprintln!("adroit: the work host answered {answer:?} for the drafts");
                return None;
            }
        }
    }
}

fn retry_drafts(remote: bool, answer: &Answer) -> bool {
    matches!(answer, Answer::Failed { text } if remote || text == GONE)
}

/// Send the work host what changed of `now` since `kept`, telling the keeper
/// through `tx` whether it kept it.
fn save(app: AppCtx, tx: UnboundedSender<Op>, kept: &KeptDrafts, now: KeptDrafts) {
    let changes = now.since(kept);
    let asked = app.ask(Ask::KeepDrafts(Box::new(changes)));
    spawn(async move {
        let saved = match asked.await {
            Answer::Done => Some(now),
            Answer::Failed { text } => {
                eprintln!("adroit: cannot keep what is written: {text}");
                None
            }
            answer => {
                eprintln!("adroit: the work host answered {answer:?} for the drafts");
                None
            }
        };
        let _ = tx.send(Op::Saved(saved));
    });
}

/// How often what is written is kept, so a window that ends however it ends
/// loses little of it.
const SAVE_EVERY: Duration = Duration::from_secs(2);

/// Where ops for the prompt are sent; cheap to copy into every handler.
#[derive(Clone, Copy)]
pub struct Prompts(CopyValue<UnboundedSender<Op>>);

impl Prompts {
    pub fn send(&self, op: Op) -> Result<(), SendError<Op>> {
        self.0.read().send(op)
    }
}

/// Run the ops sent to the returned sender on the prompt Neovim holds, one
/// at a time, so that taking a tab's text and putting another's never cross.
/// The view state is told the images attached to the prompt shown and the
/// tabs that have text kept aside, for the window to mark.
pub(crate) fn keeper(nvim: Signal<Option<Arc<Embedded>>>, app: AppCtx) -> Prompts {
    let show_images = move |images: &[PathBuf]| {
        if app.prompt.peek().images != images {
            app.dispatch(Msg::PromptImages(images.to_vec()));
        }
    };
    let (tx, mut ops) = unbounded_channel();
    // The prompt starts out showing what is written for nothing open, which
    // the view state starts out showing too; showing it puts back what was
    // written there before the window last closed.
    let _ = tx.send(Op::Show(None));
    {
        let tx = tx.clone();
        spawn(async move {
            loop {
                tokio::time::sleep(SAVE_EVERY).await;
                if tx.send(Op::Save).is_err() {
                    break;
                }
            }
        });
    }
    let saving = tx.clone();
    spawn(async move {
        // What the work host keeps, which saving changes; none when nothing
        // may be written.
        let mut kept = kept_drafts(app).await;
        let checkpoint = kept.clone().unwrap_or_default();
        let mut drafts = Drafts::restored(checkpoint.text);
        let mut images = adroit_core::prompt::images::Attachments::restored(checkpoint.images);
        // A save the work host has not answered yet; another waits for it.
        let mut in_flight = false;
        let mut opened = false;
        let mut history = History::default();
        let mut deferred = VecDeque::new();
        while let Some(incoming) = ops.recv().await {
            deferred.push_back(incoming);
            while let Some(op) = deferred.pop_front() {
                let nv = nvim.peek().clone();
                let put = |text: String| {
                    let nv = nv.clone();
                    async move {
                        match nv {
                            Some(nv) => nv.put_back(&text).await.is_ok(),
                            None => false,
                        }
                    }
                };
                let op = if nv.is_some() {
                    match restore_before_op(&mut drafts, &mut opened, op, put).await {
                        Ok(op) => op,
                        Err(op) => {
                            eprintln!(
                                "adroit: cannot restore the saved prompt; its draft remains saved"
                            );
                            deferred.push_front(op);
                            break;
                        }
                    }
                } else {
                    op
                };
                if opened && kept.is_some() && !*app.drafts_ready.peek() {
                    let mut ready = app.drafts_ready;
                    ready.set(true);
                }
                let checkpoint_due = matches!(&op, Op::Save);
                match op {
                    Op::Show(now) => {
                        if now == *drafts.shown() {
                            continue;
                        }
                        let Some(nv) = &nv else {
                            images.show(now.clone());
                            history.reset();
                            drafts.turn_to(now);
                            show_images(images.current());
                            continue;
                        };
                        if let Ok(at) = nv.cursor().await {
                            drafts.leave_cursor(at);
                        }
                        let Ok(taken) = nv.take_text().await else {
                            deferred.push_front(Op::Show(now));
                            break;
                        };
                        images.show(now.clone());
                        history.reset();
                        if let Some(text) = drafts.show(now, taken) {
                            put_draft(&mut drafts, &mut opened, text, put).await;
                            if let Some(at) = drafts.cursor() {
                                let _ = nv.set_cursor(at).await;
                            }
                        }
                    }
                    Op::Close(now) => {
                        images.close(now.clone());
                        history.reset();
                        if nv.is_none() {
                            drafts.turn_to(now);
                        } else if let Some(text) = drafts.close(now) {
                            put_draft(&mut drafts, &mut opened, text, put).await;
                        }
                    }
                    Op::Add(tab, lines, reply) => {
                        let took = if drafts.add(&tab, &lines) {
                            put(lines.join("\n")).await
                        } else {
                            true
                        };
                        if let Some(reply) = reply {
                            let _ = reply.send(took);
                        }
                    }
                    Op::Attach(path) => images.add(vec![path]),
                    Op::RemoveImage(path) => images.remove(&path),
                    Op::RestoreImages(tab, paths) => images.restore(tab, paths),
                    Op::Sent => history.reset(),
                    Op::Recall(tab, back, sent) => {
                        let Some(nv) = nv.as_ref().filter(|_| tab == *drafts.shown()) else {
                            continue;
                        };
                        let Ok(now) = nv.take_text().await else {
                            deferred.push_front(Op::Recall(tab, back, sent));
                            break;
                        };
                        let text = history.step(back, now.clone(), &sent).unwrap_or(now);
                        put_draft(&mut drafts, &mut opened, text, put).await;
                    }
                    Op::Take(reply) => {
                        let text = match &nv {
                            Some(nv) if app.remote_status().is_some() => nv.text().await.ok(),
                            Some(nv) => nv.take_text().await.ok(),
                            None => None,
                        };
                        let Some(text) = text else {
                            deferred.push_front(Op::Take(reply));
                            break;
                        };
                        let attachments = if app.remote_status().is_some() {
                            images.current().to_vec()
                        } else {
                            images.take()
                        };
                        let _ = reply.send((drafts.shown().clone(), text, attachments));
                    }
                    Op::Consume(context, reply) => {
                        let tab = remote::target(&context);
                        let handled = if drafts.shown() == &tab {
                            match &nv {
                                Some(nv) => nv.consume_text(&context.text).await.is_ok(),
                                None => false,
                            }
                        } else {
                            drafts.consume_hidden(&tab, &context.text);
                            true
                        };
                        if handled {
                            images.consume(&tab, &context.images);
                        }
                        // A newer draft is deliberately preserved.
                        let _ = reply.send(handled);
                    }
                    Op::Recover(context, reply) => {
                        let restored = if drafts.shown() == &remote::target(&context) {
                            match &nv {
                                Some(nv) => match nv.text().await {
                                    Ok(text) if text == context.text => true,
                                    Ok(text) if text.is_empty() => put(context.text.clone()).await,
                                    _ => false,
                                },
                                None => false,
                            }
                        } else {
                            false
                        };
                        if restored && images.current().is_empty() {
                            images.add(context.images);
                        }
                        let _ = reply.send(restored);
                    }
                    Op::Save => {}
                    Op::Saved(saved) => {
                        in_flight = false;
                        if saved.is_some() {
                            kept = saved;
                        }
                        continue;
                    }
                }
                if checkpoint_due
                    && !in_flight
                    && let Some(was) = &kept
                {
                    let text = if let Some(nv) = &nv {
                        match nv.text().await {
                            Ok(text) => text,
                            Err(error) => {
                                eprintln!("adroit: cannot save the prompt checkpoint: {error}");
                                continue;
                            }
                        }
                    } else {
                        String::new()
                    };
                    let now = KeptDrafts {
                        text: drafts.saved(&text),
                        images: images.saved(),
                    };
                    if now != *was {
                        in_flight = true;
                        save(app, saving.clone(), was, now);
                    }
                }
                show_images(images.current());
                let mut holding = drafts.holding();
                holding.extend(images.holding());
                if app.prompt.peek().drafted != holding {
                    app.dispatch(Msg::Drafted(holding));
                }
            }
        }
    });
    Prompts(CopyValue::new(tx))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn remote_draft_loading_retries_initial_connection_failures() {
        let offline = Answer::Failed {
            text: "The remote host is disconnected; the request was not sent".into(),
        };
        assert!(retry_drafts(true, &offline));
        assert!(!retry_drafts(false, &offline));
        assert!(retry_drafts(false, &Answer::Failed { text: GONE.into() }));
        assert!(!retry_drafts(true, &Answer::Drafts(Box::default())));
    }

    #[tokio::test]
    async fn a_failed_tab_restore_keeps_both_drafts_until_the_prompt_can_take_it() {
        for closing in [false, true] {
            let old = Some(Tab::Session("old".into()));
            let next = Some(Tab::Session("next".into()));
            let mut drafts = Drafts::restored(BTreeMap::from([(
                "session:next".into(),
                "next draft".into(),
            )]));
            drafts.turn_to(old);
            let mut opened = true;
            let text = if closing {
                drafts.close(next.clone())
            } else {
                drafts.show(next.clone(), "old draft".into())
            }
            .unwrap();
            put_draft(&mut drafts, &mut opened, text, |_| {
                std::future::ready(false)
            })
            .await;
            assert_eq!(
                drafts.saved("").get("session:next").map(String::as_str),
                Some("next draft")
            );
            if !closing {
                assert_eq!(
                    drafts.saved("").get("session:old").map(String::as_str),
                    Some("old draft")
                );
            }
            assert!(!opened);
            let mut restored = String::new();
            assert!(
                restore_before_op(&mut drafts, &mut opened, Op::Show(next), |text| {
                    restored = text;
                    std::future::ready(true)
                })
                .await
                .is_ok()
            );
            assert_eq!(restored, "next draft");
            assert!(opened);
        }
    }

    #[tokio::test]
    async fn failed_initial_restore_retains_the_identity_switch_and_take_reply_for_retry() {
        let old = Some(Tab::Session("old".into()));
        let next = Some(Tab::Session("next".into()));
        let mut drafts =
            Drafts::restored(BTreeMap::from([("session:old".into(), "old draft".into())]));
        drafts.turn_to(old.clone());
        let mut opened = false;
        let queued = restore_before_op(&mut drafts, &mut opened, Op::Show(next.clone()), |_| {
            std::future::ready(false)
        })
        .await;
        let Err(op @ Op::Show(_)) = queued else {
            panic!("the failed operation must remain available")
        };
        assert!(!opened);
        assert_eq!(*drafts.shown(), old);
        assert_eq!(
            drafts.saved("").get("session:old").map(String::as_str),
            Some("old draft")
        );
        let mut restored = String::new();
        let result = restore_before_op(&mut drafts, &mut opened, op, |text| {
            restored = text;
            std::future::ready(true)
        })
        .await;
        let Ok(Op::Show(next)) = result else {
            panic!("the retried identity switch must survive")
        };
        drafts.show(next, restored);
        assert_eq!(
            drafts.saved("").get("session:old").map(String::as_str),
            Some("old draft")
        );

        drafts.turn_to(old);
        opened = false;
        let (reply, received) = oneshot::channel();
        let Err(op) = restore_before_op(&mut drafts, &mut opened, Op::Take(reply), |_| {
            std::future::ready(false)
        })
        .await
        else {
            panic!("the failed take must remain queued")
        };
        let Ok(Op::Take(reply)) =
            restore_before_op(&mut drafts, &mut opened, op, |_| std::future::ready(true)).await
        else {
            panic!("the take reply must survive retry")
        };
        reply
            .send((drafts.shown().clone(), "old draft".into(), vec![]))
            .unwrap();
        assert_eq!(received.await.unwrap().1, "old draft");
    }
}
