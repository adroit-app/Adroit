use crate::testing::here;
use adroit_model::{Key, Process, Wait};

use super::*;
use crate::actions::Action;
use crate::content::Area;
use crate::effect::Effect;
use crate::keymap::Context;
use crate::layout::Side;
use crate::msg::Msg;
use crate::scenario::Scenario;
use crate::testing::{listing, session, session_tab, there};
use crate::tree::nav::Item;

/// A window listing the sessions `a` and `b`, showing `a`.
fn window() -> Scenario {
    Scenario::new()
        .event(listing([session("a"), session("b")]))
        .send(Msg::ShowSession(here(), "a".into()))
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
                environment: here(),
                id: "a".into(),
                place: RenamePlace::Tab,
            }],
            (Some(Holder::Rename), true, true),
        ),
        (
            vec![Msg::Confirm(Box::new(Ask::RestartDaemon(here())))],
            (Some(Holder::Confirm), true, true),
        ),
        (
            vec![
                Msg::Confirm(Box::new(Ask::RestartDaemon(here()))),
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
    Effect::Host {
        environment: here(),
        request: Request::Stop {
            session: Box::new(session("a").build()),
        },
    }
}

#[test]
fn a_question_is_answered_by_its_keys_and_holds_every_other() {
    let stop = Ask::Stop(here(), session("a").build());
    let process = Process {
        pid: 7,
        name: "node".into(),
        ..Default::default()
    };
    let kill = Ask::KillProcess(here(), session("a").build(), process.clone());
    // (asked, key, what answering asks done, whether it is still asked)
    let cases = [
        (stop.clone(), "y", Some(stop_a()), false),
        (stop.clone(), "Enter", Some(stop_a()), false),
        (
            kill,
            "y",
            Some(Effect::Host {
                environment: here(),
                request: Request::Kill {
                    session: Box::new(session("a").build()),
                    process,
                },
            }),
            false,
        ),
        (
            Ask::RestartDaemon(here()),
            "y",
            Some(Effect::RestartDaemon(here())),
            false,
        ),
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
        .send(Msg::Confirm(Box::new(Ask::RestartDaemon(here()))))
        .send(Msg::ConfirmAnswer(true));
    assert_eq!(run.last_effects(), [Effect::RestartDaemon(here())]);
    let run = window()
        .send(Msg::Confirm(Box::new(Ask::RestartDaemon(here()))))
        .send(Msg::ConfirmAnswer(false));
    assert!(run.last_effects().is_empty());
    assert_eq!(run.state().overlays.confirm, None);
}

#[test]
fn ending_a_session_asks_of_the_selected_row_s_or_else_the_shown_one() {
    // (the row selected, from the tree, the session asked of)
    let cases = [
        (Some(Item::Session(here(), "b".into())), true, Some("b")),
        (Some(Item::Waiting(here(), "b".into())), true, Some("b")),
        (Some(Item::Session(here(), "b".into())), false, Some("a")),
        (None, true, Some("a")),
    ];
    for (row, selection, want) in cases {
        let run = window()
            .send(Msg::TreeSelect(row.clone()))
            .send(Msg::ConfirmStop { selection });
        let asked = match &run.state().overlays.confirm {
            Some(Ask::Stop(_, s)) => Some(s.id.as_str()),
            _ => None,
        };
        assert_eq!(asked, want, "{row:?} {selection}");
    }
    let nothing = Scenario::new().send(Msg::ConfirmStop { selection: false });
    assert_eq!(nothing.state().overlays.confirm, None);
}

#[test]
fn ending_a_session_shown_from_another_environment_asks_that_environments_host() {
    let there = adroit_model::EnvironmentId::new("there");
    let run = Scenario::new()
        .event(listing([session("a").name("here-a")]))
        .event_from(there.clone(), listing([session("a").name("there-a")]))
        .send(crate::testing::show(crate::content::Tab::Session(
            there.clone(),
            "a".into(),
        )))
        .send(Msg::ConfirmStop { selection: false })
        .send(Msg::ConfirmAnswer(true));
    assert_eq!(
        run.last_effects(),
        [Effect::Host {
            environment: there,
            request: Request::Stop {
                session: Box::new(session("a").name("there-a").build()),
            },
        }]
    );
}

fn renaming(id: &str, place: RenamePlace) -> Msg {
    Msg::RenameStart {
        environment: here(),
        id: id.into(),
        place,
    }
}

fn typed(id: &str, place: RenamePlace, text: &str) -> Msg {
    Msg::RenameText {
        environment: here(),
        id: id.into(),
        place,
        text: text.into(),
    }
}

fn ended(id: &str, place: RenamePlace, commit: bool) -> Msg {
    Msg::RenameEnd {
        environment: here(),
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
    let Some(Effect::Host {
        request: Request::Rename { id, name, was, .. },
        ..
    }) = run.last_effects().first()
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
            !run.last_effects().iter().any(|e| matches!(
                e,
                Effect::Host {
                    request: Request::Rename { .. },
                    ..
                }
            )),
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
        (
            Target::Row(Item::Session(here(), "b".into())),
            Area::Sidebar,
        ),
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
        target: Target::Row(Item::Session(here(), "b".into())),
        action: Action::SessionNew,
    });
    assert_eq!(
        row.state().tree.selected,
        Some(Item::Session(here(), "b".into()))
    );
}

fn standing(environment: EnvironmentId, note: Option<&str>) -> Msg {
    Msg::DaemonStanding {
        environment,
        note: note.map(str::to_string),
    }
}

fn restarted(environment: EnvironmentId, note: Option<&str>) -> Msg {
    Msg::DaemonRestarted {
        environment,
        note: note.map(str::to_string),
    }
}

fn banner(run: &Scenario) -> Option<(EnvironmentId, String)> {
    run.state()
        .overlays
        .banner
        .shown()
        .map(|(environment, note)| (environment.clone(), note.to_string()))
}

#[test]
fn the_banner_shows_what_the_daemon_says_until_put_away_and_again_after_a_restart() {
    let says = |note: &str| Some((here(), note.to_string()));
    let run = Scenario::new().send(standing(here(), Some("old")));
    assert_eq!(banner(&run), says("old"));
    let run = run.send(Msg::BannerClose);
    assert_eq!(banner(&run), None);
    let run = run.send(standing(here(), Some("old")));
    assert_eq!(banner(&run), None);
    let run = run.send(restarted(here(), Some("still")));
    assert_eq!(banner(&run), says("still"));
    let run = run.send(restarted(here(), None));
    assert_eq!(banner(&run), None);
}

#[test]
fn each_environment_s_daemon_has_a_banner_of_its_own() {
    let run = Scenario::new()
        .send(standing(there(), Some("old there")))
        .send(standing(here(), None));
    assert_eq!(banner(&run), Some((there(), "old there".into())));
    let run = run.send(standing(here(), Some("old here")));
    let first = banner(&run).map(|(environment, _)| environment);
    let run = run.send(Msg::BannerClose);
    let second = banner(&run).map(|(environment, _)| environment);
    assert!(second.is_some(), "putting one away leaves the other");
    assert_ne!(first, second);
    let run = run.send(restarted(second.unwrap(), None));
    assert_eq!(banner(&run), None);
}

#[test]
fn the_banner_restarts_the_daemon_of_the_environment_it_speaks_of() {
    let run = Scenario::new().send(standing(there(), Some("old")));
    let Some((environment, _)) = banner(&run) else {
        panic!("no banner");
    };
    let run = run
        .send(Msg::Confirm(Box::new(Ask::RestartDaemon(environment))))
        .send(Msg::ConfirmAnswer(true));
    assert_eq!(run.last_effects(), [Effect::RestartDaemon(there())]);
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

#[test]
fn renaming_another_environment_s_session_asks_that_environment_s_host() {
    let run = crate::testing::shows_there(window(), [session("a").name("there-a")])
        .send(Msg::RenameStart {
            environment: there(),
            id: "a".into(),
            place: RenamePlace::Sidebar,
        })
        .send(Msg::RenameText {
            environment: there(),
            id: "a".into(),
            place: RenamePlace::Sidebar,
            text: "new".into(),
        })
        .send(Msg::RenameEnd {
            environment: there(),
            id: "a".into(),
            place: RenamePlace::Sidebar,
            commit: true,
        });
    let Some(Effect::Host {
        environment,
        request: Request::Rename { name, was, .. },
    }) = run.last_effects().first()
    else {
        panic!("{:?}", run.last_effects());
    };
    assert_eq!(
        (environment, name.as_str(), was.as_str()),
        (&there(), "new", "there-a")
    );
}

#[test]
fn killing_a_process_of_another_environment_s_session_asks_that_environment_s_host() {
    let process = Process {
        pid: 7,
        name: "node".into(),
        ..Default::default()
    };
    let run = window()
        .send(Msg::Confirm(Box::new(Ask::KillProcess(
            there(),
            session("a").build(),
            process.clone(),
        ))))
        .send(Msg::ConfirmAnswer(true));
    assert_eq!(
        run.last_effects(),
        [Effect::Host {
            environment: there(),
            request: Request::Kill {
                session: Box::new(session("a").build()),
                process,
            },
        }]
    );
}
