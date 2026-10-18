//! What is written in the prompt for each tab. The prompt holds the shown
//! tab's text; the others' is kept aside until their tab is shown again.

use std::collections::HashMap;
use std::sync::Arc;

use adroit_model::Session;
use adroit_nvim::Embedded;
use dioxus::prelude::*;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;

use crate::nav;
use crate::tabs::Tab;

/// The text kept for the tabs not shown, and which tab the prompt is for.
#[derive(Debug, Default)]
pub struct Drafts {
    shown: Option<Tab>,
    kept: HashMap<Option<Tab>, String>,
}

impl Drafts {
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

/// What the prompt is asked to do, one after another.
pub enum Op {
    Show(Option<Tab>),
    Close(Option<Tab>),
    /// Lines for a tab; answered with whether the prompt took them.
    Add(Option<Tab>, Vec<String>, Option<oneshot::Sender<bool>>),
    Follow(Vec<Session>),
    /// The shown tab's text, with the tab it was written for, leaving the
    /// prompt empty.
    Take(oneshot::Sender<(Option<Tab>, String)>),
    /// A prompt was sent: recalling starts again from the newest.
    Sent,
    /// Put the prompt sent one before, or one after when false, among those
    /// given oldest first, in the prompt, if it still shows the tab they were
    /// read for.
    Recall(Option<Tab>, bool, Vec<String>),
}

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
pub fn keeper(nvim: Signal<Option<Arc<Embedded>>>) -> Prompts {
    let (tx, mut ops) = unbounded_channel();
    spawn(async move {
        let mut drafts = Drafts::default();
        let mut history = History::default();
        while let Some(op) = ops.recv().await {
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
            match op {
                Op::Show(now) => {
                    if now == drafts.shown {
                        continue;
                    }
                    history.reset();
                    let taken = match &nv {
                        Some(nv) => nv.take_text().await.unwrap_or_default(),
                        None => String::new(),
                    };
                    if let Some(text) = drafts.show(now, taken) {
                        put(text).await;
                    }
                }
                Op::Close(now) => {
                    history.reset();
                    if let Some(text) = drafts.close(now) {
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
                Op::Follow(sessions) => drafts.follow(&sessions),
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
                    let _ = reply.send((drafts.shown.clone(), text));
                }
            }
        }
    });
    Prompts(CopyValue::new(tx))
}

#[cfg(test)]
mod tests {
    use super::*;

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
            role: crate::tabs::Role::Worker,
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
    fn each_tab_gets_back_what_was_written_for_it() {
        let mut d = Drafts::default();
        assert_eq!(d.show(session("a"), String::new()), None);
        assert_eq!(d.show(session("b"), "for a".into()), None);
        assert_eq!(d.show(session("a"), "for b".into()), Some("for a".into()));
        assert_eq!(d.show(session("b"), String::new()), Some("for b".into()));
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
