use adroit_model::EnvironmentId;
use adroit_protocol::{Event, Request, Version};

use super::Link;
use crate::effect::Effect;
use crate::msg::Msg;
use crate::scenario::Scenario;
use crate::testing::{self, here, listing, session};

// Events rather than builders, so one case can mix patches and snapshots.
fn patch(seq: u64) -> Event {
    testing::patch(seq).into()
}

fn snapshot(seq: u64) -> Event {
    testing::snapshot().seq(seq).into()
}

/// An environment other than the window's own.
fn there() -> EnvironmentId {
    EnvironmentId::new("there")
}

fn asks_snapshot(environment: EnvironmentId) -> Effect {
    Effect::Host {
        environment,
        request: Request::Snapshot,
    }
}

fn seq_of(run: &Scenario, environment: &EnvironmentId) -> Option<u64> {
    run.state().hosts.work(environment).map(|w| w.seq.0)
}

#[test]
fn opening_the_window_says_hello_in_every_version_it_speaks_to_its_own_environments_host() {
    let run = Scenario::new().send(Msg::Opened {
        environment: there(),
        home: Default::default(),
    });
    assert!(run.last_effects().contains(&Effect::Host {
        environment: there(),
        request: Request::Hello {
            versions: adroit_protocol::SPOKEN.to_vec(),
        },
    }));
    assert_eq!(run.state().hosts.environment, there());
    assert_eq!(run.state().hosts.own().link, Link::Opening);
}

#[test]
fn opening_another_environment_greets_its_host_once_and_names_it() {
    let run = Scenario::new().send(Msg::EnvironmentOpened {
        environment: there(),
        name: "Work machine".into(),
    });
    assert_eq!(
        run.last_effects(),
        [Effect::Host {
            environment: there(),
            request: Request::Hello {
                versions: adroit_protocol::SPOKEN.to_vec(),
            },
        }]
    );
    assert_eq!(
        run.state().hosts.get(&there()).map(|h| &h.link),
        Some(&Link::Opening)
    );
    assert_eq!(run.state().hosts.environment, here());
    let renamed = run.send(Msg::EnvironmentOpened {
        environment: there(),
        name: "Renamed".into(),
    });
    assert!(renamed.last_effects().is_empty());
    assert_eq!(renamed.state().hosts.name(&there()), "Renamed");
}

#[test]
fn naming_the_window_s_own_environment_does_not_greet_its_host_again() {
    let run = Scenario::new().send(Msg::EnvironmentOpened {
        environment: here(),
        name: "This computer".into(),
    });
    assert!(run.last_effects().is_empty());
    assert_eq!(run.state().hosts.name(&here()), "This computer");
}

#[test]
fn the_answer_to_hello_decides_the_link_of_the_host_that_answered() {
    let cases = [
        (
            Event::Welcome {
                version: adroit_protocol::SPOKEN[0],
            },
            Link::Ready {
                version: adroit_protocol::SPOKEN[0],
            },
        ),
        (
            Event::Welcome {
                version: Version(99),
            },
            Link::Refused {
                versions: vec![Version(99)],
            },
        ),
        (
            Event::Refused {
                versions: vec![Version(7)],
            },
            Link::Refused {
                versions: vec![Version(7)],
            },
        ),
    ];
    for (event, link) in cases {
        let run = Scenario::new().event_from(there(), event.clone());
        let link_of = |environment| run.state().hosts.get(&environment).map(|h| &h.link);
        assert_eq!(link_of(there()), Some(&link), "{event:?}");
        assert_eq!(link_of(here()), Some(&Link::Opening), "{event:?}");
    }
}

#[test]
fn patches_apply_in_order_and_a_gap_asks_for_the_work_again_once() {
    // (events after a snapshot at 3, seq held, snapshots asked for)
    let cases: Vec<(Vec<Event>, Option<u64>, usize)> = vec![
        (vec![patch(4), patch(5)], Some(5), 0),
        (vec![patch(3)], Some(3), 0),
        (vec![patch(2), patch(4)], Some(4), 0),
        (vec![patch(5)], Some(3), 1),
        (vec![patch(5), patch(6), patch(7)], Some(3), 1),
        (vec![patch(5), snapshot(6), patch(8)], Some(6), 2),
        (vec![patch(5), snapshot(6), patch(7)], Some(7), 1),
    ];
    for environment in [here(), there()] {
        for (events, seq, asked) in cases.clone() {
            let run = Scenario::new()
                .event_from(environment.clone(), snapshot(3))
                .events_from(environment.clone(), events.clone());
            assert_eq!(seq_of(&run, &environment), seq, "{environment} {events:?}");
            let asks = run
                .effects()
                .iter()
                .filter(|e| **e == asks_snapshot(environment.clone()))
                .count();
            assert_eq!(asks, asked, "{environment} {events:?}");
        }
    }
}

#[test]
fn patches_before_the_first_snapshot_wait_for_it() {
    for environment in [here(), there()] {
        let run = Scenario::new().events_from(environment.clone(), [patch(1), patch(5)]);
        assert_eq!(run.state().hosts.work(&environment), None, "{environment}");
        assert!(run.effects().is_empty(), "{environment}");
    }
}

#[test]
fn one_environments_snapshots_and_patches_leave_anothers_work_alone() {
    let run = Scenario::new()
        .event_from(here(), snapshot(3))
        .event_from(there(), snapshot(10))
        .event_from(there(), patch(11))
        .event_from(here(), patch(4));
    assert_eq!(seq_of(&run, &here()), Some(4));
    assert_eq!(seq_of(&run, &there()), Some(11));
}

#[test]
fn a_gap_in_one_environment_asks_only_its_host_for_the_work_again() {
    let run = Scenario::new()
        .event_from(here(), snapshot(3))
        .event_from(there(), snapshot(3))
        .event_from(there(), patch(5))
        .event_from(here(), patch(4));
    let asked: Vec<_> = run
        .effects()
        .iter()
        .filter(|e| {
            matches!(
                e,
                Effect::Host {
                    request: Request::Snapshot,
                    ..
                }
            )
        })
        .cloned()
        .collect();
    assert_eq!(asked, vec![asks_snapshot(there())]);
    assert_eq!(seq_of(&run, &here()), Some(4));
    // One host waiting for its snapshot does not keep another from asking.
    let run = run.event_from(here(), patch(6));
    assert_eq!(run.last_effects(), [asks_snapshot(here())]);
}

mod shown {
    use adroit_model::SessionId;
    use adroit_model::testing::SessionBuilder;

    use super::*;
    use crate::content::{Area, Tab};

    fn opener(id: SessionId) -> SessionBuilder {
        session("opener").delegation(id)
    }

    /// The window looking at `looking`, listing `listed`, then the host
    /// listing `appearing` besides.
    fn appears(looking: &str, listed: Vec<SessionBuilder>, appearing: SessionBuilder) -> Scenario {
        let mut all = listed.clone();
        all.push(appearing);
        Scenario::new()
            .event_from(here(), listing(listed).seq(1))
            .send(testing::show(Tab::Session(here(), looking.into())))
            .event_from(here(), testing::patch(2).listed(listed_of(all)))
    }

    fn listed_of(sessions: Vec<SessionBuilder>) -> adroit_protocol::Listed {
        adroit_protocol::testing::listed()
            .sessions(sessions)
            .build()
    }

    fn focused(run: &Scenario) -> Option<Tab> {
        run.state().content.windows.focused().cloned()
    }

    fn opened(run: &Scenario, tab: &Tab) -> bool {
        run.state().content.windows.shows_anywhere(tab)
    }

    #[test]
    fn a_session_asked_to_be_shown_opens_with_the_keys_while_its_opener_is_looked_at() {
        let id = SessionId::new();
        let run = appears("opener", vec![opener(id)], session("new").parent(id).show());
        assert_eq!(focused(&run), Some(Tab::Session(here(), "new".into())));
        assert_eq!(run.state().content.focus, Area::Console);
    }

    /// The window looking at the opener of delegation `id` while `typing`,
    /// then the host listing a session it opened, asking that it be shown.
    fn appears_while(id: SessionId, typing: Vec<Msg>) -> Scenario {
        Scenario::new()
            .event_from(here(), listing(vec![opener(id)]).seq(1))
            .send(testing::show(Tab::Session(here(), "opener".into())))
            .send_all(typing)
            .event_from(
                here(),
                testing::patch(2).listed(listed_of(vec![
                    opener(id),
                    session("new").parent(id).show(),
                ])),
            )
    }

    #[test]
    fn a_session_its_opener_opens_while_its_prompt_is_written_opens_behind_the_shown_tab() {
        let id = SessionId::new();
        for mode in [("insert", "INSERT"), ("cmd", "COMMAND")] {
            let run = appears_while(
                id,
                vec![Msg::Focus(Area::Prompt), Msg::PromptMode(mode.0, mode.1)],
            );
            assert_eq!(focused(&run), Some(Tab::Session(here(), "opener".into())));
            assert_eq!(run.state().content.focus, Area::Prompt);
            assert!(opened(&run, &Tab::Session(here(), "new".into())));
        }
    }

    #[test]
    fn a_session_its_opener_opens_while_the_console_passes_every_key_opens_behind() {
        let id = SessionId::new();
        let run = appears_while(
            id,
            vec![Msg::Focus(Area::Console), Msg::ConsolePassEveryKey],
        );
        assert_eq!(focused(&run), Some(Tab::Session(here(), "opener".into())));
        assert!(run.state().console.direct);
        assert!(opened(&run, &Tab::Session(here(), "new".into())));
    }

    #[test]
    fn a_session_its_opener_opens_while_its_prompt_is_in_normal_mode_takes_the_keys() {
        let id = SessionId::new();
        let run = appears_while(
            id,
            vec![
                Msg::Focus(Area::Prompt),
                Msg::PromptMode("normal", "NORMAL"),
            ],
        );
        assert_eq!(focused(&run), Some(Tab::Session(here(), "new".into())));
        assert_eq!(run.state().content.focus, Area::Console);
    }

    #[test]
    fn a_session_appearing_while_the_user_looks_elsewhere_opens_behind_the_shown_tab() {
        let id = SessionId::new();
        let run = appears(
            "other",
            vec![opener(id), session("other")],
            session("new").parent(id).show(),
        );
        assert_eq!(focused(&run), Some(Tab::Session(here(), "other".into())));
        assert!(opened(&run, &Tab::Session(here(), "new".into())));
    }

    #[test]
    fn a_session_started_in_the_background_is_only_listed() {
        let id = SessionId::new();
        let run = appears("opener", vec![opener(id)], session("new").parent(id));
        assert_eq!(focused(&run), Some(Tab::Session(here(), "opener".into())));
        assert!(!opened(&run, &Tab::Session(here(), "new".into())));
    }

    #[test]
    fn a_session_asked_to_be_shown_by_another_opener_opens_behind_the_shown_tab() {
        let run = appears(
            "opener",
            vec![opener(SessionId::new())],
            session("new").parent(SessionId::new()).show(),
        );
        assert_eq!(focused(&run), Some(Tab::Session(here(), "opener".into())));
        assert!(opened(&run, &Tab::Session(here(), "new".into())));
    }

    #[test]
    fn a_session_no_session_opened_opens_behind_the_shown_tab() {
        let run = appears("other", vec![session("other")], session("new").show());
        assert_eq!(focused(&run), Some(Tab::Session(here(), "other".into())));
        assert!(opened(&run, &Tab::Session(here(), "new".into())));
    }

    /// How many tabs show `tab`, after the host lists session `a` asked to be
    /// opened at each of `times` in turn, while the window looks at `other`.
    fn asked_to_open(times: &[u64]) -> Scenario {
        let mut run = Scenario::new()
            .event_from(here(), listing(vec![session("other"), session("a")]).seq(1))
            .send(testing::show(Tab::Session(here(), "other".into())));
        for (n, at) in times.iter().enumerate() {
            run = run.event_from(
                here(),
                testing::patch(n as u64 + 2).listed(listed_of(vec![
                    session("other"),
                    session("a").opened_ms(*at),
                ])),
            );
        }
        run
    }

    #[test]
    fn a_session_asked_from_outside_to_be_opened_opens_behind_the_shown_tab() {
        let run = asked_to_open(&[5]);
        assert!(opened(&run, &Tab::Session(here(), "a".into())));
        assert_eq!(focused(&run), Some(Tab::Session(here(), "other".into())));
    }

    #[test]
    fn a_session_is_opened_once_for_each_time_it_is_asked_to_be() {
        let a = Tab::Session(here(), "a".into());
        let run = asked_to_open(&[5]);
        let tabs = run.state().content.windows.tabs().len();
        let again = asked_to_open(&[5, 5, 6]);
        assert_eq!(again.state().content.windows.tabs().len(), tabs);
        assert!(opened(&again, &a));
    }

    #[test]
    fn a_session_asked_to_be_opened_before_the_window_found_it_is_left_as_it_is() {
        let run =
            Scenario::new().event_from(here(), listing(vec![session("a").opened_ms(5)]).seq(1));
        assert!(!opened(&run, &Tab::Session(here(), "a".into())));
    }

    /// A terminal of id `id` the session of delegation `opener` started,
    /// asking that it be shown.
    fn shown_terminal(id: u128, opener: SessionId) -> adroit_model::TerminalSession {
        adroit_model::TerminalSession {
            opener: Some(opener),
            show: true,
            ..adroit_model::testing::shell(id, "/r/opener")
        }
    }

    /// The window looking at `looking`, listing `listed`, then the host
    /// listing `appearing` besides them.
    fn terminal_appears(
        looking: &str,
        listed: Vec<SessionBuilder>,
        appearing: adroit_model::TerminalSession,
    ) -> Scenario {
        let all = adroit_protocol::testing::listed()
            .sessions(listed.clone())
            .terminal(appearing)
            .build();
        Scenario::new()
            .event_from(here(), listing(listed).seq(1))
            .send(testing::show(Tab::Session(here(), looking.into())))
            .event_from(here(), testing::patch(2).listed(all))
    }

    #[test]
    fn a_terminal_asked_to_be_shown_opens_with_the_keys_while_its_opener_is_looked_at() {
        let id = SessionId::new();
        let run = terminal_appears("opener", vec![opener(id)], shown_terminal(7, id));
        assert_eq!(focused(&run), Some(Tab::Terminal(here(), 7)));
        assert_eq!(run.state().content.focus, Area::Console);
    }

    #[test]
    fn a_terminal_appearing_while_the_user_looks_elsewhere_opens_behind_the_shown_tab() {
        let id = SessionId::new();
        let run = terminal_appears(
            "other",
            vec![opener(id), session("other")],
            shown_terminal(7, id),
        );
        assert_eq!(focused(&run), Some(Tab::Session(here(), "other".into())));
        assert!(opened(&run, &Tab::Terminal(here(), 7)));
    }

    #[test]
    fn a_terminal_started_in_the_background_is_only_listed() {
        let id = SessionId::new();
        let unasked = adroit_model::TerminalSession {
            show: false,
            ..shown_terminal(7, id)
        };
        let run = terminal_appears("opener", vec![opener(id)], unasked);
        assert_eq!(focused(&run), Some(Tab::Session(here(), "opener".into())));
        assert!(!opened(&run, &Tab::Terminal(here(), 7)));
    }

    #[test]
    fn a_terminal_started_by_another_or_by_no_session_opens_behind_the_shown_tab() {
        let id = SessionId::new();
        for opener_of_it in [Some(SessionId::new()), None] {
            let terminal = adroit_model::TerminalSession {
                opener: opener_of_it,
                ..shown_terminal(7, id)
            };
            let run = terminal_appears("opener", vec![opener(id)], terminal);
            assert_eq!(focused(&run), Some(Tab::Session(here(), "opener".into())));
            assert!(opened(&run, &Tab::Terminal(here(), 7)));
        }
    }

    #[test]
    fn of_two_sessions_appearing_one_takes_the_keys_and_the_other_opens_behind() {
        let id = SessionId::new();
        let all = vec![
            opener(id),
            session("first").parent(id).show(),
            session("second").parent(id).show(),
        ];
        let run = Scenario::new()
            .event_from(here(), listing(vec![opener(id)]).seq(1))
            .send(testing::show(Tab::Session(here(), "opener".into())))
            .event_from(here(), testing::patch(2).listed(listed_of(all)));
        assert_eq!(focused(&run), Some(Tab::Session(here(), "first".into())));
        assert!(opened(&run, &Tab::Session(here(), "second".into())));
    }

    #[test]
    fn a_terminal_listed_again_is_not_shown_again() {
        let id = SessionId::new();
        let both = || {
            adroit_protocol::testing::listed()
                .sessions([opener(id)])
                .terminal(shown_terminal(7, id))
                .build()
        };
        let run = Scenario::new()
            .event_from(here(), testing::snapshot().listed(both()).seq(1))
            .send(testing::show(Tab::Session(here(), "opener".into())))
            .event_from(here(), testing::patch(2).listed(both()));
        assert_eq!(focused(&run), Some(Tab::Session(here(), "opener".into())));
    }

    #[test]
    fn a_session_first_listed_by_a_host_that_took_over_is_not_shown_again() {
        let id = SessionId::new();
        let both = || vec![opener(id), session("new").parent(id).show()];
        let run = Scenario::new()
            .event_from(here(), listing(both()).seq(1))
            .send(testing::show(Tab::Session(here(), "opener".into())))
            .event_from(here(), testing::snapshot().seq(1))
            .event_from(here(), testing::patch(2).listed(listed_of(both())));
        assert_eq!(focused(&run), Some(Tab::Session(here(), "opener".into())));
        assert!(!opened(&run, &Tab::Session(here(), "new".into())));
    }

    #[test]
    fn a_session_listed_again_is_not_shown_again() {
        let id = SessionId::new();
        let both = || vec![opener(id), session("new").parent(id).show()];
        let run = Scenario::new()
            .event_from(here(), listing(both()).seq(1))
            .send(testing::show(Tab::Session(here(), "opener".into())))
            .event_from(here(), testing::patch(2).listed(listed_of(both())));
        assert_eq!(focused(&run), Some(Tab::Session(here(), "opener".into())));
    }
}

#[test]
fn a_session_is_found_only_in_the_environment_it_runs_in() {
    let run = Scenario::new()
        .event_from(here(), listing([session("a").name("here-a")]))
        .event_from(there(), listing([session("b").name("there-b")]));
    let name = |environment: EnvironmentId, id: &str| {
        run.state()
            .hosts
            .session(&environment, id)
            .map(|(_, s)| s.name.clone())
    };
    assert_eq!(name(here(), "a").as_deref(), Some("here-a"));
    assert_eq!(name(there(), "b").as_deref(), Some("there-b"));
    assert_eq!(name(there(), "a"), None);
    assert_eq!(name(here(), "b"), None);
    assert_eq!(name(EnvironmentId::new("elsewhere"), "a"), None);
}

#[test]
fn the_same_session_id_in_two_environments_is_two_sessions() {
    let run = Scenario::new()
        .event_from(here(), listing([session("a").name("here-a")]))
        .event_from(there(), listing([session("a").name("there-a")]));
    let name = |environment: EnvironmentId| {
        run.state()
            .hosts
            .session(&environment, "a")
            .map(|(_, s)| s.name.clone())
    };
    assert_eq!(name(here()).as_deref(), Some("here-a"));
    assert_eq!(name(there()).as_deref(), Some("there-a"));
}

#[test]
fn what_the_host_says_of_a_request_is_said_in_the_status_line_and_typing_goes_to_the_pane() {
    use adroit_protocol::{Told, Typing};

    use crate::content::Tab;

    let said = |told: Told| {
        Scenario::new()
            .event(Event::Told(told))
            .state()
            .chrome
            .status
            .line
            .clone()
    };
    let notice = said(Told::Notice {
        text: "ended fix".into(),
    });
    assert_eq!(notice.as_deref(), Some("ended fix"));
    let failure = said(Told::Failure {
        text: "cannot end fix".into(),
    });
    assert_eq!(failure.as_deref(), Some("cannot end fix"));
    let typing = vec![Typing::Paste {
        text: "/rename fixer".into(),
    }];
    for environment in [here(), there()] {
        let run = Scenario::new().event_from(
            environment.clone(),
            Event::Type {
                session: "0a".into(),
                typing: typing.clone(),
            },
        );
        assert!(
            run.last_effects().contains(&Effect::TypeInto {
                tab: Tab::Session(environment.clone(), "0a".into()),
                typing: typing.clone(),
            }),
            "{environment}"
        );
    }
}

#[test]
fn a_failure_a_host_repeats_is_said_once_until_its_work_comes_again() {
    use adroit_protocol::Told;

    let failure = || {
        Event::Told(Told::Failure {
            text: "cannot reach the work host".into(),
        })
    };
    let line = |run: &Scenario| run.state().chrome.status.line.clone();
    let repeated = Scenario::new()
        .event(failure())
        .send(Msg::Status(None))
        .event(failure());
    assert_eq!(line(&repeated), None);
    let again = repeated.event(listing([session("a")])).event(failure());
    assert_eq!(line(&again).as_deref(), Some("cannot reach the work host"));
}

#[test]
fn a_host_s_failure_is_named_after_its_environment_while_several_are_shown() {
    use adroit_protocol::Told;

    let run = Scenario::new()
        .send(Msg::EnvironmentOpened {
            environment: there(),
            name: "There".into(),
        })
        .event_from(
            there(),
            Event::Told(Told::Failure {
                text: "cannot reach the work host".into(),
            }),
        );
    assert_eq!(
        run.state().chrome.status.line.as_deref(),
        Some("There: cannot reach the work host")
    );
}
