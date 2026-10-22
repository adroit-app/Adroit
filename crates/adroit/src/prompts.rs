//! What is written in the prompt for each tab. The prompt holds the shown
//! tab's text; the others' is kept aside until their tab is shown again.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use adroit_model::Session;
use adroit_nvim::Embedded;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;

use crate::nav;
use crate::paths::instance_state;
use crate::tabs::{self, Tab};

/// The name the text written with nothing open is kept by.
const NOTHING_OPEN: &str = "nothing";

/// The text kept for the tabs not shown, and which tab the prompt is for.
#[derive(Debug, Default)]
pub struct Drafts {
    shown: Option<Tab>,
    kept: HashMap<Option<Tab>, String>,
}

impl Drafts {
    fn rebind(&mut self, previous: &Tab, next: &Tab, shown_text: Option<String>) -> Option<String> {
        let visible = self
            .shown
            .as_ref()
            .is_some_and(|tab| tab == previous || tab == next);
        if let Some(text) = shown_text {
            self.kept.insert(self.shown.clone(), text);
        }
        if let Some(source) = self
            .kept
            .remove(&Some(previous.clone()))
            .filter(|text| !text.is_empty())
        {
            let target = self.kept.entry(Some(next.clone())).or_default();
            if !target.is_empty() {
                target.push_str("\n\n");
            }
            target.push_str(&source);
        }
        visible.then(|| self.kept.remove(&self.shown).unwrap_or_default())
    }

    /// `now` is shown, the prompt having held `taken` for the tab shown
    /// before. What to put in the prompt for `now`, if anything was kept.
    pub fn show(&mut self, now: Option<Tab>, taken: String) -> Option<String> {
        let before = std::mem::replace(&mut self.shown, now.clone());
        // Written with nothing open, it is the first prompt of the draft
        // opened next, unless that draft has its own.
        if before.is_none()
            && matches!(now, Some(Tab::Draft { .. }))
            && !taken.is_empty()
            && !self.kept.contains_key(&now)
        {
            return Some(taken);
        }
        if !taken.is_empty() {
            self.kept.insert(before, taken);
        }
        self.kept.remove(&now)
    }

    /// The tab shown before closed and `now` is shown: what the prompt holds
    /// stays, for `now`. What was kept for `now`, to put after it.
    pub fn close(&mut self, now: Option<Tab>) -> Option<String> {
        self.shown = now.clone();
        self.kept.remove(&now)
    }

    /// Lines arrived for `tab`. True when they belong in the prompt, as `tab`
    /// is shown; otherwise they are kept after what `tab` has.
    pub fn add(&mut self, tab: &Option<Tab>, lines: &[String]) -> bool {
        if *tab == self.shown {
            return true;
        }
        let text = self.kept.entry(tab.clone()).or_default();
        for line in lines {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(line);
        }
        false
    }

    /// What is written for each tab, by the name the tab is kept by, the
    /// shown one's being `shown_text`. A draft's tab names nothing that
    /// outlives the window, so its text is not among them.
    pub fn saved(&self, shown_text: &str) -> BTreeMap<String, String> {
        let name = |tab: &Option<Tab>| match tab {
            None => Some(NOTHING_OPEN.to_string()),
            Some(tab) => tabs::kept_name(tab),
        };
        self.kept
            .iter()
            .map(|(tab, text)| (tab, text.as_str()))
            .chain(Some((&self.shown, shown_text)))
            .filter(|(_, text)| !text.trim().is_empty())
            .filter_map(|(tab, text)| Some((name(tab)?, text.to_string())))
            .collect()
    }

    /// The text [`Drafts::saved`] gave, kept aside for each tab, with nothing
    /// shown yet.
    pub fn restored(saved: BTreeMap<String, String>) -> Self {
        let kept = saved
            .into_iter()
            .filter_map(|(name, text)| match name.as_str() {
                NOTHING_OPEN => Some((None, text)),
                _ => tabs::from_kept(&name).map(|tab| (Some(tab), text)),
            })
            .collect();
        Self { shown: None, kept }
    }

    /// `now` is shown while there is no prompt to put its text in: the text
    /// stays kept until [`Drafts::first`] gives it.
    pub fn turn_to(&mut self, now: Option<Tab>) {
        self.shown = now;
    }

    /// The text for the tab shown when the prompt first opens, which is the
    /// window shown with nothing open unless a tab was shown before then.
    pub fn first(&mut self) -> Option<String> {
        self.kept.remove(&self.shown)
    }

    /// The tabs not shown that have text kept aside.
    pub fn holding(&self) -> HashSet<Tab> {
        self.kept
            .iter()
            .filter(|(_, text)| !text.trim().is_empty())
            .filter_map(|(tab, _)| tab.clone())
            .collect()
    }

    /// Keep each session's text under the id it goes by now.
    pub fn follow(&mut self, sessions: &[Session]) {
        let moved = |tab: &Option<Tab>| match tab {
            Some(Tab::Session(id)) => nav::successor(sessions, id)
                .map(|new| Some(Tab::Session(new.to_string())))
                .unwrap_or_else(|| tab.clone()),
            _ => tab.clone(),
        };
        self.shown = moved(&self.shown);
        self.kept = std::mem::take(&mut self.kept)
            .into_iter()
            .map(|(tab, text)| (moved(&tab), text))
            .collect();
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LeaderDrafts(HashMap<adroit_model::LogicalSessionId, LeaderDraft>);

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LeaderDraft {
    generation: u64,
    terminal: Option<(u64, String)>,
}

impl LeaderDrafts {
    fn update(&mut self, snapshot: &adroit_model::ControlSnapshot) -> Vec<(Tab, Tab)> {
        let mut moved = Vec::new();
        for session in &snapshot.sessions {
            if session.binding.role() != adroit_model::SessionRole::Leader {
                continue;
            }
            let id = session.binding.logical_id();
            let cached = self.0.entry(id).or_default();
            if session.generation < cached.generation {
                continue;
            }
            cached.generation = session.generation;
            let Some(terminal) = &session.terminal_id else {
                continue;
            };
            if let Some((generation, previous)) = &cached.terminal {
                if session.generation <= *generation {
                    continue;
                }
                if terminal != previous {
                    moved.push((
                        Tab::Session(previous.clone()),
                        Tab::Session(terminal.clone()),
                    ));
                }
            }
            cached.terminal = Some((session.generation, terminal.clone()));
        }
        moved
    }
}

/// Where recalling the prompts sent has got to; the prompts themselves are
/// Claude Code's, read each time.
#[derive(Debug, Default)]
pub struct History {
    /// How far back the prompt is, and what it held before recalling began.
    at: Option<(usize, String)>,
}

impl History {
    /// Recalling starts again from the newest.
    pub fn reset(&mut self) {
        self.at = None;
    }

    /// The text to put in the prompt one step `back` or forward through
    /// `sent`, oldest first, the prompt holding `now`; forward past the
    /// newest gives back what it held before recalling began. `None` when
    /// there is nowhere to go.
    pub fn step(&mut self, back: bool, now: String, sent: &[String]) -> Option<String> {
        let mut kept: Vec<&str> = Vec::new();
        for s in sent.iter().map(|s| s.trim_end()) {
            if kept.last() != Some(&s) {
                kept.push(s);
            }
        }
        let n = kept.len();
        // The prompts may have changed since the last step: another tab's,
        // or a history rewritten meanwhile.
        let at = self.at.take().filter(|(k, _)| *k <= n);
        match (at, back) {
            (None, true) if n > 0 => {
                self.at = Some((1, now));
                Some(kept[n - 1].to_string())
            }
            (None, _) => None,
            (Some((k, draft)), true) if k < n => {
                self.at = Some((k + 1, draft));
                Some(kept[n - k - 1].to_string())
            }
            (Some((k, draft)), true) => {
                self.at = Some((k, draft));
                None
            }
            (Some((1, draft)), false) => Some(draft),
            (Some((k, draft)), false) => {
                self.at = Some((k - 1, draft));
                Some(kept[n - k + 1].to_string())
            }
        }
    }
}

pub type Taken = (Option<Tab>, String, Vec<PathBuf>);

/// What the prompt is asked to do, one after another.
pub enum Op {
    Show(Option<Tab>),
    Close(Option<Tab>),
    /// Lines for a tab; answered with whether the prompt took them.
    Add(Option<Tab>, Vec<String>, Option<oneshot::Sender<bool>>),
    Follow(Vec<Session>),
    ObserveLeaders(Box<adroit_model::ControlSnapshot>),
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
            drafts.kept.insert(drafts.shown.clone(), text);
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
    leaders: LeaderDrafts,
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
            leaders: LeaderDrafts::default(),
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
            adroit_model::RequestId::new()
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
/// `drafted` follows the tabs that have text kept aside, for the window to
/// mark.
pub fn keeper(
    nvim: Signal<Option<Arc<Embedded>>>,
    mut drafted: Signal<HashSet<Tab>>,
    mut shown_images: Signal<Vec<PathBuf>>,
) -> Prompts {
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
        let mut images = crate::pasted::Attachments::restored(checkpoint.images);
        let mut leaders = checkpoint.leaders;
        let mut saved = None;
        let mut pending_observation = None;
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
                let checkpoint_due = matches!(&op, Op::Save | Op::ObserveLeaders(_));
                let observation = match &op {
                    Op::ObserveLeaders(snapshot) => Some(snapshot.clone()),
                    Op::Save => pending_observation.take(),
                    _ => None,
                };
                if let Some(snapshot) = observation {
                    let mut staged = leaders.clone();
                    let mut accepted = true;
                    for (previous, next) in staged.update(&snapshot) {
                        let visible = drafts
                            .shown
                            .as_ref()
                            .is_some_and(|tab| tab == &previous || tab == &next);
                        let text = if visible {
                            history.reset();
                            match &nv {
                                Some(nv) => match nv.take_text().await {
                                    Ok(text) => Some(text),
                                    Err(error) => {
                                        eprintln!(
                                            "adroit: cannot recover the Leader draft: {error}"
                                        );
                                        accepted = false;
                                        break;
                                    }
                                },
                                None => None,
                            }
                        } else {
                            None
                        };
                        if let Some(text) = drafts.rebind(&previous, &next, text)
                            && (nv.is_none() || !put(text.clone()).await)
                        {
                            drafts.kept.insert(drafts.shown.clone(), text);
                            opened = false;
                        }
                        images.rebind(&previous, &next);
                    }
                    if !accepted {
                        pending_observation = Some(snapshot);
                        continue;
                    }
                    leaders = staged;
                    pending_observation = None;
                }
                match op {
                    Op::Show(now) => {
                        if now == drafts.shown {
                            continue;
                        }
                        images.show(now.clone());
                        history.reset();
                        let Some(nv) = &nv else {
                            drafts.turn_to(now);
                            shown_images.set(images.current().to_vec());
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
                    Op::Follow(sessions) => {
                        images.follow(&sessions);
                        drafts.follow(&sessions);
                    }
                    Op::ObserveLeaders(_) => {}
                    Op::Attach(path) => images.add(vec![path]),
                    Op::RemoveImage(path) => images.remove(&path),
                    Op::RestoreImages(tab, paths) => images.restore(tab, paths),
                    Op::Sent => history.reset(),
                    Op::Recall(tab, back, sent) => {
                        let Some(nv) = nv.as_ref().filter(|_| tab == drafts.shown) else {
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
                        let _ = reply.send((drafts.shown.clone(), text, images.take()));
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
                        leaders: leaders.clone(),
                    };
                    if writable && let Err(error) = checkpoint.keep(&directory, &mut saved) {
                        eprintln!("adroit: cannot save the prompt checkpoint: {error}");
                    }
                }
                if *shown_images.peek() != images.current() {
                    shown_images.set(images.current().to_vec());
                }
                let mut holding = drafts.holding();
                holding.extend(images.holding());
                if *drafted.peek() != holding {
                    drafted.set(holding);
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
        assert_eq!(drafts.shown, old);
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
            .send((drafts.shown.clone(), "old draft".into(), vec![]))
            .unwrap();
        assert_eq!(received.await.unwrap().1, "old draft");
    }

    #[test]
    fn a_checkpoint_restores_a_leaders_draft_after_restart_while_the_window_was_closed() {
        use adroit_model::*;
        let directory = tempfile::tempdir().unwrap();
        let logical = LogicalSessionId::new();
        let other = LogicalSessionId::new();
        let record = |id, generation, terminal: &str| ManagedSession {
            binding: SessionBinding::leader(id, Some(RunId::new())),
            agent: Agent::Codex,
            generation,
            state: ManagedSessionState::Running,
            permissions: AgentPermissions::Default,
            cwd: Some("/same-root".into()),
            terminal_id: Some(terminal.into()),
            conversation_id: None,
            readiness: Default::default(),
            spawned_at_ms: None,
        };
        let snapshot = |sessions| ControlSnapshot {
            sessions,
            ..Default::default()
        };
        let mut leaders = LeaderDrafts::default();
        leaders.update(&snapshot(vec![
            record(logical, 1, "old"),
            record(other, 1, "other"),
        ]));
        let checkpoint = DraftCheckpoint {
            text: BTreeMap::from([
                ("session:old".into(), "saved request draft".into()),
                ("session:other".into(), "unrelated draft".into()),
            ]),
            images: BTreeMap::from([("session:old".into(), vec![PathBuf::from("/old.png")])]),
            leaders,
        };
        let mut saved = None;
        checkpoint.keep(directory.path(), &mut saved).unwrap();
        std::fs::write(
            directory.path().join("drafts.json"),
            "stale invalid legacy file",
        )
        .unwrap();
        let restored = DraftCheckpoint::load(directory.path()).unwrap();
        assert_eq!(restored, checkpoint);
        let mut drafts = Drafts::restored(restored.text);
        let mut images = crate::pasted::Attachments::restored(restored.images);
        let mut leaders = restored.leaders;
        for (previous, next) in leaders.update(&snapshot(vec![
            record(logical, 2, "new"),
            record(other, 1, "other"),
        ])) {
            drafts.rebind(&previous, &next, None);
            images.rebind(&previous, &next);
        }
        assert_eq!(
            drafts.show(Some(Tab::Session("new".into())), String::new()),
            Some("saved request draft".into())
        );
        images.show(Some(Tab::Session("new".into())));
        assert_eq!(images.current(), &[PathBuf::from("/old.png")]);
        assert_eq!(
            drafts.show(Some(Tab::Session("other".into())), String::new()),
            Some("unrelated draft".into())
        );
        assert!(
            leaders
                .update(&snapshot(vec![record(logical, 1, "old")]))
                .is_empty()
        );
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

    #[test]
    fn leader_draft_recovery_uses_only_authoritative_identity_and_forward_generations() {
        use adroit_model::*;
        let leader = LogicalSessionId::new();
        let other = LogicalSessionId::new();
        let run = RunId::new();
        let record = |id, generation, terminal: Option<&str>| ManagedSession {
            binding: SessionBinding::leader(id, Some(run)),
            agent: Agent::Codex,
            generation,
            state: if terminal.is_some() {
                ManagedSessionState::Exited
            } else {
                ManagedSessionState::Preparing
            },
            permissions: AgentPermissions::Default,
            cwd: Some("/same-root".into()),
            terminal_id: terminal.map(String::from),
            conversation_id: None,
            readiness: Default::default(),
            spawned_at_ms: None,
        };
        let snapshot = |session| ControlSnapshot {
            sessions: vec![session],
            ..Default::default()
        };
        let mut cache = LeaderDrafts::default();
        assert!(
            cache
                .update(&snapshot(record(leader, 1, Some("old"))))
                .is_empty()
        );
        assert!(
            cache
                .update(&snapshot(record(other, 2, Some("unrelated"))))
                .is_empty()
        );
        assert!(cache.update(&snapshot(record(leader, 2, None))).is_empty());
        assert!(
            cache
                .update(&snapshot(record(leader, 1, Some("stale"))))
                .is_empty()
        );
        assert_eq!(
            cache.update(&snapshot(record(leader, 2, Some("new")))),
            vec![(Tab::Session("old".into()), Tab::Session("new".into()))]
        );
        assert!(
            cache
                .update(&snapshot(record(leader, 2, Some("new"))))
                .is_empty()
        );
        assert!(cache.update(&snapshot(record(leader, 4, None))).is_empty());
        assert!(
            cache
                .update(&snapshot(record(leader, 3, Some("reordered"))))
                .is_empty()
        );
        assert_eq!(
            cache.update(&snapshot(record(leader, 4, Some("latest")))),
            vec![(Tab::Session("new".into()), Tab::Session("latest".into()))]
        );
    }

    #[test]
    fn rebinding_a_restarted_leader_recovers_text_and_images_without_touching_another_request() {
        let old = Tab::Session("root-a-generation-1".into());
        let new = Tab::Session("root-a-generation-2".into());
        let other = Tab::Session("root-b".into());
        let mut drafts = Drafts::default();
        let mut images = crate::pasted::Attachments::default();
        drafts.show(Some(old.clone()), String::new());
        images.show(Some(old.clone()));
        images.add(vec!["/old.png".into()]);
        drafts.show(Some(other.clone()), "original request draft".into());
        images.show(Some(other.clone()));
        images.add(vec!["/other.png".into()]);
        drafts.show(Some(new.clone()), "unrelated request draft".into());
        images.show(Some(new.clone()));
        images.add(vec!["/new.png".into()]);
        drafts.show(Some(other.clone()), "new conversation draft".into());
        images.show(Some(other.clone()));

        assert_eq!(drafts.rebind(&old, &new, None), None);
        images.rebind(&old, &new);
        assert_eq!(images.current(), &[PathBuf::from("/other.png")]);
        assert_eq!(
            drafts.show(Some(new.clone()), "unrelated request draft".into()),
            Some("new conversation draft\n\noriginal request draft".into())
        );
        images.show(Some(new));
        assert_eq!(
            images.current(),
            &[PathBuf::from("/new.png"), PathBuf::from("/old.png")]
        );
        assert_eq!(
            drafts.show(Some(other), String::new()),
            Some("unrelated request draft".into())
        );
        assert!(!drafts.saved("").contains_key("session:root-a-generation-1"));
    }

    #[test]
    fn rebinding_merges_a_visible_target_buffer_and_does_not_duplicate_a_visible_source() {
        let old = Tab::Session("old".into());
        let new = Tab::Session("new".into());
        let mut drafts = Drafts::default();
        drafts.show(Some(old.clone()), String::new());
        assert_eq!(
            drafts.rebind(&old, &new, Some("old draft".into())),
            Some(String::new())
        );
        assert_eq!(
            drafts.show(Some(new.clone()), String::new()),
            Some("old draft".into())
        );
        drafts.show(Some(old.clone()), "target draft".into());
        drafts.show(Some(new.clone()), "recovered source".into());
        assert_eq!(
            drafts.rebind(&old, &new, Some("target draft".into())),
            Some("target draft\n\nrecovered source".into())
        );
    }

    fn session(id: &str) -> Option<Tab> {
        Some(Tab::Session(id.into()))
    }

    fn kept(prompts: &[&str]) -> Vec<String> {
        prompts.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn sent_prompts_are_recalled_newest_first_and_the_draft_comes_back() {
        let mut h = History::default();
        let sent = kept(&["one", "two"]);
        assert_eq!(h.step(true, "draft".into(), &sent), Some("two".into()));
        assert_eq!(h.step(true, "two".into(), &sent), Some("one".into()));
        assert_eq!(h.step(true, "one".into(), &sent), None);
        assert_eq!(h.step(false, "one".into(), &sent), Some("two".into()));
        assert_eq!(h.step(false, "two".into(), &sent), Some("draft".into()));
        assert_eq!(h.step(false, "draft".into(), &sent), None);
    }

    #[test]
    fn the_same_prompt_twice_in_a_row_is_recalled_once_and_nothing_sent_recalls_nothing() {
        let mut h = History::default();
        assert_eq!(h.step(true, String::new(), &[]), None);
        let sent = kept(&["again", "again\n"]);
        assert_eq!(h.step(true, String::new(), &sent), Some("again".into()));
        assert_eq!(h.step(true, "again".into(), &sent), None);
    }

    #[test]
    fn fewer_prompts_than_recalling_went_back_through_start_it_again() {
        let mut h = History::default();
        let sent = kept(&["one", "two", "three"]);
        h.step(true, "draft".into(), &sent);
        h.step(true, "three".into(), &sent);
        assert_eq!(h.step(false, "two".into(), &[]), None);
        assert_eq!(h.step(true, String::new(), &kept(&["x"])), Some("x".into()));
    }

    #[test]
    fn recalling_starts_again_from_the_newest_once_reset() {
        let mut h = History::default();
        let sent = kept(&["one", "two"]);
        h.step(true, "draft".into(), &sent);
        h.reset();
        assert_eq!(h.step(true, String::new(), &sent), Some("two".into()));
        assert_eq!(h.step(false, "two".into(), &sent), Some(String::new()));
    }

    #[test]
    fn what_is_written_with_nothing_open_goes_to_the_draft_opened_next() {
        let draft = Some(Tab::Draft {
            worktree: "/src".into(),
            id: Default::default(),
        });
        let mut d = Drafts::default();
        assert_eq!(
            d.show(draft.clone(), "add a readme".into()),
            Some("add a readme".into())
        );
        // Not to a session, which it was not written for.
        let mut d = Drafts::default();
        assert_eq!(d.show(session("a"), "add a readme".into()), None);
        assert_eq!(d.show(None, String::new()), Some("add a readme".into()));
    }

    #[test]
    fn what_is_written_survives_the_window_and_comes_back_to_its_tab() {
        let mut d = Drafts::default();
        d.show(session("a"), String::new());
        d.show(Some(Tab::Terminal(1)), "for a".into());
        let saved = d.saved("in the shell's prompt");
        let mut back = Drafts::restored(saved);
        assert_eq!(back.show(session("a"), String::new()), Some("for a".into()));
        assert_eq!(
            back.show(Some(Tab::Terminal(1)), String::new()),
            Some("in the shell's prompt".into())
        );
    }

    #[test]
    fn a_draft_tab_s_text_is_not_kept_past_the_window_but_nothing_open_s_is() {
        let draft = Some(Tab::Draft {
            worktree: "/src".into(),
            id: Default::default(),
        });
        let mut d = Drafts::default();
        d.show(draft, String::new());
        d.show(None, "for the draft".into());
        let mut back = Drafts::restored(d.saved("with nothing open"));
        assert!(back.holding().is_empty());
        // The window opens with nothing shown: its text comes back first.
        assert_eq!(back.first(), Some("with nothing open".into()));
        assert_eq!(back.first(), None);
    }

    #[test]
    fn a_tab_shown_before_the_prompt_opens_gets_its_text_once_it_does() {
        let mut d = Drafts::default();
        d.show(session("a"), String::new());
        d.show(None, "for a".into());
        let mut back = Drafts::restored(d.saved(""));

        back.turn_to(session("a"));

        assert_eq!(back.saved(""), d.saved(""));
        assert_eq!(back.first(), Some("for a".into()));
        assert_eq!(back.first(), None);
    }

    #[test]
    fn the_tabs_holding_text_aside_are_known_until_shown_again() {
        let mut d = Drafts::default();
        d.show(session("a"), String::new());
        d.show(session("b"), "for a".into());
        d.show(None, String::new());
        assert_eq!(d.holding(), HashSet::from([Tab::Session("a".into())]));
        d.show(session("a"), String::new());
        assert!(d.holding().is_empty());
    }

    #[test]
    fn each_tab_gets_back_what_was_written_for_it() {
        let mut d = Drafts::default();
        assert_eq!(d.show(session("a"), String::new()), None);
        assert_eq!(d.show(session("b"), "for a".into()), None);
        assert_eq!(d.show(session("a"), "for b".into()), Some("for a".into()));
        assert_eq!(d.show(session("b"), String::new()), Some("for b".into()));
    }

    #[test]
    fn hiding_the_last_session_keeps_its_prompt_and_images_for_reopening() {
        use crate::pasted::Attachments;

        let leader = session("leader");
        let mut drafts = Drafts::default();
        let mut images = Attachments::default();
        drafts.show(leader.clone(), String::new());
        images.show(leader.clone());
        images.add(vec!["/leader.png".into()]);

        assert_eq!(drafts.show(None, "unsent-view-draft".into()), None);
        images.show(None);
        assert!(images.current().is_empty());
        assert!(
            drafts
                .saved("")
                .values()
                .any(|text| text == "unsent-view-draft")
        );

        assert_eq!(
            drafts.show(leader.clone(), String::new()),
            Some("unsent-view-draft".into())
        );
        images.show(leader);
        assert_eq!(images.current(), &[PathBuf::from("/leader.png")]);
    }

    #[test]
    fn a_stopped_sessions_unsent_input_stays_saved_under_its_original_identity() {
        use crate::pasted::Attachments;

        let stopped = session("root-a");
        let other = session("root-b");
        let mut drafts = Drafts::default();
        let mut images = Attachments::default();
        drafts.show(other.clone(), String::new());
        images.show(other.clone());
        images.add(vec!["/root-b.png".into()]);
        drafts.show(stopped.clone(), "root-b draft".into());
        images.show(stopped.clone());
        images.add(vec!["/root-a.png".into()]);

        let visible = drafts
            .show(other.clone(), "preserve-final draft".into())
            .unwrap();
        images.show(other);
        assert_eq!(visible, "root-b draft");
        assert_eq!(images.current(), &[PathBuf::from("/root-b.png")]);
        let remaining = [Session {
            id: "root-b".into(),
            session_id: "conversation-b".into(),
            ..Default::default()
        }];
        drafts.follow(&remaining);
        images.follow(&remaining);
        let saved_text = drafts.saved(&visible);
        let saved_images = images.saved();
        assert_eq!(
            saved_text.get("session:root-a").map(String::as_str),
            Some("preserve-final draft")
        );
        assert_eq!(
            saved_text.get("session:root-b").map(String::as_str),
            Some("root-b draft")
        );
        assert_eq!(
            saved_images.get("session:root-a"),
            Some(&vec![PathBuf::from("/root-a.png")])
        );
        let mut recovered = Drafts::restored(saved_text);
        assert_eq!(
            recovered.show(stopped, String::new()),
            Some("preserve-final draft".into())
        );
    }

    #[test]
    fn hiding_a_session_keeps_both_sessions_prompts_and_attachments_separate() {
        use crate::pasted::Attachments;

        let leader = session("leader");
        let worker = session("worker");
        let mut drafts = Drafts::default();
        let mut images = Attachments::default();
        drafts.show(worker.clone(), String::new());
        images.show(worker.clone());
        images.add(vec!["/worker.png".into()]);
        drafts.show(leader.clone(), "worker draft".into());
        images.show(leader.clone());
        images.add(vec!["/leader.png".into()]);

        assert_eq!(
            drafts.show(worker, "leader draft".into()),
            Some("worker draft".into())
        );
        images.show(session("worker"));
        assert_eq!(images.current(), &[PathBuf::from("/worker.png")]);
        assert_eq!(
            drafts.show(leader.clone(), "edited worker draft".into()),
            Some("leader draft".into())
        );
        images.show(leader);
        assert_eq!(images.current(), &[PathBuf::from("/leader.png")]);
    }

    #[test]
    fn closing_an_ephemeral_draft_keeps_its_text_and_images_accessible_to_the_next_composer() {
        use crate::pasted::Attachments;

        for next in [None, session("existing")] {
            let draft = Some(Tab::draft("/repo"));
            let mut drafts = Drafts::default();
            let mut images = Attachments::default();
            drafts.show(next.clone(), String::new());
            images.show(next.clone());
            images.add(vec!["/next.png".into()]);
            drafts.show(draft.clone(), "next draft".into());
            images.show(draft);
            images.add(vec!["/unsent.png".into()]);

            let mut composer = "unsent new-session draft".to_string();
            if let Some(text) = drafts.close(next.clone()) {
                composer.push('\n');
                composer.push_str(&text);
            }
            images.close(next);
            assert!(composer.contains("unsent new-session draft"));
            assert!(
                drafts
                    .saved(&composer)
                    .values()
                    .any(|text| text.contains("unsent new-session draft"))
            );
            assert!(images.current().contains(&PathBuf::from("/unsent.png")));
            assert!(images.current().contains(&PathBuf::from("/next.png")));
        }
    }

    #[test]
    fn what_a_closed_tab_held_stays_and_the_next_tab_s_follows_it() {
        let mut d = Drafts::default();
        d.show(session("a"), String::new());
        d.show(session("b"), "for a".into());
        assert_eq!(d.close(session("a")), Some("for a".into()));
        assert_eq!(d.show(session("b"), String::new()), None);
    }

    #[test]
    fn lines_for_a_tab_not_shown_are_kept_after_its_text() {
        let mut d = Drafts::default();
        d.show(session("a"), String::new());
        d.show(session("b"), "one".into());
        assert!(d.add(&session("b"), &["here".into()]));
        assert!(!d.add(&session("a"), &["two".into(), "three".into()]));
        assert_eq!(
            d.show(session("a"), String::new()),
            Some("one\ntwo\nthree".into())
        );
    }

    #[test]
    fn a_session_s_text_follows_it_to_its_new_id() {
        let mut d = Drafts::default();
        d.show(session("old"), String::new());
        d.show(session("other"), "for old".into());
        let moved = Session {
            id: "new".into(),
            session_id: "old".into(),
            ..Default::default()
        };
        d.follow(&[moved]);
        assert_eq!(
            d.show(session("new"), String::new()),
            Some("for old".into())
        );
    }
}
