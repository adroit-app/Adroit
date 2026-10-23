use adroit_model::testing::worktree as wt;
use adroit_model::{Agent, Session, build};
use adroit_protocol::{
    Change, Event, HistoryRead, Listed, PromptChange, Request, Seq, TreeWork, WorkPatch,
    WorkSnapshot,
};

use crate::content::Tab;
use crate::effect::Effect;
use crate::msg::Msg;
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
