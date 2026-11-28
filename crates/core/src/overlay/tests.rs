use crate::testing::here;
use adroit_model::{Key, Process, Wait};

use super::rename::Place as RenamePlace;
use super::*;
use crate::actions::Action;
use crate::chrome::panels::layout::Side;
use crate::content::Area;
use crate::effect::Effect;
use crate::keymap::Context;
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
        reach: Reach::Here,
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
        (vec![Msg::AddingOpen], (Some(Holder::Adding), true, true)),
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
fn a_key_the_menu_does_not_take_closes_it_and_does_nothing_else() {
    for key in ["y", "Escape", "q"] {
        let run = window()
            .send(menu_on(Target::Tab(session_tab("a"))))
            .send(held(key));
        assert_eq!(run.state().overlays.menu, None, "{key}");
        assert!(run.last_effects().is_empty(), "{key}");
    }
    let run = window()
        .send(menu_on(Target::Tab(session_tab("a"))))
        .send(Msg::HeldKey(Key::new("c").ctrl()));
    assert_eq!(run.state().overlays.menu, None);
}

fn menu_cursor(run: &Scenario) -> Option<usize> {
    run.state().overlays.menu.as_ref().and_then(|m| m.cursor)
}

#[test]
fn the_menu_is_moved_through_round_its_ends_by_the_keys_that_move_in_a_list() {
    let open = || window().send(menu_on(Target::Tab(session_tab("a"))));
    let len = {
        let run = open();
        let state = run.state();
        menu_entries(state.overlays.menu.as_ref().unwrap(), &state.hosts).len()
    };
    assert!(len > 1);
    // (keys pressed, the entry moved to)
    let cases: [(&[&str], Option<usize>); 5] = [
        (&[], None),
        (&["j"], Some(0)),
        (&["ArrowDown", "ArrowDown"], Some(1)),
        (&["k"], Some(len - 1)),
        (&["j", "k"], Some(len - 1)),
    ];
    for (keys, at) in cases {
        let run = keys.iter().fold(open(), |run, k| run.send(held(k)));
        assert_eq!(menu_cursor(&run), at, "{keys:?}");
    }
    let ctrl = open().send(Msg::HeldKey(Key::new("n").ctrl()));
    assert_eq!(menu_cursor(&ctrl), Some(0));
}

#[test]
fn enter_runs_the_menu_entry_moved_to_on_what_the_menu_was_opened_on() {
    let target = Target::Tab(session_tab("a"));
    let open = window().send(menu_on(target.clone()));
    let second = {
        let state = open.state();
        menu_entries(state.overlays.menu.as_ref().unwrap(), &state.hosts)[1]
            .action
            .clone()
    };
    let run = open.send(held("j")).send(held("j")).send(held("Enter"));
    assert_eq!(run.state().overlays.menu, None);
    assert_eq!(run.last_effects().last(), Some(&Effect::Perform(second)));
    let unmoved = window().send(menu_on(target)).send(held("Enter"));
    assert!(unmoved.last_effects().is_empty());
}

#[test]
fn a_key_bound_to_close_over_the_window_closes_every_overlay() {
    let text = "[overlay]\n\"Ctrl+g\" = \"overlay.close\"\n";
    let opens = [
        menu_on(Target::Tab(session_tab("a"))),
        Msg::KeysHelpOpen(vec![Context::Global]),
        Msg::PickerOpen,
        Msg::ResumeOpen,
        Msg::AddingOpen,
        Msg::Confirm(Box::new(Ask::RestartDaemon(here()))),
    ];
    for open in opens {
        let run = window()
            .send(Msg::KeysRead(crate::config::testing::keys_file(text)))
            .send(open.clone())
            .send(Msg::HeldKey(Key::new("g").ctrl()));
        assert_eq!(held_by(&run).0, None, "{open:?}");
    }
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
            Ask::UpdateMachine(here(), "Here".into()),
            "y",
            Some(Effect::UpdateMachine(here())),
            false,
        ),
        (
            Ask::RestartDaemon(here()),
            "y",
            Some(Effect::RestartDaemon(here())),
            false,
        ),
        (
            Ask::RestartGateway,
            "y",
            Some(Effect::RestartGateway),
            false,
        ),
        (stop.clone(), "n", None, false),
        (stop.clone(), "Escape", None, false),
        (stop.clone(), "q", None, false),
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
fn the_sheet_adding_an_environment_closes_by_the_keys_that_close_over_the_window() {
    for key in ["y", "j", "Enter"] {
        let kept = window().send(Msg::AddingOpen).send(held(key));
        assert!(kept.state().overlays.adding, "{key}");
        assert!(kept.last_effects().is_empty(), "{key}");
    }
    let closed = window().send(Msg::AddingOpen).send(held("Escape"));
    assert!(!closed.state().overlays.adding);
    let ctrl = window()
        .send(Msg::AddingOpen)
        .send(Msg::HeldKey(Key::new("c").ctrl()));
    assert!(!ctrl.state().overlays.adding);
    let cancelled = window().send(Msg::AddingOpen).send(Msg::AddingClose);
    assert!(!cancelled.state().overlays.adding);
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
fn a_name_emptied_and_kept_hands_the_session_s_naming_back_to_its_agent() {
    let run = window()
        .send(renaming("a", RenamePlace::Sidebar))
        .send(typed("a", RenamePlace::Sidebar, "  "))
        .send(ended("a", RenamePlace::Sidebar, true));
    assert_eq!(
        run.last_effects(),
        [Effect::Host {
            environment: here(),
            request: Request::HandBackName { id: "a".into() },
        }]
    );

    let cancelled = window()
        .send(renaming("a", RenamePlace::Tab))
        .send(typed("a", RenamePlace::Tab, ""))
        .send(ended("a", RenamePlace::Tab, false));
    assert!(cancelled.last_effects().is_empty());
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
        (Target::Start(Item::Environment(here())), Area::Sidebar),
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
            .send(Msg::KeysRead(crate::config::testing::keys_file(text)))
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

fn taken(id: &str) -> adroit_model::Interrupted {
    adroit_model::Interrupted {
        id: id.into(),
        name: id.into(),
        cwd: "/r".into(),
        started_ms: 1,
        what: adroit_model::InterruptedKind::Terminal {
            program: adroit_model::TerminalProgram::Shell,
            file: None,
        },
    }
}

fn taking(ids: &[&str], seq: u64) -> adroit_protocol::testing::PatchBuilder {
    let mut listed = adroit_protocol::testing::listed();
    for id in ids {
        listed = listed.interrupted(taken(id));
    }
    crate::testing::patch(seq).listed(listed)
}

fn offered(run: &Scenario) -> Option<Vec<String>> {
    let state = run.state();
    interrupted_banner(&state.config.settings, &state.hosts, &state.overlays.banner)
        .map(|(_, waiting)| waiting.into_iter().map(|i| i.id.clone()).collect())
}

#[test]
fn the_banner_offers_what_the_terminal_host_took_only_while_the_settings_ask() {
    let asked = window().event(taking(&["a", "7"], 2));
    assert_eq!(offered(&asked), Some(vec!["a".into(), "7".into()]));
    for setting in ["always", "never"] {
        let run = window()
            .event(crate::config::testing::configured(&format!(
                "restore_sessions = \"{setting}\"\n"
            )))
            .event(taking(&["a"], 2));
        assert_eq!(offered(&run), None, "{setting}");
    }
}

#[test]
fn the_banner_brings_back_or_lets_go_of_everything_it_offers() {
    let run = || window().event(taking(&["a", "7"], 2));
    let ids = vec!["a".to_string(), "7".to_string()];
    assert_eq!(
        run().send(Msg::BringBackAll).last_effects(),
        [Effect::BringBack {
            environment: here(),
            ids: ids.clone()
        }]
    );
    assert_eq!(
        run().send(Msg::LetGoAll).last_effects(),
        [Effect::LetGo {
            environment: here(),
            ids
        }]
    );
}

#[test]
fn the_banner_put_away_shows_again_only_once_more_is_taken() {
    let run = window()
        .event(taking(&["a"], 2))
        .send(Msg::InterruptedLater);
    assert_eq!(offered(&run), None);
    let run = run.event(taking(&["a"], 3));
    assert_eq!(offered(&run), None);
    let run = run.event(taking(&["a", "b"], 4));
    assert_eq!(offered(&run), Some(vec!["a".into(), "b".into()]));
}

#[test]
fn what_did_not_come_back_is_said_with_why() {
    let outcomes = vec![
        adroit_model::BroughtBack {
            id: "a".into(),
            name: "fix".into(),
            by: Some("b".into()),
            refused: None,
        },
        adroit_model::BroughtBack {
            id: "c".into(),
            name: "probe".into(),
            by: None,
            refused: Some("/gone is not there any more".into()),
        },
    ];
    let run = window().send(Msg::BroughtBack {
        environment: here(),
        outcomes,
    });
    assert_eq!(
        run.state().chrome.status.line.as_deref(),
        Some("probe did not come back: /gone is not there any more")
    );
}

fn outdated_banner(run: &Scenario) -> Option<(EnvironmentId, String)> {
    run.state()
        .overlays
        .banner
        .outdated()
        .map(|(environment, text)| (environment.clone(), text.to_string()))
}

/// A window that shows the environments `here` and `there`.
fn shown_here_and_there() -> Scenario {
    Scenario::new()
        .event_from(here(), listing([session("a")]))
        .event_from(there(), listing([session("b")]))
}

fn outdated(environment: EnvironmentId, text: &str) -> Msg {
    Msg::WorkHostOutdated {
        environment,
        text: text.into(),
    }
}

#[test]
fn a_work_host_older_than_its_state_gets_a_banner_that_restarts_it() {
    let run = shown_here_and_there().send(outdated(there(), "a newer adroit wrote it"));
    assert_eq!(
        outdated_banner(&run),
        Some((there(), "a newer adroit wrote it".into()))
    );
    let run = run.send(Msg::OutdatedRestart);
    assert_eq!(run.last_effects(), [Effect::RestartWorkHost(there())]);
    assert_eq!(outdated_banner(&run), None);
}

#[test]
fn the_banner_of_a_work_host_older_than_its_state_goes_until_it_fails_so_again() {
    let run = shown_here_and_there()
        .send(outdated(there(), "first"))
        .send(Msg::OutdatedLater);
    assert_eq!(outdated_banner(&run), None);
    assert_eq!(run.last_effects(), []);
    let run = run.send(outdated(there(), "again"));
    assert_eq!(outdated_banner(&run), Some((there(), "again".into())));
}

#[test]
fn putting_one_work_host_s_banner_away_leaves_another_s() {
    let run = shown_here_and_there()
        .send(outdated(there(), "there"))
        .send(outdated(here(), "here"));
    let first = outdated_banner(&run).map(|(environment, _)| environment);
    let run = run.send(Msg::OutdatedLater);
    let second = outdated_banner(&run).map(|(environment, _)| environment);
    assert!(second.is_some());
    assert_ne!(first, second);
}

#[test]
fn a_work_host_older_than_its_state_raises_no_banner_once_the_window_stops_showing_it() {
    let run = Scenario::new().send(outdated(there(), "late"));
    assert_eq!(outdated_banner(&run), None);
}

#[test]
fn a_work_host_that_greets_the_window_again_takes_its_banner_away() {
    let run = shown_here_and_there()
        .send(outdated(there(), "old"))
        .event_from(
            there(),
            adroit_protocol::Event::Welcome {
                version: adroit_protocol::SPOKEN[0],
            },
        );
    assert_eq!(outdated_banner(&run), None);
}

#[test]
fn a_work_host_that_tells_of_failing_for_newer_state_raises_the_banner_too() {
    let text = adroit_protocol::newer_state(11);
    let run = shown_here_and_there().event_from(
        there(),
        adroit_protocol::Event::Told(adroit_protocol::Told::Failure { text: text.clone() }),
    );
    assert_eq!(outdated_banner(&run), Some((there(), text)));
}
