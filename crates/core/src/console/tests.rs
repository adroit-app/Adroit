use adroit_model::testing::worktree as wt;
use adroit_model::{Key, Session, State, TerminalSession, Wait, build};
use adroit_protocol::{Change, Event, Listed, Seq, TreeChange, TreeWork, WorkPatch, WorkSnapshot};

use super::*;
use crate::actions::Action;
use crate::content::Area;
use crate::effect::Effect;
use crate::keymap::Context;
use crate::msg::Msg;
use crate::scenario::Scenario;

fn session(id: &str) -> Session {
    Session {
        id: id.into(),
        name: format!("{id}-name"),
        cwd: format!("/r/{id}").into(),
        adroit_editor: true,
        ..Default::default()
    }
}

fn placed(sessions: Vec<Session>) -> Listed {
    let terminals = vec![TerminalSession {
        id: 1,
        name: "shell".into(),
        cwd: "/r/a".into(),
        ..Default::default()
    }];
    let found = sessions
        .iter()
        .map(|s| s.cwd.clone())
        .chain(Some("/r/a".into()))
        .map(|cwd| (cwd.clone(), Some(wt(cwd, "/r", "main"))))
        .collect();
    let tree = build(&sessions, &terminals, &found, &[], None);
    Listed {
        sessions,
        terminals,
        tree,
    }
}

fn listing(sessions: Vec<Session>) -> Event {
    Event::Snapshot(Box::new(WorkSnapshot {
        seq: Seq(1),
        tree: TreeWork {
            listed: Some(Box::new(placed(sessions))),
            ..Default::default()
        },
        ..Default::default()
    }))
}

fn relisted(seq: u64, sessions: Vec<Session>) -> Event {
    Event::Patch(WorkPatch {
        seq: Seq(seq),
        changes: vec![Change::Tree(TreeChange::Listed {
            listed: Box::new(placed(sessions)),
        })],
    })
}

fn tab(id: &str) -> Tab {
    Tab::Session(id.into())
}

/// A window listing the sessions `a` and `b`, showing `a`, its console
/// having the keys.
fn window() -> Scenario {
    Scenario::new()
        .event(listing(vec![session("a"), session("b")]))
        .send(Msg::ShowSession("a".into()))
        .send(Msg::Focus(Area::Console))
}

/// A window that can ask `a` for its input, which may hold text.
fn asking() -> Scenario {
    window().send_all([
        Msg::PromptRuns(true),
        Msg::EditorAnswers,
        Msg::InputMayHold(tab("a")),
    ])
}

fn with_pane() -> Probe {
    Probe {
        pane: true,
        ..Default::default()
    }
}

fn press(key: Key, probe: Probe) -> Msg {
    Msg::Key {
        key,
        probe: Box::new(probe),
    }
}

fn key(name: &str) -> Msg {
    press(Key::new(name), with_pane())
}

fn reading(normal: bool, line: usize, scrolls_itself: bool) -> Probe {
    Probe {
        pane: true,
        scrolls_itself,
        reader: Some(ReaderAt {
            normal,
            edge: Edge {
                line,
                lines: 40,
                page: 10,
            },
        }),
        inserting: false,
    }
}

#[test]
fn passing_every_key_sends_them_as_they_are_until_escape() {
    let run = window().send_all([Msg::ConsolePassEveryKey, key("x")]);
    assert_eq!(run.last_effects(), [Effect::PaneKey(Key::new("x"))]);
    assert!(run.state().console.stale.contains(&tab("a")));
    let run = run.send(press(Key::new("c").ctrl(), with_pane()));
    assert_eq!(run.last_effects(), [Effect::PaneKey(Key::new("Escape"))]);
    // What the keys typed into the session's input is asked for once they
    // stop going there.
    let run = run.send(key("Escape"));
    assert_eq!(run.last_effects(), [Effect::AskInputAfter(PULL_AFTER_FREE)]);
    assert!(!run.state().console.direct);
}

#[test]
fn text_pasted_into_the_session_is_asked_for_once_the_keys_leave_it() {
    let run = window().send_all([
        Msg::PromptRuns(true),
        Msg::EditorAnswers,
        Msg::ConsolePassEveryKey,
        Msg::Text {
            text: "pasted words".into(),
            pasted: true,
        },
    ]);
    assert_eq!(
        run.last_effects(),
        [Effect::PaneText {
            text: "pasted words".into(),
            pasted: true
        }]
    );
    let run = run.send(key("Escape")).send(Msg::AskInput);
    assert_eq!(run.last_effects(), [Effect::AskInput(tab("a"))]);
}

#[test]
fn passing_every_key_stops_without_a_program_to_pass_them_to() {
    let run = window().send_all([
        Msg::ConsolePassEveryKey,
        press(Key::new("x"), Probe::default()),
    ]);
    assert!(!run.state().console.direct);
}

#[test]
fn the_next_key_alone_goes_to_the_session_as_it_is() {
    let run = window().send_all([Msg::ConsoleSendNext, key("i")]);
    assert_eq!(run.last_effects(), [Effect::PaneKey(Key::new("i"))]);
    assert!(!run.state().console.send_next);
    let run = run.send(key("i"));
    assert!(matches!(
        run.last_effects(),
        [Effect::PerformKey(Action::PromptWrite, _)]
    ));
}

#[test]
fn a_key_sent_as_it_is_drops_a_request_for_the_session_s_input() {
    let run = asking()
        .send(Msg::InputAsked(tab("a")))
        .send(Msg::ConsoleSendNext)
        .send(key("x"));
    assert_eq!(run.state().console.pull, None);
}

#[test]
fn a_binding_waits_for_its_next_key_and_runs_with_the_key_that_ended_it() {
    let run = window().send(press(Key::new("q").ctrl(), with_pane()));
    assert_eq!(run.state().console.pending, [Key::new("q").ctrl()]);
    let run = run.send(key("s"));
    assert_eq!(
        run.last_effects(),
        [Effect::PerformKey(Action::WindowSplit, Key::new("s"))]
    );
    assert!(run.state().console.pending.is_empty());
}

#[test]
fn keys_begun_in_one_area_do_not_carry_over_to_another() {
    let run = window().send_all([
        Msg::ConsolePassEveryKey,
        Msg::ConsoleSendNext,
        press(Key::new("q").ctrl(), Probe::default()),
    ]);
    let run = run.send(Msg::Focus(Area::Sidebar));
    let console = &run.state().console;
    assert!(console.pending.is_empty());
    assert!(!console.send_next);
    assert!(!console.direct);
}

#[test]
fn an_unbound_key_in_a_session_s_console_starts_reading_it_with_the_keys_before() {
    let run = window().send(key("j"));
    assert_eq!(
        run.last_effects(),
        [Effect::ReadConsole(vec![Key::new("j")])]
    );
    let run = window().send_all([key("g"), key("g")]);
    assert_eq!(
        run.last_effects(),
        [Effect::ReadConsole(vec![Key::new("g"), Key::new("g")])]
    );
    let none = window().send(press(Key::new("j"), Probe::default()));
    assert!(none.last_effects().is_empty());
}

#[test]
fn read_the_console_takes_its_keys_and_leaves_with_its_own() {
    let run = || window().send(Msg::ReadingStarted);
    // (key, where the reader is, the program scrolls itself, effects)
    let cases = [
        (
            "j",
            reading(true, 5, false),
            vec![Effect::ReaderKey(Key::new("j"))],
        ),
        ("k", reading(true, 0, true), vec![Effect::PaneWheel(1)]),
        (
            "k",
            reading(true, 0, false),
            vec![Effect::ReaderKey(Key::new("k"))],
        ),
        (
            "q",
            reading(false, 0, true),
            vec![Effect::ReaderKey(Key::new("q"))],
        ),
        ("q", reading(true, 5, true), vec![Effect::ConsoleLive]),
    ];
    for (name, probe, want) in cases {
        let after = run().send(press(Key::new(name), probe.clone()));
        assert_eq!(after.last_effects(), want, "{name} {probe:?}");
    }
    let left = run().send(press(Key::new("q"), reading(true, 5, true)));
    assert!(!left.state().console.reading);
    let top = run()
        .send(press(Key::new("g"), reading(true, 5, true)))
        .send(press(Key::new("g"), reading(true, 5, true)));
    assert_eq!(
        top.last_effects(),
        [
            Effect::PaneScrollKey(Key::new("Home").ctrl()),
            Effect::ReaderKey(Key::new("g"))
        ]
    );
    // A global binding is the window's while the console is read.
    let split = run()
        .send(press(Key::new("q").ctrl(), reading(true, 5, true)))
        .send(press(Key::new("s"), reading(true, 5, true)));
    assert_eq!(
        split.last_effects(),
        [Effect::PerformKey(Action::WindowSplit, Key::new("s"))]
    );
}

#[test]
fn another_tab_shown_is_shown_live() {
    let run = window()
        .send(Msg::ReadingStarted)
        .send(Msg::ShowSession("b".into()));
    assert!(!run.state().console.reading);
}

#[test]
fn a_shell_takes_every_key_and_the_prompt_takes_those_it_does_not_bind() {
    let shell = window().send(Msg::Show {
        thing: Tab::Terminal(1),
        place: None,
        how: None,
    });
    let run = shell.send(key("i"));
    assert_eq!(run.last_effects(), [Effect::PaneKey(Key::new("i"))]);
    let prompt = window().send_all([Msg::Focus(Area::Prompt), key("x")]);
    assert_eq!(prompt.last_effects(), [Effect::PromptKey(Key::new("x"))]);
}

#[test]
fn the_keys_help_key_opens_the_help_for_where_the_keys_are() {
    let run = window().send_all([Msg::Focus(Area::Prompt), key("F1")]);
    assert_eq!(
        run.state().chrome.keys_help,
        Some(vec![Context::Prompt, Context::Global])
    );
}

#[test]
fn coming_into_the_prompt_asks_the_session_for_its_input_once() {
    let run = asking().send(Msg::Focus(Area::Prompt));
    assert_eq!(
        run.last_effects(),
        [Effect::ConsoleLive, Effect::AskInput(tab("a"))]
    );
    let run = run.send(Msg::InputAsked(tab("a")));
    assert_eq!(
        run.last_effects(),
        [Effect::LapsePull {
            pull: 1,
            after: PULL_WAIT
        }]
    );
    assert_eq!(run.state().console.pull, Some(tab("a")));
    let again = run
        .send(Msg::Focus(Area::Console))
        .send(Msg::Focus(Area::Prompt));
    assert_eq!(again.last_effects(), [Effect::ConsoleLive]);
}

#[test]
fn a_session_is_asked_for_its_input_only_when_it_can_hand_it_back() {
    let cases: Vec<(&str, Vec<Msg>)> = vec![
        ("no Neovim", vec![Msg::PromptRuns(false)]),
        ("passing keys", vec![Msg::ConsolePassEveryKey]),
        ("a dialog", vec![Msg::Dialog(true)]),
    ];
    for (why, msgs) in cases {
        let run = asking().send_all(msgs).send(Msg::AskInput);
        assert!(run.last_effects().is_empty(), "{why}");
    }
    let unread = window().send_all([Msg::PromptRuns(true), Msg::EditorAnswers, Msg::AskInput]);
    assert!(unread.last_effects().is_empty());
    let own_editor = asking()
        .event(relisted(
            2,
            vec![
                Session {
                    adroit_editor: false,
                    ..session("a")
                },
                session("b"),
            ],
        ))
        .send(Msg::AskInput);
    assert!(own_editor.last_effects().is_empty());
}

#[test]
fn a_request_lapses_unless_a_later_one_was_made() {
    let run = asking()
        .send(Msg::InputAsked(tab("a")))
        .send(Msg::InputMayHold(tab("a")))
        .send(Msg::InputAsked(tab("a")))
        .send(Msg::PullLapsed(1));
    assert_eq!(run.state().console.pull, Some(tab("a")));
    let run = run.send(Msg::PullLapsed(2));
    assert_eq!(run.state().console.pull, None);
    let taken = asking()
        .send(Msg::InputAsked(tab("a")))
        .send(Msg::InputTaken);
    assert_eq!(taken.state().console.pull, None);
}

#[test]
fn a_session_that_stops_taking_every_key_is_asked_for_its_input_a_moment_later() {
    let waiting = Session {
        state: State::Waiting(Wait::Permission),
        ..session("a")
    };
    let run = asking().event(relisted(2, vec![waiting, session("b")]));
    assert!(
        !run.last_effects()
            .contains(&Effect::AskInputAfter(PULL_AFTER_FREE))
    );
    let run = run.event(relisted(3, vec![session("a"), session("b")]));
    assert!(
        run.last_effects()
            .contains(&Effect::AskInputAfter(PULL_AFTER_FREE))
    );
}

#[test]
fn a_session_no_longer_listed_has_no_input_kept() {
    let run = asking().send(Msg::InputMayHold(tab("b")));
    let run = run.event(relisted(2, vec![session("a")]));
    assert!(run.state().console.stale.contains(&tab("a")));
    assert!(!run.state().console.stale.contains(&tab("b")));
}

#[test]
fn text_for_a_session_is_written_in_the_prompt_and_an_image_kept_for_it() {
    let text = |pasted| Msg::Text {
        text: "hi".into(),
        pasted,
    };
    let run = window().send(text(true));
    assert_eq!(run.state().content.focus, Area::Prompt);
    assert_eq!(
        run.last_effects().last(),
        Some(&Effect::PromptText {
            text: "hi".into(),
            pasted: true
        })
    );
    let direct = window().send_all([Msg::ConsolePassEveryKey, text(false)]);
    assert_eq!(
        direct.last_effects(),
        [Effect::PaneText {
            text: "hi".into(),
            pasted: false
        }]
    );
    let held = window().send_all([Msg::PickerOpen, text(false)]);
    assert!(held.last_effects().is_empty());
    let image = |run: Scenario| {
        run.send(Msg::ImagePasted {
            mime: "image/png".into(),
            data: "AA==".into(),
        })
    };
    assert_eq!(
        image(window()).last_effects(),
        [Effect::KeepImage {
            mime: "image/png".into(),
            data: "AA==".into()
        }]
    );
    let shell = window().send(Msg::Show {
        thing: Tab::Terminal(1),
        place: None,
        how: None,
    });
    assert_eq!(
        image(shell).last_effects(),
        [Effect::PaneText {
            text: String::new(),
            pasted: true
        }]
    );
}

#[test]
fn text_or_a_menu_bar_entry_breaks_off_a_sequence_begun() {
    let begun = || window().send(press(Key::new("q").ctrl(), with_pane()));
    let typed = begun().send(Msg::Text {
        text: "hi".into(),
        pasted: false,
    });
    assert!(typed.state().console.pending.is_empty());
    let menu = begun().send(Msg::SequenceDropped);
    assert!(menu.state().console.pending.is_empty());
}
