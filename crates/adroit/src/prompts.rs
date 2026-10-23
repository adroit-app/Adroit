//! What is written in the prompt for each tab. The prompt holds the shown
//! tab's text; the others' is kept aside until their tab is shown again.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use adroit_nvim::Embedded;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;

use adroit_core::msg::Msg;

use crate::bridge::AppCtx;
use crate::content::Tab;
use crate::paths::instance_state;

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

/// Where what is written is kept past the window.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DraftCheckpoint {
    text: BTreeMap<String, String>,
    images: BTreeMap<String, Vec<PathBuf>>,
}

impl DraftCheckpoint {
    fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| format!("cannot read {}: {error}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("cannot read {}: {error}", path.display())),
        }
    }

    fn load(directory: &Path) -> Result<Self, String> {
        if let Some(checkpoint) = Self::read(&directory.join("draft-checkpoint.json"))? {
            return Ok(checkpoint);
        }
        Ok(Self {
            text: Self::read(&directory.join("drafts.json"))?.unwrap_or_default(),
            images: Self::read(&directory.join("attachments.json"))?.unwrap_or_default(),
        })
    }

    fn keep(&self, directory: &Path, saved: &mut Option<Self>) -> Result<(), String> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        if saved.as_ref() == Some(self) {
            return Ok(());
        }
        let temporary = directory.join(format!(
            ".draft-checkpoint-{}.tmp",
            adroit_model::DraftId::new()
        ));
        let result = (|| {
            std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)
                .map_err(|error| error.to_string())?;
            file.write_all(&serde_json::to_vec(self).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            std::fs::rename(&temporary, directory.join("draft-checkpoint.json"))
                .map_err(|error| error.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result?;
        *saved = Some(self.clone());
        Ok(())
    }
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
    spawn(async move {
        let directory = instance_state();
        let (checkpoint, writable) = match DraftCheckpoint::load(&directory) {
            Ok(checkpoint) => (checkpoint, true),
            Err(error) => {
                eprintln!(
                    "adroit: {error}; draft checkpoint saving is disabled to preserve the existing file"
                );
                (DraftCheckpoint::default(), false)
            }
        };
        let mut drafts = Drafts::restored(checkpoint.text);
        let mut images = adroit_core::prompt::images::Attachments::restored(checkpoint.images);
        let mut saved = None;
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
                        let lines: Vec<String> = text.lines().map(String::from).collect();
                        match nv {
                            Some(nv) => nv.append(&lines).await.is_ok(),
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
                let checkpoint_due = matches!(&op, Op::Save);
                match op {
                    Op::Show(now) => {
                        if now == *drafts.shown() {
                            continue;
                        }
                        images.show(now.clone());
                        history.reset();
                        let Some(nv) = &nv else {
                            drafts.turn_to(now);
                            show_images(images.current());
                            continue;
                        };
                        let taken = nv.take_text().await.unwrap_or_default();
                        if let Some(text) = drafts.show(now, taken) {
                            put(text).await;
                        }
                    }
                    Op::Close(now) => {
                        images.close(now.clone());
                        history.reset();
                        if nv.is_none() {
                            drafts.turn_to(now);
                        } else if let Some(text) = drafts.close(now) {
                            put(text).await;
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
                        let now = nv.take_text().await.unwrap_or_default();
                        let text = history.step(back, now.clone(), &sent).unwrap_or(now);
                        put(text).await;
                    }
                    Op::Take(reply) => {
                        let text = match &nv {
                            Some(nv) => nv.take_text().await.unwrap_or_default(),
                            None => String::new(),
                        };
                        let _ = reply.send((drafts.shown().clone(), text, images.take()));
                    }
                    Op::Save => {}
                }
                if checkpoint_due {
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
                    let checkpoint = DraftCheckpoint {
                        text: drafts.saved(&text),
                        images: images.saved(),
                    };
                    if writable && let Err(error) = checkpoint.keep(&directory, &mut saved) {
                        eprintln!("adroit: cannot save the prompt checkpoint: {error}");
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
    use super::*;

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

    #[test]
    fn failed_checkpoint_writes_do_not_advance_the_saved_baseline_and_can_retry() {
        let directory = tempfile::tempdir().unwrap();
        let blocked = directory.path().join("not-a-directory");
        std::fs::write(&blocked, "Keep me").unwrap();
        let checkpoint = DraftCheckpoint::default();
        let mut saved = None;
        assert!(checkpoint.keep(&blocked, &mut saved).is_err());
        assert!(saved.is_none());
        assert_eq!(std::fs::read_to_string(&blocked).unwrap(), "Keep me");
        checkpoint.keep(directory.path(), &mut saved).unwrap();
        assert_eq!(saved, Some(checkpoint.clone()));
        assert_eq!(DraftCheckpoint::load(directory.path()).unwrap(), checkpoint);
    }

    #[test]
    fn legacy_files_are_read_only_until_a_checkpoint_exists_and_never_mask_a_broken_checkpoint() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("drafts.json"),
            r#"{"session:old":"legacy draft"}"#,
        )
        .unwrap();
        std::fs::write(
            directory.path().join("attachments.json"),
            r#"{"session:old":["/legacy.png"]}"#,
        )
        .unwrap();
        let checkpoint = DraftCheckpoint::load(directory.path()).unwrap();
        assert_eq!(
            checkpoint.text.get("session:old").map(String::as_str),
            Some("legacy draft")
        );
        assert_eq!(
            checkpoint.images.get("session:old"),
            Some(&vec![PathBuf::from("/legacy.png")])
        );
        std::fs::write(
            directory.path().join("draft-checkpoint.json"),
            "incomplete checkpoint",
        )
        .unwrap();
        assert!(DraftCheckpoint::load(directory.path()).is_err());
    }
}
