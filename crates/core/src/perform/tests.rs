use adroit_model::testing::worktree as wt;
use adroit_model::{Agent, Key, Session, TerminalSession, build};
use adroit_protocol::{Event, Listed, Request, Seq, TreeWork, WorkSnapshot};

use super::*;
use crate::keymap::Chord;
use crate::scenario::Scenario;

fn session(id: &str) -> Session {
    Session {
        id: id.into(),
        name: format!("{id}-name"),
        cwd: format!("/r/{id}").into(),
        ..Default::default()
    }
}

fn listing(sessions: Vec<Session>) -> Event {
    let terminals = vec![TerminalSession {
        id: 7,
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
    Event::Snapshot(Box::new(WorkSnapshot {
        seq: Seq(1),
        tree: TreeWork {
            listed: Some(Box::new(Listed {
                sessions,
                terminals,
                tree,
            })),
            ..Default::default()
        },
        ..Default::default()
    }))
}

/// A window showing the session `a`, the keys in `area`.
fn showing(area: Area) -> Scenario {
    Scenario::new()
        .event(listing(vec![session("a")]))
        .send(Msg::ShowSession("a".into()))
        .send(Msg::Focus(area))
}

fn perform(action: Action, key: Option<&str>, probe: Probe) -> Msg {
    Msg::Perform {
        action,
        key: key.map(Key::new),
        probe: Box::new(probe),
    }
}

fn with_pane() -> Probe {
    Probe {
        pane: true,
        rows: 30,
        ..Default::default()
    }
}

#[test]
fn the_scroll_actions_scroll_the_console_s_program_or_else_the_details() {
    // (area, a pane shown, action, effect)
    let cases = [
        (
            Area::Console,
            true,
            Action::ScrollDown,
            Effect::PaneWheel(-1),
        ),
        (Area::Console, true, Action::ScrollUp, Effect::PaneWheel(1)),
        (
            Area::Console,
            true,
            Action::ScrollHalfDown,
            Effect::PaneWheel(-15),
        ),
        (
            Area::Console,
            true,
            Action::ScrollHalfUp,
            Effect::PaneWheel(15),
        ),
        (
            Area::Console,
            true,
            Action::ScrollTop,
            Effect::PaneWheel(i32::MAX),
        ),
        (
            Area::Console,
            true,
            Action::ScrollBottom,
            Effect::PaneWheel(i32::MIN),
        ),
        (
            Area::Console,
            false,
            Action::ScrollDown,
            Effect::ScrollDetails(Scroll::Down),
        ),
        (
            Area::Details,
            true,
            Action::ScrollHalfDown,
            Effect::ScrollDetails(Scroll::HalfDown),
        ),
        (
            Area::Sidebar,
            true,
            Action::ScrollHalfUp,
            Effect::ScrollDetails(Scroll::HalfUp),
        ),
        (
            Area::Details,
            true,
            Action::ScrollTop,
            Effect::ScrollDetails(Scroll::Top),
        ),
    ];
    for (area, pane, action, effect) in cases {
        let probe = Probe {
            pane,
            ..with_pane()
        };
        let run = showing(area).send(perform(action.clone(), None, probe));
        assert_eq!(run.last_effects(), [effect], "{area:?} {pane} {action:?}");
    }
}

#[test]
fn a_key_sent_from_the_prompt_is_neovim_s_while_it_owns_the_keys_or_no_session_takes_it() {
    let send = Action::ConsoleSend(None);
    // (Neovim owns the keys, a pane shown, the effects)
    let cases = [
        (true, true, vec![Effect::PromptKey(Key::new("Down"))]),
        (false, false, vec![Effect::PromptKey(Key::new("Down"))]),
        (false, true, vec![Effect::PaneKey(Key::new("Down"))]),
    ];
    for (owns, pane, effects) in cases {
        let probe = Probe {
            pane,
            prompt_owns: owns,
            ..with_pane()
        };
        let run = showing(Area::Prompt)
            .send(Msg::PromptRuns(true))
            .send(perform(send.clone(), Some("Down"), probe));
        let got: Vec<_> = run
            .last_effects()
            .iter()
            .filter(|e| matches!(e, Effect::PromptKey(_) | Effect::PaneKey(_)))
            .cloned()
            .collect();
        assert_eq!(got, effects, "owns={owns} pane={pane}");
    }
}

#[test]
fn a_chord_sent_goes_to_the_pane_as_the_binding_names_it_and_needs_one() {
    let send = Action::ConsoleSend(Chord::parse("Escape").ok());
    let run = showing(Area::Console).send(perform(send.clone(), Some("Ctrl+c"), with_pane()));
    assert_eq!(run.last_effects()[0], Effect::PaneKey(Key::new("Escape")));
    let run = showing(Area::Console).send(perform(send, Some("Ctrl+c"), Probe::default()));
    assert!(run.last_effects().is_empty());
}

#[test]
fn the_prompt_s_actions_do_nothing_where_no_prompt_is_shown() {
    for action in [
        Action::PromptFocus,
        Action::PromptWrite,
        Action::PromptSend,
        Action::HistoryPrev,
    ] {
        let run = Scenario::new()
            .event(listing(vec![session("a")]))
            .send(Msg::Show {
                thing: Tab::Terminal(7),
                place: None,
                how: None,
            })
            .send(Msg::Focus(Area::Console))
            .send(perform(action.clone(), Some("i"), with_pane()));
        assert!(run.last_effects().is_empty(), "{action:?}");
        assert_eq!(run.state().content.focus, Area::Console, "{action:?}");
    }
}

#[test]
fn writing_moves_to_the_prompt_and_starts_neovim_writing_unless_it_already_inserts() {
    let run = showing(Area::Console)
        .send(Msg::PromptRuns(true))
        .send(perform(Action::PromptWrite, Some("i"), with_pane()));
    assert_eq!(run.state().content.focus, Area::Prompt);
    assert!(
        run.last_effects()
            .contains(&Effect::PromptKey(Key::new("i")))
    );
    let inserting = Probe {
        inserting: true,
        ..with_pane()
    };
    let run = showing(Area::Console)
        .send(Msg::PromptRuns(true))
        .send(perform(Action::PromptWrite, Some("i"), inserting));
    assert!(
        !run.last_effects()
            .iter()
            .any(|e| matches!(e, Effect::PromptKey(_)))
    );
}

#[test]
fn stepping_through_the_prompts_sent_asks_the_host_and_starts_writing() {
    let run = showing(Area::Console)
        .send(Msg::PromptRuns(true))
        .send(perform(Action::HistoryPrev, Some("Up"), with_pane()));
    assert_eq!(run.state().content.focus, Area::Prompt);
    let effects = run.last_effects();
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::Host(Request::History {
            agent: Agent::Claude,
            ..
        })
    )));
    assert_eq!(effects.last(), Some(&Effect::StartWriting));
    // Neovim's own list moves instead while it shows one.
    let owns = Probe {
        prompt_owns: true,
        ..with_pane()
    };
    let run = showing(Area::Prompt)
        .send(Msg::PromptRuns(true))
        .send(perform(Action::HistoryPrev, Some("Up"), owns));
    assert_eq!(run.last_effects(), [Effect::PromptKey(Key::new("Up"))]);
}

#[test]
fn tab_in_the_prompt_is_handed_to_the_window_with_what_decides_where_it_goes() {
    let owns = Probe {
        prompt_owns: true,
        ..with_pane()
    };
    let run = showing(Area::Prompt)
        .send(Msg::PromptRuns(true))
        .send(perform(Action::PromptTakeSuggestion, None, owns));
    assert_eq!(
        run.last_effects(),
        [Effect::TakeSuggestion {
            key: Key::new("Tab"),
            neovim_owns: true,
            session: true,
            hands_back: false,
            no_images: true,
            tab: Some(Tab::Session("a".into())),
        }]
    );
    let run = showing(Area::Prompt).send(perform(Action::PromptTakeSuggestion, None, with_pane()));
    assert!(run.last_effects().is_empty(), "no Neovim runs");
}

#[test]
fn closing_a_shell_s_row_selects_the_row_above_and_ends_the_shell() {
    let run = showing(Area::Sidebar).send(Msg::TreeSelect(Some(Item::Terminal(7))));
    let above = nav::row_above(&run.state().tree.rows, &Item::Terminal(7));
    let run = run.send(perform(Action::TerminalStop, None, Probe::default()));
    assert_eq!(run.state().tree.selected, above);
    assert_eq!(run.last_effects().last(), Some(&Effect::EndShell(7)));
}

#[test]
fn the_window_s_own_actions_are_handed_to_it() {
    let cases = [
        (Action::SettingsOpen, Effect::OpenSettings),
        (Action::WindowClose, Effect::CloseWindow),
        (Action::WindowOpen, Effect::OpenWindow),
    ];
    for (action, effect) in cases {
        let run = showing(Area::Sidebar).send(perform(action, None, Probe::default()));
        assert_eq!(run.last_effects(), [effect]);
    }
}

#[test]
fn only_a_draft_changes_its_agent() {
    let run = showing(Area::Console).send(perform(Action::SessionAgentNext, None, with_pane()));
    assert_eq!(
        run.state().chrome.status.line.as_deref(),
        Some("Only a session not started yet can change its agent")
    );
    let draft = Tab::draft("/r/a");
    let run = Scenario::new()
        .event(listing(vec![session("a")]))
        .send(Msg::Show {
            thing: draft.clone(),
            place: None,
            how: None,
        })
        .send(perform(Action::SessionAgentNext, None, Probe::default()));
    assert_eq!(
        content::agent_of(run.state(), Some(&draft)),
        Agent::Claude.next()
    );
}

#[test]
fn copying_a_row_puts_its_path_on_the_clipboard() {
    let run = showing(Area::Sidebar)
        .send(Msg::TreeSelect(Some(Item::Terminal(7))))
        .send(perform(Action::CursorCopy, None, Probe::default()));
    assert!(matches!(
        run.last_effects(),
        [Effect::Copy { done, .. }] if done.starts_with("Copied the ")
    ));
}

#[test]
fn a_viewer_shown_takes_the_viewer_s_actions() {
    let run = Scenario::new()
        .send(Msg::Show {
            thing: Tab::Viewer("/r/a/shot.png".into()),
            place: None,
            how: None,
        })
        .send(Msg::Focus(Area::Console))
        .send(perform(Action::ViewerZoomIn, None, Probe::default()));
    assert!(matches!(
        run.last_effects(),
        [Effect::Viewer {
            action: ViewerAction::ZoomIn,
            ..
        }]
    ));
}
