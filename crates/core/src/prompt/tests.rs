use adroit_model::testing::worktree as wt;
use adroit_model::{Agent, Session, build};
use adroit_protocol::{
    Change, Event, HistoryRead, Listed, PromptChange, Request, Seq, TreeWork, WorkPatch,
    WorkSnapshot,
};

use std::path::PathBuf;

use crate::content::Tab;
use crate::effect::Effect;
use crate::msg::Msg;
use crate::prompt::drafts::Drafts;
use crate::prompt::images::Attachments;
use crate::scenario::Scenario;

fn listing() -> Event {
    let sessions = vec![Session {
        id: "a".into(),
        name: "a".into(),
        cwd: "/r/a".into(),
        ..Default::default()
    }];
    let found = [("/r/a".into(), Some(wt("/r/a", "/r", "main")))]
        .into_iter()
        .collect();
    let tree = build(&sessions, &[], &found, &[], None);
    Event::Snapshot(Box::new(WorkSnapshot {
        seq: Seq(1),
        tree: TreeWork {
            listed: Some(Box::new(Listed {
                sessions,
                terminals: Vec::new(),
                tree,
            })),
            ..Default::default()
        },
        ..Default::default()
    }))
}

fn history(seq: u64, asked: u64, sent: &[&str]) -> Event {
    Event::Patch(WorkPatch {
        seq: Seq(seq),
        changes: vec![Change::Prompt(PromptChange::History {
            read: HistoryRead {
                asked,
                project: "/r/a".into(),
                sent: sent.iter().map(|s| s.to_string()).collect(),
            },
        })],
    })
}

fn recalled(run: &Scenario) -> Vec<Effect> {
    run.last_effects()
        .iter()
        .filter(|e| matches!(e, Effect::PromptRecall { .. }))
        .cloned()
        .collect()
}

fn recall(back: bool) -> Msg {
    Msg::Recall {
        back,
        agent: Agent::Claude,
    }
}

#[test]
fn stepping_through_the_prompts_sent_asks_the_host_for_the_project_s_and_puts_its_answer_in() {
    let run = Scenario::new()
        .event(listing())
        .send(Msg::ShowSession("a".into()))
        .send(recall(true));
    assert_eq!(
        run.last_effects(),
        [Effect::Host(Request::History {
            asked: 1,
            agent: Agent::Claude,
            project: "/r/a".into(),
        })]
    );
    // An answer for another request puts nothing in.
    let run = run.event(history(2, 7, &["old"]));
    assert!(
        !run.last_effects()
            .iter()
            .any(|e| matches!(e, Effect::PromptRecall { .. }))
    );
    let run = run.event(history(3, 1, &["fix it"]));
    assert_eq!(
        recalled(&run),
        [Effect::PromptRecall {
            tab: Some(Tab::Session("a".into())),
            back: true,
            sent: vec!["fix it".into()],
        }]
    );
    let again = run.send(recall(false)).event(history(4, 2, &["fix it"]));
    assert_eq!(
        recalled(&again),
        [Effect::PromptRecall {
            tab: Some(Tab::Session("a".into())),
            back: false,
            sent: vec!["fix it".into()],
        }]
    );
}

#[test]
fn with_no_project_there_is_nothing_to_step_through() {
    let run = Scenario::new().send(recall(true));
    assert_eq!(
        run.last_effects(),
        [Effect::PromptRecall {
            tab: None,
            back: true,
            sent: Vec::new(),
        }]
    );
}

fn session(id: &str) -> Option<Tab> {
    Some(Tab::Session(id.into()))
}

#[test]
fn hiding_the_last_session_keeps_its_prompt_and_images_for_reopening() {
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

fn shown_with_images() -> Scenario {
    Scenario::new()
        .event(listing())
        .send(Msg::ShowSession("a".into()))
        .send(Msg::Focus(crate::content::Area::Prompt))
        .send(Msg::PromptImages(vec![
            "/one.png".into(),
            "/two.png".into(),
        ]))
}

fn key(name: &str) -> Msg {
    Msg::Key {
        key: adroit_model::Key::new(name),
        probe: Box::default(),
    }
}

#[test]
fn the_images_attached_take_the_keys_once_one_is_selected() {
    let run = shown_with_images().send(Msg::ImagesChoose);
    assert_eq!(run.state().content.focus, crate::content::Area::Prompt);
    assert_eq!(run.state().prompt.selected, Some("/one.png".into()));
    let run = run.send(key("l"));
    assert_eq!(run.state().prompt.selected, Some("/two.png".into()));
    let run = run.send(key("x"));
    assert_eq!(run.last_effects(), [Effect::RemoveImage("/two.png".into())]);
    let run = run.send(key("Escape"));
    assert_eq!(run.state().prompt.selected, None);
    // With none selected, the key is the prompt's.
    let run = run.send(key("x"));
    assert_eq!(
        run.last_effects(),
        [Effect::PromptKey(adroit_model::Key::new("x"))]
    );
}

#[test]
fn images_attached_or_taken_away_leave_none_selected() {
    let run = shown_with_images()
        .send(Msg::ImageSelect(Some("/two.png".into())))
        .send(Msg::PromptImages(vec!["/one.png".into()]));
    assert_eq!(run.state().prompt.selected, None);
}
