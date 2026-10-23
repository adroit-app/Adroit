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

mod sending {
    use adroit_model::{Agent, Session, State, Wait};

    use super::*;
    use crate::content::Area;
    use crate::prompt::send::{COMPOSE_WAIT, Taken};
    use adroit_protocol::{Composed, Typing};

    fn listed(state: State) -> Event {
        let sessions = vec![Session {
            id: "a".into(),
            name: "a".into(),
            cwd: "/r/a".into(),
            state,
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

    fn written(tab: Tab, text: &str) -> Taken {
        Taken {
            tab: Some(tab),
            text: text.into(),
            agent: Agent::Claude,
            ..Default::default()
        }
    }

    /// The steps the host composes in these tests.
    fn typed() -> Vec<Typing> {
        vec![Typing::Key {
            name: "Enter".into(),
        }]
    }

    /// A host that composes each prompt as a shell line or not, with an
    /// image missing or not.
    fn composing(shell: bool, image_missing: bool) -> Scenario {
        let mut seq = 1;
        Scenario::answering(move |request| match request {
            Request::Compose { asked, .. } => {
                seq += 1;
                vec![Event::Patch(WorkPatch {
                    seq: Seq(seq),
                    changes: vec![Change::Prompt(PromptChange::Composed {
                        composed: Composed {
                            asked: *asked,
                            shell,
                            image_missing,
                            typing: typed(),
                        },
                    })],
                })]
            }
            _ => Vec::new(),
        })
    }

    fn sending(state: State, tab: Tab) -> Scenario {
        sending_with(state, tab, false, false)
    }

    fn sending_with(state: State, tab: Tab, shell: bool, image_missing: bool) -> Scenario {
        composing(shell, image_missing)
            .event(listed(state))
            .send(Msg::Show {
                thing: tab,
                place: None,
                how: None,
            })
            .send(Msg::PromptSend)
    }

    fn notices(run: &Scenario) -> Vec<String> {
        run.last_effects()
            .iter()
            .filter_map(|e| match e {
                Effect::LapseNotice { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    /// What the last message asked of the host and the window to send.
    fn sends(run: &Scenario) -> Vec<Effect> {
        run.last_effects()
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Effect::Host(Request::Compose { .. })
                        | Effect::SendPrompt(_)
                        | Effect::StartDraft(_)
                )
            })
            .cloned()
            .collect()
    }

    fn restored(run: &Scenario) -> bool {
        run.last_effects()
            .iter()
            .any(|e| matches!(e, Effect::PromptRestore { .. }))
    }

    #[test]
    fn a_prompt_is_taken_once_while_it_is_on_its_way() {
        let run = sending(State::Idle, Tab::Session("a".into()));
        assert_eq!(run.last_effects(), [Effect::TakePrompt]);
        let again = run.send(Msg::PromptSend);
        assert!(again.last_effects().is_empty());
        let empty = again.send(Msg::PromptEmpty).send(Msg::PromptSend);
        assert_eq!(empty.last_effects(), [Effect::TakePrompt]);
        let shell = sending(State::Idle, Tab::Terminal(1));
        assert!(shell.last_effects().is_empty());
    }

    #[test]
    fn a_prompt_that_cannot_go_is_put_back_with_a_word_why() {
        let session = Tab::Session("a".into());
        let draft = Tab::draft("/r/a");
        let image = |t: Taken| Taken {
            images: vec!["/a.png".into()],
            ..t
        };
        // (session's state, taken, the host composes a shell line, an image
        // missing, the notice)
        let cases = [
            (
                State::Waiting(Wait::Permission),
                written(session.clone(), "go"),
                false,
                false,
                "Answer the session above first; your message waits here",
            ),
            (
                State::Idle,
                image(written(session.clone(), "!ls")),
                true,
                false,
                "Images cannot be sent with a shell command",
            ),
            (
                State::Idle,
                image(written(session.clone(), "see")),
                false,
                true,
                "An attached image is unavailable; remove it or paste it again",
            ),
            (
                State::Idle,
                written(draft.clone(), "name: fixer\n"),
                false,
                false,
                "Write what the session is to do below its name",
            ),
            (
                State::Idle,
                written(draft.clone(), "!ls"),
                true,
                false,
                "A shell line runs in a session: start it with a message first",
            ),
        ];
        for (state, taken, shell, missing, notice) in cases {
            let tab = taken.tab.clone().unwrap();
            let run = sending_with(state, tab, shell, missing)
                .send(Msg::PromptTaken(Box::new(taken.clone())));
            assert_eq!(notices(&run), [notice.to_string()], "{taken:?}");
            assert!(restored(&run), "{taken:?}");
            assert_eq!(run.state().prompt.sending, None);
        }
        let waited = sending(State::Waiting(Wait::Permission), session.clone())
            .send(Msg::PromptTaken(Box::new(written(session, "go"))));
        assert_eq!(waited.state().content.focus, Area::Console);
    }

    #[test]
    fn a_prompt_for_a_session_is_handed_over_and_put_back_if_it_did_not_go() {
        let session = Tab::Session("a".into());
        let taken = written(session.clone(), "fix it");
        let run = sending(State::Idle, session).send(Msg::PromptTaken(Box::new(taken.clone())));
        let composed = Taken {
            typing: typed(),
            ..taken.clone()
        };
        assert_eq!(
            sends(&run),
            [
                Effect::Host(Request::Compose {
                    asked: 1,
                    agent: Agent::Claude,
                    text: "fix it".into(),
                    images: Vec::new(),
                }),
                Effect::SendPrompt(Box::new(composed)),
            ]
        );
        assert_eq!(
            run.send(Msg::PromptWent(true)).last_effects(),
            [Effect::PromptSent]
        );
        let failed = sending(State::Idle, Tab::Session("a".into()))
            .send(Msg::PromptTaken(Box::new(taken.clone())))
            .send(Msg::PromptWent(false));
        assert_eq!(
            failed.last_effects(),
            [Effect::PromptRestore {
                tab: taken.tab,
                text: taken.text,
                images: Vec::new(),
            }]
        );
    }

    #[test]
    fn a_prompt_the_host_does_not_compose_in_time_is_put_back_with_a_word_why() {
        let session = Tab::Session("a".into());
        let taken = written(session.clone(), "fix it");
        let silent = || {
            Scenario::answering(|_| Vec::new())
                .event(listed(State::Idle))
                .send(Msg::Show {
                    thing: session.clone(),
                    place: None,
                    how: None,
                })
                .send(Msg::PromptSend)
                .send(Msg::PromptTaken(Box::new(taken.clone())))
        };
        assert!(silent().last_effects().contains(&Effect::LapseCompose {
            asked: 1,
            after: COMPOSE_WAIT,
        }));
        let lapsed = silent().send(Msg::ComposeLapsed(1));
        assert_eq!(
            notices(&lapsed),
            ["The work host did not answer; your message waits here".to_string()]
        );
        assert!(restored(&lapsed));
        assert_eq!(lapsed.state().prompt.sending, None);
        // One composed in time, or asked before, lapses without a word.
        let composed = sending(State::Idle, Tab::Session("a".into()))
            .send(Msg::PromptTaken(Box::new(taken.clone())))
            .send(Msg::ComposeLapsed(1));
        assert!(composed.last_effects().is_empty());
        let earlier = silent().send(Msg::ComposeLapsed(0));
        assert!(earlier.last_effects().is_empty());
    }

    #[test]
    fn a_draft_starts_a_session_named_by_its_prompt_or_the_name_above_it() {
        let draft = Tab::draft("/r/a");
        // (written, the name, the prompt it starts with)
        let cases = [
            ("name: fixer\nfix the tests", "fixer", "fix the tests"),
            ("a", "a-2", "a"),
        ];
        for (text, name, prompt) in cases {
            let run = sending(State::Idle, draft.clone())
                .send(Msg::PromptTaken(Box::new(written(draft.clone(), text))));
            let sends = sends(&run);
            let [
                Effect::Host(Request::Compose { .. }),
                Effect::StartDraft(start),
            ] = sends.as_slice()
            else {
                panic!("{text:?}: {:?}", run.last_effects());
            };
            assert_eq!(start.draft, draft, "{text:?}");
            assert_eq!(start.name, name, "{text:?}");
            assert_eq!(start.text, prompt, "{text:?}");
        }
    }
}

#[test]
fn the_prompt_shows_what_is_written_for_what_the_focused_window_shows() {
    let shown = |run: &Scenario| {
        run.last_effects()
            .iter()
            .filter_map(|e| match e {
                Effect::PromptShow(tab) => Some(tab.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let run = Scenario::new().send(Msg::Focus(crate::content::Area::Console));
    assert!(shown(&run).is_empty());
    let run = run.event(listing()).send(Msg::ShowSession("a".into()));
    assert_eq!(shown(&run), [Some(Tab::Session("a".into()))]);
    let run = run.send(Msg::Focus(crate::content::Area::Prompt));
    assert!(shown(&run).is_empty());
}

#[test]
fn the_prompt_s_neovim_is_offered_the_sessions_and_the_middle_s_files_each_time_they_change() {
    use crate::complete::Offer;
    use adroit_protocol::{PlaceChange, PlaceRead};
    let offers = |run: &Scenario| -> Vec<Effect> {
        run.last_effects()
            .iter()
            .filter(|e| matches!(e, Effect::PromptOffer(_) | Effect::PromptRoot(_)))
            .cloned()
            .collect()
    };
    let shown = Scenario::new()
        .event(listing())
        .send(Msg::ShowSession("a".into()));
    assert!(offers(&shown).is_empty(), "no Neovim runs to offer them");
    let runs = shown.send(Msg::PromptRuns(true));
    assert_eq!(
        offers(&runs),
        [
            Effect::PromptOffer(vec![Offer::session("a")]),
            Effect::PromptRoot(Some("/r/a".into())),
        ]
    );
    let read = runs.event(Event::Patch(WorkPatch {
        seq: Seq(2),
        changes: vec![Change::Place(PlaceChange::Read {
            read: PlaceRead {
                worktree: "/r/a".into(),
                files: vec!["src/lib.rs".into()],
                jj: false,
            },
        })],
    }));
    assert_eq!(
        offers(&read),
        [Effect::PromptOffer(vec![
            Offer::session("a"),
            Offer::file("src/lib.rs"),
        ])]
    );
    let typed = read.send(Msg::ConsoleTyped);
    assert!(offers(&typed).is_empty());
    // A Neovim started again is offered them again.
    let again = typed
        .send(Msg::PromptRuns(false))
        .send(Msg::PromptRuns(true));
    assert_eq!(offers(&again).len(), 2);
}

fn commands(seq: u64, dir: &str, names: &[&str]) -> Event {
    use adroit_protocol::CommandsRead;
    Event::Patch(WorkPatch {
        seq: Seq(seq),
        changes: vec![Change::Prompt(PromptChange::Commands {
            read: CommandsRead {
                agent: Agent::Claude,
                dir: dir.into(),
                commands: names
                    .iter()
                    .map(|n| adroit_model::Command {
                        name: (*n).into(),
                        description: String::new(),
                        argument_hint: String::new(),
                    })
                    .collect(),
            },
        })],
    })
}

fn commands_offered(run: &Scenario) -> Vec<Vec<String>> {
    run.effects()
        .iter()
        .filter_map(|e| match e {
            Effect::PromptCommands(list) => Some(list.iter().map(|c| c.name.clone()).collect()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_host_is_asked_for_the_commands_of_the_agent_and_directory_shown() {
    use adroit_protocol::CommandsSight;
    let run = Scenario::new()
        .event(listing())
        .send(Msg::ShowSession("a".into()));
    assert_eq!(
        run.state().host.watched.commands,
        Some(CommandsSight {
            agent: Agent::Claude,
            dir: "/r/a".into(),
        })
    );
    let run = run.send(Msg::CommandsWanted);
    assert_eq!(run.last_effects(), [Effect::Host(Request::CommandsWanted)]);
}

#[test]
fn the_prompt_is_offered_the_commands_read_for_what_is_shown_once_each() {
    let run = Scenario::new()
        .event(listing())
        .send(Msg::ShowSession("a".into()))
        .send(Msg::PromptRuns(true))
        .event(commands(2, "/elsewhere", &["stale"]))
        .event(commands(3, "/r/a", &["compact"]))
        .event(commands(4, "/r/a", &["compact"]))
        .event(commands(5, "/r/a", &["compact", "review"]));
    assert_eq!(
        commands_offered(&run),
        [
            vec!["compact".to_string()],
            vec!["compact".into(), "review".into()]
        ]
    );
    // Neovim started again is offered them again.
    let run = run.send(Msg::PromptRuns(false)).send(Msg::PromptRuns(true));
    assert_eq!(
        commands_offered(&run)[2..],
        [vec!["compact".to_string(), "review".into()]]
    );
}
