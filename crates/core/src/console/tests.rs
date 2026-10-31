use crate::testing::here;
use adroit_model::{Key, Wait};
use adroit_protocol::{ConsoleChange, Dialog, Event};

use super::*;
use crate::actions::Action;
use crate::content::Area;
use crate::effect::Effect;
use crate::keymap::Context;
use crate::msg::Msg;
use crate::scenario::Scenario;
use crate::testing::{
    ListedBuilder, SessionBuilder, listed, patch, session, shell, show, snapshot,
};

/// The sessions listed with a shell in `a`'s worktree, each able to hand
/// its input back.
fn listing(sessions: impl IntoIterator<Item = SessionBuilder>) -> ListedBuilder {
    listed()
        .sessions(sessions.into_iter().map(|s| s.adroit_editor(true)))
        .terminal(shell(1, "/r/a"))
}

/// The host's `seq`th word: whether `session`'s screen shows a dialog.
fn dialog(seq: u64, session: &str, open: bool) -> Event {
    patch(seq)
        .change(ConsoleChange::Dialog {
            dialog: Dialog {
                session: session.into(),
                open,
            },
        })
        .into()
}

fn tab(id: &str) -> Tab {
    Tab::Session(here(), id.into())
}

/// A window listing the sessions `a` and `b`, showing `a`, its console
/// having the keys.
fn window() -> Scenario {
    Scenario::new()
        .event(snapshot().listed(listing([session("a"), session("b")])))
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
        ..Probe::default()
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
fn passing_every_key_stops_when_the_session_they_went_to_ends() {
    let run = window()
        .send_all([Msg::ConsolePassEveryKey, key("x")])
        .event(patch(2).listed(listing([session("b")])));
    assert_ne!(run.state().content.active(), Some(&tab("a")));
    assert_eq!(run.state().content.focus, Area::Console);
    assert!(!run.state().console.direct);
    let shown_next = window()
        .send(Msg::ConsolePassEveryKey)
        .send(Msg::ShowSession("b".into()));
    assert!(!shown_next.state().console.direct);
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
fn the_reader_quitting_shows_the_console_live_again() {
    let run = window().send(Msg::ReadingStarted).send(Msg::ReaderQuit);
    assert!(!run.state().console.reading);
    assert_eq!(run.last_effects(), [Effect::ConsoleLive]);
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
    let shell = window().send(show(Tab::Terminal(here(), 1)));
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
        ("a dialog", vec![Msg::Host(dialog(2, "a", true))]),
    ];
    for (why, msgs) in cases {
        let run = asking().send_all(msgs).send(Msg::AskInput);
        assert!(run.last_effects().is_empty(), "{why}");
    }
    let unread = window().send_all([Msg::PromptRuns(true), Msg::EditorAnswers, Msg::AskInput]);
    assert!(unread.last_effects().is_empty());
    let own_editor = asking()
        .event(
            patch(2).listed(
                listed()
                    .session(session("a"))
                    .session(session("b").adroit_editor(true))
                    .terminal(shell(1, "/r/a")),
            ),
        )
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
    let waiting = session("a").waiting(Wait::Permission);
    let run = asking().event(patch(2).listed(listing([waiting, session("b")])));
    assert!(
        !run.last_effects()
            .contains(&Effect::AskInputAfter(PULL_AFTER_FREE))
    );
    let run = run.event(patch(3).listed(listing([session("a"), session("b")])));
    assert!(
        run.last_effects()
            .contains(&Effect::AskInputAfter(PULL_AFTER_FREE))
    );
}

#[test]
fn keys_and_text_that_answer_a_waiting_session_leave_its_input_alone() {
    let waiting = session("a").waiting(Wait::Permission);
    let answering = |area| {
        window()
            .send_all([Msg::PromptRuns(true), Msg::EditorAnswers])
            .event(patch(2).listed(listing([waiting.clone(), session("b")])))
            .send(Msg::Focus(area))
    };
    for area in [Area::Console, Area::Prompt] {
        let run = answering(area).send(key("Enter"));
        assert_eq!(run.last_effects(), [Effect::PaneKey(Key::new("Enter"))]);
        let run = run.send(Msg::Text {
            text: "1".into(),
            pasted: false,
        });
        assert!(matches!(run.last_effects(), [Effect::PaneText { .. }]));
        assert!(run.state().console.stale.is_empty(), "{area:?}");
        // Answered, the session goes back to work with nothing to take back.
        let run = run
            .event(patch(3).listed(listing([session("a"), session("b")])))
            .send(Msg::AskInput);
        assert!(run.last_effects().is_empty(), "{area:?}");
    }
}

#[test]
fn a_working_session_is_asked_for_its_input_only_once_it_stops_working() {
    let working = session("a").working();
    let run = asking().event(patch(2).listed(listing([working, session("b")])));
    assert!(
        !run.last_effects()
            .contains(&Effect::AskInputAfter(PULL_AFTER_FREE))
    );
    let entered = run.send(Msg::Focus(Area::Prompt));
    assert_eq!(entered.last_effects(), [Effect::ConsoleLive]);
    assert!(entered.state().console.stale.contains(&tab("a")));
    let idle = entered.event(patch(3).listed(listing([session("a"), session("b")])));
    assert!(
        idle.last_effects()
            .contains(&Effect::AskInputAfter(PULL_AFTER_FREE))
    );
    let asked = idle.send(Msg::AskInput);
    assert_eq!(asked.last_effects(), [Effect::AskInput(tab("a"))]);
}

#[test]
fn a_session_no_longer_listed_has_no_input_kept() {
    let run = asking().send(Msg::InputMayHold(tab("b")));
    let run = run.event(patch(2).listed(listing([session("a")])));
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
        image(window()).last_effects().last(),
        Some(&Effect::KeepImage {
            mime: "image/png".into(),
            data: "AA==".into()
        })
    );
    let shell = window().send(Msg::Show {
        thing: Tab::Terminal(here(), 1),
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
fn ctrl_enter_reaches_the_prompt_after_an_image_pasted_while_the_console_had_the_keys() {
    let run = window()
        .send(Msg::ImagePasted {
            mime: "image/png".into(),
            data: "AA==".into(),
        })
        .send(Msg::PromptImages(vec!["/kept.png".into()]));
    assert_eq!(run.state().content.focus, Area::Prompt);
    let run = run.send(press(Key::new("Enter").ctrl(), with_pane()));
    assert_eq!(
        run.last_effects(),
        [Effect::PromptKey(Key::new("Enter").ctrl())]
    );
}

#[test]
fn image_files_dropped_for_a_session_are_kept_for_its_prompt_as_a_pasted_image_is() {
    let dropped = |paths: &[&str]| Msg::FilesDropped(paths.iter().map(Into::into).collect());
    let run = window().send(dropped(&["/a.PNG", "/notes.txt", "/b.jpeg", "/c"]));
    assert_eq!(run.state().content.focus, Area::Prompt);
    assert_eq!(
        run.last_effects().last(),
        Some(&Effect::KeepImageFiles(vec![
            "/a.PNG".into(),
            "/b.jpeg".into()
        ]))
    );
    for (why, run) in [
        ("no image", window().send(dropped(&["/notes.txt"]))),
        (
            "passing every key",
            window().send_all([Msg::ConsolePassEveryKey, dropped(&["/a.png"])]),
        ),
        (
            "the picker open",
            window().send_all([Msg::PickerOpen, dropped(&["/a.png"])]),
        ),
        (
            "a shell shown",
            window().send_all([
                Msg::Show {
                    thing: Tab::Terminal(here(), 1),
                    place: None,
                    how: None,
                },
                Msg::Focus(Area::Console),
                dropped(&["/a.png"]),
            ]),
        ),
    ] {
        assert!(run.last_effects().is_empty(), "{why}");
        assert_eq!(run.state().content.focus, Area::Console, "{why}");
    }
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

#[test]
fn a_dialog_is_open_while_the_host_saw_one_on_the_screen_of_the_session_shown() {
    let run = window().event(dialog(2, "a", true));
    assert!(run.state().console.dialog);
    assert!(waits(run.state()));
    let run = run.send(Msg::ShowSession("b".into()));
    assert!(!run.state().console.dialog, "b's screen was not looked at");
    let run = run.event(dialog(3, "b", false));
    assert!(!run.state().console.dialog);
    let run = run.event(dialog(4, "b", true));
    assert!(run.state().console.dialog);
}

#[test]
fn a_command_key_runs_the_window_s_binding_while_a_dialog_takes_every_key() {
    let stop = || crate::keymap::Chord::parse("Mod+Backspace").unwrap().key();
    let inserting = Probe {
        pane: true,
        inserting: true,
        ..Default::default()
    };
    let cases = [
        ("console", Area::Console, with_pane()),
        ("prompt", Area::Prompt, with_pane()),
        ("inserting", Area::Prompt, inserting),
    ];
    for (why, area, probe) in cases {
        let run = window()
            .event(dialog(2, "a", true))
            .send(Msg::Focus(area))
            .send(press(stop(), probe));
        assert_eq!(
            run.last_effects(),
            [Effect::PerformKey(Action::SessionStop, stop())],
            "{why}"
        );
    }
}

fn ctrl_q() -> Msg {
    press(Key::new("q").ctrl(), with_pane())
}

/// The window with the user's own bindings in force.
fn bound(text: &str) -> Scenario {
    window().tell(adroit_protocol::ConfigChange::Keys {
        file: crate::config::testing::keys_file(text),
    })
}

fn performed(run: &Scenario) -> Vec<Action> {
    run.effects()
        .iter()
        .filter_map(|e| match e {
            Effect::PerformKey(a, _) => Some(a.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_last_key_of_a_sequence_stepping_through_tabs_steps_again_alone() {
    let run = window().send_all([ctrl_q(), key("g"), key("t"), key("t")]);
    assert_eq!(
        run.last_effects(),
        [Effect::PerformKey(Action::TabNext, Key::new("t"))]
    );
    // Another key the same keys begin a repeating binding with steps too.
    let run = run.send(key("T"));
    assert_eq!(
        run.last_effects(),
        [Effect::PerformKey(Action::TabPrev, Key::new("T"))]
    );
}

#[test]
fn the_user_s_own_sequence_repeats_by_its_last_key_as_well() {
    let run = bound("[global]\n\"Ctrl+q ]\" = \"tab.next\"\n\"Ctrl+q [\" = \"tab.prev\"\n")
        .send_all([ctrl_q(), key("]"), key("]"), key("]"), key("[")]);
    assert_eq!(
        performed(&run),
        [
            Action::TabNext,
            Action::TabNext,
            Action::TabNext,
            Action::TabPrev
        ]
    );
}

#[test]
fn a_key_that_repeats_nothing_ends_the_repeating_and_does_what_it_does() {
    let run = window().send_all([ctrl_q(), key("}"), key("s")]);
    // `s` alone is no binding of the console: it starts reading it.
    assert_eq!(
        run.last_effects(),
        [Effect::ReadConsole(vec![Key::new("s")])]
    );
    assert_eq!(run.state().console.repeat, None);
    let run = run.send(key("}"));
    assert_eq!(performed(&run), [Action::WorkspaceNext]);
}

#[test]
fn a_sequence_that_does_not_step_through_anything_does_not_repeat() {
    let run = window().send_all([ctrl_q(), key("s"), key("s")]);
    assert_eq!(performed(&run), [Action::WindowSplit]);
    assert_eq!(run.state().console.repeat, None);
}

#[test]
fn text_typed_ends_the_repeating() {
    let run = window().send_all([
        ctrl_q(),
        key("}"),
        Msg::Text {
            text: "}".into(),
            pasted: false,
        },
    ]);
    assert_eq!(run.state().console.repeat, None);
}

#[test]
fn the_keys_repeat_while_the_last_key_is_pressed_again() {
    let run = window().send_all([ctrl_q(), key("}")]);
    assert_eq!(run.state().console.repeat, Some(vec![Key::new("q").ctrl()]));
}
