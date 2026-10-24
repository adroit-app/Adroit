use adroit_model::{Key, Process, Wait};

use super::*;
use crate::actions::Action;
use crate::content::Area;
use crate::effect::Effect;
use crate::keymap::Context;
use crate::layout::Side;
use crate::msg::Msg;
use crate::scenario::Scenario;
use crate::testing::{listing, session, session_tab};
use crate::tree::nav::Item;

/// A window listing the sessions `a` and `b`, showing `a`.
fn window() -> Scenario {
    Scenario::new()
        .event(listing([session("a"), session("b")]))
        .send(Msg::ShowSession("a".into()))
}

fn held(name: &str) -> Msg {
    Msg::HeldKey(Key::new(name))
}

fn menu_on(target: Target) -> Msg {
    Msg::MenuOpen {
        x: 1.0,
        y: 2.0,
        target,
    }
}

fn held_by(run: &Scenario) -> (Option<Holder>, bool, bool) {
    let state = run.state();
    let held = holding(&state.overlays, &state.picker, &state.chrome);
    (
        held.first().copied(),
        takes_text_away(&held),
        holds_menu_bar(&held),
    )
}

#[test]
fn what_is_drawn_over_the_window_holds_the_keys_the_text_and_the_menu_bar() {
    // (opened, what holds the keys, text goes nowhere, the menu bar waits)
    let cases = [
        (vec![], (None, false, false)),
        (
            vec![menu_on(Target::Tab(session_tab("a")))],
            (Some(Holder::Menu), false, false),
        ),
        (
            vec![Msg::KeysHelpOpen(vec![Context::Global])],
            (Some(Holder::KeysHelp), true, false),
        ),
        (vec![Msg::PickerOpen], (Some(Holder::Picker), true, true)),
        (vec![Msg::ResumeOpen], (Some(Holder::Picker), true, true)),
        (
            vec![Msg::RenameStart {
                id: "a".into(),
                place: RenamePlace::Tab,
            }],
            (Some(Holder::Rename), true, true),
        ),
        (
            vec![Msg::Confirm(Box::new(Ask::RestartDaemon))],
            (Some(Holder::Confirm), true, true),
        ),
        (
            vec![
                Msg::Confirm(Box::new(Ask::RestartDaemon)),
                menu_on(Target::Tab(session_tab("a"))),
            ],
            (Some(Holder::Menu), true, true),
        ),
    ];
    for (opened, want) in cases {
        let run = window().send_all(opened.clone());
        assert_eq!(held_by(&run), want, "{opened:?}");
    }
}

#[test]
fn a_key_closes_a_menu_left_open_and_does_nothing_else() {
    let run = window()
        .send(menu_on(Target::Tab(session_tab("a"))))
        .send(held("y"));
    assert_eq!(run.state().overlays.menu, None);
    assert!(run.last_effects().is_empty());
}

fn stop_a() -> Effect {
    Effect::Host(Request::Stop {
        session: Box::new(session("a").build()),
    })
}

#[test]
fn a_question_is_answered_by_its_keys_and_holds_every_other() {
    let stop = Ask::Stop(session("a").build());
    let process = Process {
        pid: 7,
        name: "node".into(),
        ..Default::default()
    };
    let kill = Ask::KillProcess(session("a").build(), process.clone());
    // (asked, key, what answering asks done, whether it is still asked)
    let cases = [
        (stop.clone(), "y", Some(stop_a()), false),
        (stop.clone(), "Enter", Some(stop_a()), false),
        (
            kill,
            "y",
            Some(Effect::Host(Request::Kill {
                session: Box::new(session("a").build()),
                process,
            })),
            false,
        ),
        (Ask::RestartDaemon, "y", Some(Effect::RestartDaemon), false),
        (stop.clone(), "n", None, false),
        (stop.clone(), "Escape", None, false),
        (stop, "x", None, true),
    ];
    for (ask, key, want, asked) in cases {
        let run = window()
            .send(Msg::Confirm(Box::new(ask.clone())))
            .send(held(key));
        assert_eq!(run.last_effects().first().cloned(), want, "{ask:?} {key}");
        assert_eq!(
            run.state().overlays.confirm.is_some(),
            asked,
            "{ask:?} {key}"
        );
    }
}

#[test]
fn a_click_answers_as_the_keys_do() {
    let run = window()
        .send(Msg::Confirm(Box::new(Ask::RestartDaemon)))
        .send(Msg::ConfirmAnswer(true));
    assert_eq!(run.last_effects(), [Effect::RestartDaemon]);
    let run = window()
        .send(Msg::Confirm(Box::new(Ask::RestartDaemon)))
        .send(Msg::ConfirmAnswer(false));
    assert!(run.last_effects().is_empty());
    assert_eq!(run.state().overlays.confirm, None);
}

#[test]
fn ending_a_session_asks_of_the_selected_row_s_or_else_the_shown_one() {
    // (the row selected, from the tree, the session asked of)
    let cases = [
        (Some(Item::Session("b".into())), true, Some("b")),
        (Some(Item::Waiting("b".into())), true, Some("b")),
        (Some(Item::Session("b".into())), false, Some("a")),
        (None, true, Some("a")),
    ];
    for (row, selection, want) in cases {
        let run = window()
            .send(Msg::TreeSelect(row.clone()))
            .send(Msg::ConfirmStop { selection });
        let asked = match &run.state().overlays.confirm {
            Some(Ask::Stop(s)) => Some(s.id.as_str()),
            _ => None,
        };
        assert_eq!(asked, want, "{row:?} {selection}");
    }
    let nothing = Scenario::new().send(Msg::ConfirmStop { selection: false });
    assert_eq!(nothing.state().overlays.confirm, None);
}

fn renaming(id: &str, place: RenamePlace) -> Msg {
    Msg::RenameStart {
        id: id.into(),
        place,
    }
}

fn typed(id: &str, place: RenamePlace, text: &str) -> Msg {
    Msg::RenameText {
        id: id.into(),
        place,
        text: text.into(),
    }
}

fn ended(id: &str, place: RenamePlace, commit: bool) -> Msg {
    Msg::RenameEnd {
        id: id.into(),
        place,
        commit,
    }
}

#[test]
fn a_name_typed_and_kept_renames_the_session() {
    let run = window()
        .send(renaming("a", RenamePlace::Tab))
        .send(typed("a", RenamePlace::Tab, "new"))
        .send(ended("a", RenamePlace::Tab, true));
    assert_eq!(run.state().overlays.rename, None);
    let Some(Effect::Host(Request::Rename { id, name, was, .. })) = run.last_effects().first()
    else {
        panic!("{:?}", run.last_effects());
    };
    assert_eq!(
        (id.as_str(), name.as_str(), was.as_str()),
        ("a", "new", "a-name")
    );
}

#[test]
fn a_name_given_up_unchanged_or_for_another_field_renames_nothing() {
    // (typed, ended, still renaming)
    let cases = [
        (
            typed("a", RenamePlace::Tab, "new"),
            ended("a", RenamePlace::Tab, false),
            false,
        ),
        (
            typed("a", RenamePlace::Tab, "a-name"),
            ended("a", RenamePlace::Tab, true),
            false,
        ),
        (
            typed("a", RenamePlace::Tab, "new"),
            ended("a", RenamePlace::Sidebar, true),
            true,
        ),
        (
            typed("b", RenamePlace::Tab, "new"),
            ended("b", RenamePlace::Tab, true),
            true,
        ),
    ];
    for (text, end, still) in cases {
        let run = window()
            .send(renaming("a", RenamePlace::Tab))
            .send(text.clone())
            .send(end.clone());
        assert!(
            !run.last_effects()
                .iter()
                .any(|e| matches!(e, Effect::Host(Request::Rename { .. }))),
            "{text:?} {end:?}"
        );
        assert_eq!(
            run.state().overlays.rename.is_some(),
            still,
            "{text:?} {end:?}"
        );
    }
    let other =
        window()
            .send(renaming("a", RenamePlace::Tab))
            .send(typed("b", RenamePlace::Tab, "new"));
    assert_eq!(
        other
            .state()
            .overlays
            .rename
            .as_ref()
            .map(|r| r.text.as_str()),
        Some("a-name")
    );
}

#[test]
fn a_session_waiting_on_an_answer_is_answered_before_it_is_renamed() {
    let run = Scenario::new()
        .event(listing([session("a").waiting(Wait::Input)]))
        .send(renaming("a", RenamePlace::Sidebar));
    assert_eq!(run.state().overlays.rename, None);
    assert_eq!(
        run.state().chrome.status.line.as_deref(),
        Some("Answer a-name first, then rename it")
    );
}

#[test]
fn a_name_being_typed_goes_with_the_row_or_the_tab_it_is_typed_in() {
    let tab = window().send(renaming("a", RenamePlace::Tab));
    assert!(tab.state().overlays.rename.is_some());
    let gone = tab.send(Msg::Window(Action::TabClose));
    assert_eq!(gone.state().overlays.rename, None);

    let row = window().send(renaming("a", RenamePlace::Sidebar));
    assert!(row.state().overlays.rename.is_some());
    let hidden = row.send(Msg::PanelToggle(Side::Sidebar));
    assert_eq!(hidden.state().overlays.rename, None);
}

#[test]
fn a_menu_runs_its_entry_on_what_it_was_opened_on() {
    // (target, the area the keys go to)
    let cases = [
        (Target::Row(Item::Session("b".into())), Area::Sidebar),
        (Target::Tab(session_tab("a")), Area::Console),
        (Target::Link { at: 0, file: false }, Area::Details),
    ];
    for (target, area) in cases {
        let run = window().send(menu_on(target.clone())).send(Msg::MenuRun {
            target: target.clone(),
            action: Action::SessionStop,
        });
        assert_eq!(run.state().overlays.menu, None, "{target:?}");
        assert_eq!(run.state().content.focus, area, "{target:?}");
        assert_eq!(
            run.last_effects().last(),
            Some(&Effect::Perform(Action::SessionStop)),
            "{target:?}"
        );
    }
    let row = window().send(Msg::MenuRun {
        target: Target::Row(Item::Session("b".into())),
        action: Action::SessionNew,
    });
    assert_eq!(row.state().tree.selected, Some(Item::Session("b".into())));
}

#[test]
fn the_banner_shows_what_the_daemon_says_until_put_away_and_again_after_a_restart() {
    let run = Scenario::new().send(Msg::DaemonStanding(Some("old".into())));
    assert_eq!(run.state().overlays.banner.shown(), Some("old"));
    let run = run.send(Msg::BannerClose);
    assert_eq!(run.state().overlays.banner.shown(), None);
    let run = run.send(Msg::DaemonStanding(Some("old".into())));
    assert_eq!(run.state().overlays.banner.shown(), None);
    let run = run.send(Msg::DaemonRestarted(Some("still".into())));
    assert_eq!(run.state().overlays.banner.shown(), Some("still"));
    let run = run.send(Msg::DaemonRestarted(None));
    assert_eq!(run.state().overlays.banner.shown(), None);
}

#[test]
fn the_keys_help_takes_its_keys_by_the_bindings_the_chrome_holds() {
    let text = "[global]\n\"Ctrl+h\" = \"keys.help\"\n";
    let user = crate::keymap::Keymap::with_user(text).unwrap();
    let help = || {
        window()
            .tell(adroit_protocol::ConfigChange::Keys {
                file: crate::config::testing::keys_file(text),
            })
            .send(Msg::KeysHelpOpen(vec![Context::Console, Context::Global]))
    };
    assert_eq!(help().state().chrome.keymap, user);
    // (key, what it asks done, whether the help is still shown)
    let cases = [
        (Key::new("j"), vec![Effect::ScrollKeysHelp(1)], true),
        (
            Key::new("u").ctrl(),
            vec![Effect::ScrollKeysHelp(-10)],
            true,
        ),
        (Key::new("x"), vec![], true),
        (Key::new("Escape"), vec![], false),
        (Key::new("h").ctrl(), vec![], false),
    ];
    for (key, effects, shown) in cases {
        let run = help().send(Msg::HeldKey(key.clone()));
        assert_eq!(run.last_effects(), effects, "{key:?}");
        assert_eq!(run.state().chrome.keys_help.is_some(), shown, "{key:?}");
    }
}
