use crate::testing::here;
use adroit_model::{Agent, Key};
use adroit_protocol::Request;

use super::*;
use crate::keymap::Chord;
use crate::scenario::Scenario;
use crate::testing::{SnapshotBuilder, listed, session, session_tab, shell, show, snapshot};

/// The session `a` listed, with the shell 7 in its worktree.
fn listing() -> SnapshotBuilder {
    snapshot().listed(listed().session(session("a")).terminal(shell(7, "/r/a")))
}

/// A window showing the session `a`, the keys in `area`.
fn showing(area: Area) -> Scenario {
    Scenario::new()
        .event(listing())
        .send(Msg::ShowSession(here(), "a".into()))
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
fn a_key_sent_from_the_prompt_is_neovim_s_while_no_session_takes_it() {
    let send = Action::ConsoleSend(None);
    // (a pane shown, the effects)
    let cases = [
        (false, vec![Effect::PromptKey(Key::new("Down"))]),
        (true, vec![Effect::PaneKey(Key::new("Down"))]),
    ];
    for (pane, effects) in cases {
        let probe = Probe {
            pane,
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
        assert_eq!(got, effects, "pane={pane}");
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
            .event(listing())
            .send(show(Tab::Terminal(here(), 7)))
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
        Effect::Host {
            request: Request::History {
                agent: Agent::Claude,
                ..
            },
            ..
        }
    )));
    assert_eq!(effects.last(), Some(&Effect::StartWriting));
    // Asked from the prompt's Neovim, with no key of the window's.
    let run = showing(Area::Prompt)
        .send(Msg::PromptRuns(true))
        .send(perform(Action::HistoryPrev, None, with_pane()));
    assert_eq!(run.last_effects().last(), Some(&Effect::StartWriting));
}

#[test]
fn tab_in_the_prompt_is_handed_to_the_window_with_what_decides_where_it_goes() {
    let run = showing(Area::Prompt)
        .send(Msg::PromptRuns(true))
        .send(perform(Action::PromptTakeSuggestion, None, with_pane()));
    assert_eq!(
        run.last_effects(),
        [Effect::TakeSuggestion {
            key: Key::new("Tab"),
            session: true,
            hands_back: false,
            no_images: true,
            tab: Some(session_tab("a")),
        }]
    );
    let run = showing(Area::Prompt).send(perform(Action::PromptTakeSuggestion, None, with_pane()));
    assert!(run.last_effects().is_empty(), "no Neovim runs");
}

#[test]
fn closing_a_shell_s_row_selects_the_row_above_and_ends_the_shell() {
    let run = showing(Area::Sidebar).send(Msg::TreeSelect(Some(Item::Terminal(here(), 7))));
    let above = nav::row_above(&run.state().tree.rows, &Item::Terminal(here(), 7));
    let run = run.send(perform(Action::TerminalStop, None, Probe::default()));
    assert_eq!(run.state().tree.selected, above);
    assert_eq!(
        run.last_effects().last(),
        Some(&Effect::EndShell {
            environment: here(),
            id: 7,
        })
    );
}

#[test]
fn the_window_s_own_actions_are_handed_to_it() {
    let cases = [
        (Action::SettingsOpen, Effect::OpenSettings(here())),
        (Action::WindowClose, Effect::CloseWindow),
        (Action::WindowOpen, Effect::OpenWindow),
    ];
    for (action, effect) in cases {
        let run = showing(Area::Sidebar).send(perform(action, None, Probe::default()));
        assert_eq!(run.last_effects(), [effect]);
    }
}

#[test]
fn copying_a_row_puts_its_path_on_the_clipboard() {
    let run = showing(Area::Sidebar)
        .send(Msg::TreeSelect(Some(Item::Terminal(here(), 7))))
        .send(perform(Action::CursorCopy, None, Probe::default()));
    assert!(matches!(
        run.last_effects(),
        [Effect::Copy { done, .. }] if done.starts_with("Copied the ")
    ));
}

#[test]
fn a_viewer_shown_takes_the_viewer_s_actions() {
    let run = Scenario::new()
        .send(show(Tab::Viewer(here(), "/r/a/shot.png".into())))
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

/// The session `a` shown, `b` listed beside it, and the keys in the tree on
/// `b`'s row.
fn on_the_row_of_b() -> Scenario {
    Scenario::new()
        .event(snapshot().listed(listed().session(session("a")).session(session("b"))))
        .send(Msg::ShowSession(here(), "a".into()))
        .send(Msg::Focus(Area::Sidebar))
        .send(Msg::OpenRow {
            item: Item::Session(here(), "b".into()),
            how: Some(Opening::Preview),
            take_keys: false,
        })
        .send(Msg::Focus(Area::Sidebar))
}

fn previewing(run: &Scenario) -> Vec<bool> {
    let windows = &run.state().content.windows;
    windows.tabs().iter().map(|t| t.is_preview()).collect()
}

#[test]
fn looking_at_a_row_shows_it_in_the_preview_with_the_keys_staying_in_the_tree() {
    let run = on_the_row_of_b().send(perform(Action::CursorShow, None, Probe::default()));
    assert_eq!(previewing(&run), [false, true]);
    assert_eq!(run.state().content.active(), Some(&session_tab("b")));
    assert_eq!(run.state().content.focus, Area::Sidebar);
}

#[test]
fn opening_the_row_the_preview_shows_keeps_it_and_takes_the_keys() {
    let run = on_the_row_of_b().send(perform(Action::CursorGo, None, Probe::default()));
    assert_eq!(previewing(&run), [false, false]);
    assert_eq!(run.state().content.active(), Some(&session_tab("b")));
    assert_eq!(run.state().content.focus, Area::Console);
}

#[test]
fn keys_sent_to_what_the_preview_shows_keep_it() {
    let send = Action::ConsoleSend(None);
    let run = on_the_row_of_b()
        .send(Msg::Focus(Area::Console))
        .send(perform(send, Some("x"), with_pane()));
    assert_eq!(previewing(&run), [false, false]);
}

#[test]
fn the_preview_waits_while_the_keys_only_move_into_it() {
    let run = on_the_row_of_b().send(Msg::Focus(Area::Console));
    assert_eq!(previewing(&run), [false, true]);
    let run = run.send(Msg::Focus(Area::Prompt));
    assert_eq!(previewing(&run), [false, false]);
}

#[test]
fn going_to_the_next_session_looks_at_it_in_the_preview() {
    let run = Scenario::new()
        .event(snapshot().listed(listed().session(session("a")).session(session("b"))))
        .send(Msg::ShowSession(here(), "a".into()))
        .send(Msg::Window(Action::SessionNext));
    assert_eq!(previewing(&run), [false, true]);
    // Back on a session a tab shows, the preview goes.
    let run = run.send(Msg::Window(Action::SessionNext));
    assert_eq!(previewing(&run), [false]);
    assert_eq!(run.state().content.active(), Some(&session_tab("a")));
}

#[test]
fn looking_at_a_row_that_folds_goes_into_it_rather_than_folding_it() {
    use crate::tree::nav::Heading;
    let heading = Item::Heading(here(), "/r/a".into(), Heading::Sessions);
    let run = showing(Area::Sidebar)
        .send(Msg::TreeSelect(Some(heading.clone())))
        .send(perform(Action::CursorShow, None, Probe::default()));
    assert!(run.state().tree.folds.is_open(&heading));
    assert_eq!(
        run.state().tree.selected,
        Some(Item::Session(here(), "a".into()))
    );
}

#[test]
fn a_draft_switches_its_profile_keeping_options_within_an_agent_and_resetting_them_across() {
    const PROFILES: &str = "profile = \"claude\"\n[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n[[profiles]]\nid = \"work\"\nname = \"Work\"\nagent = \"claude\"\nhome = \"/w\"\n[[profiles]]\nid = \"old\"\nagent = \"claude\"\narchived = true\n[[profiles]]\nid = \"review\"\nname = \"Review\"\nagent = \"codex\"\n";
    let draft = Tab::draft(here(), "/r/a");
    let chosen = content::DraftOptions {
        model: Some("opus".into()),
        permissions: adroit_model::AgentPermissions::ReadOnly,
        ..Default::default()
    };
    let next = || perform(Action::SessionProfileNext, None, Probe::default());
    let run =
        Scenario::new()
            .event(listing().with(|w| {
                w.config.settings = Some(crate::config::testing::settings_file(PROFILES))
            }))
            .send(show(draft.clone()))
            .send(Msg::DraftOptions {
                tab: Some(draft.clone()),
                options: chosen.clone(),
            });
    let at = |run: &Scenario| {
        (
            content::profile_of(run.state(), Some(&draft)).to_string(),
            content::home_of(run.state(), Some(&draft)),
            content::agent_of(run.state(), Some(&draft)),
        )
    };
    assert_eq!(at(&run), ("claude".into(), None, Agent::Claude));

    let run = run.send(next());
    assert_eq!(at(&run), ("work".into(), Some("/w".into()), Agent::Claude));
    let options = run.state().content.drafts.options[&draft].clone();
    assert_eq!(options.model, chosen.model, "kept within an agent");
    assert_eq!(options.permissions, chosen.permissions);

    // A profile written archived is not there to pass to.
    let run = run.send(next());
    assert_eq!(at(&run), ("review".into(), None, Agent::Codex));
    let options = run.state().content.drafts.options[&draft].clone();
    assert_eq!(options.model, None);
    assert_eq!(
        options.permissions,
        adroit_model::AgentPermissions::default()
    );
    let told = run.state().chrome.status.line.clone().unwrap_or_default();
    assert!(told.contains("back to Review's defaults"), "{told}");

    let run = run.send(perform(Action::SessionProfilePrev, None, Probe::default()));
    assert_eq!(at(&run).0, "work");
    // A new draft starts from the profile last chosen.
    let other = Tab::draft(here(), "/r/b");
    let run = run.send(show(other.clone()));
    assert_eq!(
        content::profile_of(run.state(), Some(&other)).as_str(),
        "work"
    );
}

#[test]
fn only_a_draft_changes_its_profile() {
    let run = showing(Area::Console).send(perform(Action::SessionProfileNext, None, with_pane()));
    assert_eq!(
        run.state().chrome.status.line.as_deref(),
        Some("Only a session not started yet can change its profile")
    );
}

#[test]
fn in_the_details_panel_the_tree_s_ways_of_opening_open_the_row_the_same_ways() {
    // (action, the key it presses on the panel's list)
    let cases = [
        (Action::CursorGo, details::Key::Open),
        (Action::CursorShow, details::Key::OpenIn(Opening::Preview)),
        (
            Action::CursorSplit,
            details::Key::OpenIn(Opening::Split(Axis::Column)),
        ),
        (
            Action::CursorVsplit,
            details::Key::OpenIn(Opening::Split(Axis::Row)),
        ),
        (Action::CursorTab, details::Key::OpenIn(Opening::Tab)),
    ];
    for (action, want) in cases {
        assert_eq!(details_key(action.clone()), Some(want), "{action:?}");
    }
    assert_eq!(details_key(Action::CursorClose), None);
}

#[test]
fn a_drawn_image_s_window_takes_the_viewer_s_actions() {
    let drawn = Tab::Drawn {
        environment: here(),
        session: "a".into(),
        id: 7,
        width: 64,
        height: 32,
    };
    let run = showing(Area::Console).send(show(drawn)).send(perform(
        Action::ViewerZoomIn,
        None,
        Probe::default(),
    ));
    assert!(
        matches!(
            run.last_effects(),
            [Effect::Viewer {
                action: ViewerAction::ZoomIn,
                ..
            }]
        ),
        "{:?}",
        run.last_effects()
    );
}

#[test]
fn removing_an_environment_asks_first_and_then_has_the_window_archive_it() {
    use crate::overlay::confirm::Ask;
    use crate::testing::{shows_there, there};

    let asked = shows_there(showing(Area::Sidebar), [session("b")])
        .send(Msg::TreeSelect(Some(Item::Environment(there()))))
        .send(perform(Action::EnvironmentRemove, None, Probe::default()));
    assert_eq!(
        asked.state().overlays.confirm,
        Some(Ask::RemoveEnvironment(there(), "There".into()))
    );
    assert!(asked.last_effects().is_empty());

    let answered = asked.send(Msg::ConfirmAnswer(true));
    assert_eq!(
        answered.last_effects(),
        [Effect::ArchiveEnvironment(there())]
    );
    assert!(
        answered.state().hosts.get(&there()).is_some(),
        "listed until the catalog no longer keeps it"
    );
}

#[test]
fn an_environment_no_longer_kept_drops_its_host_and_tabs_and_tells_the_window() {
    use crate::testing::{shows_there, there};

    let run = shows_there(showing(Area::Sidebar), [session("b")])
        .send(Msg::ShowSession(there(), "b".into()))
        .send(Msg::TreeSelect(Some(Item::Environment(there()))))
        .send(Msg::EnvironmentRemoved(there()));
    assert!(run.state().hosts.get(&there()).is_none());
    assert!(
        run.state()
            .tree
            .selected
            .as_ref()
            .is_none_or(|row| row.environment() != &there())
    );
    assert!(
        run.state()
            .content
            .windows
            .shown()
            .iter()
            .all(|t| t.environment() != &there())
    );
    assert!(
        run.last_effects()
            .contains(&Effect::EnvironmentClosed(there()))
    );
    assert!(
        !run.last_effects().iter().any(|e| matches!(
            e,
            Effect::Host {
                request: Request::Stop { .. } | Request::Kill { .. },
                ..
            }
        )),
        "its sessions keep running"
    );
}

#[test]
fn the_environment_the_window_was_started_in_is_never_removed_from_it() {
    use crate::testing::{shows_there, there};

    let asked = shows_there(showing(Area::Sidebar), [session("b")])
        .send(Msg::TreeSelect(Some(Item::Environment(here()))))
        .send(perform(Action::EnvironmentRemove, None, Probe::default()));
    assert_eq!(asked.state().overlays.confirm, None);
    assert!(asked.last_effects().is_empty());

    let gone = asked.send(Msg::EnvironmentRemoved(here()));
    assert!(gone.state().hosts.get(&here()).is_some());
    assert!(gone.state().hosts.get(&there()).is_some());
    assert!(gone.last_effects().is_empty());
}

#[test]
fn the_environments_window_is_opened_from_anywhere() {
    for area in [Area::Sidebar, Area::Console] {
        let run = showing(area).send(perform(Action::EnvironmentsOpen, None, Probe::default()));
        assert_eq!(run.last_effects(), [Effect::OpenEnvironments], "{area:?}");
    }
}

#[test]
fn the_settings_open_for_the_environment_of_the_selected_row_or_else_of_the_tab_shown() {
    use crate::testing::{shows_there, there};

    let opened = |run: Scenario| {
        run.send(perform(Action::SettingsOpen, None, Probe::default()))
            .last_effects()
            .to_vec()
    };
    let showing_there = || {
        shows_there(showing(Area::Sidebar), [session("b")])
            .send(Msg::ShowSession(there(), "b".into()))
    };
    // (where the keys are, the row selected, the environment)
    let cases = [
        (Area::Console, None, there()),
        (
            Area::Sidebar,
            Some(Item::Session(here(), "a".into())),
            here(),
        ),
        (Area::Sidebar, Some(Item::Environment(there())), there()),
    ];
    for (area, row, environment) in cases {
        let run = showing_there()
            .send(Msg::TreeSelect(row.clone()))
            .send(Msg::Focus(area));
        assert_eq!(
            opened(run),
            [Effect::OpenSettings(environment.clone())],
            "{area:?} {row:?}"
        );
    }
}

#[test]
fn the_daemon_restart_is_asked_for_the_environment_of_the_selected_row_or_else_of_the_tab_shown() {
    use crate::overlay::confirm::Ask;
    use crate::testing::{shows_there, there};

    let showing_there = || {
        shows_there(showing(Area::Sidebar), [session("b")])
            .send(Msg::ShowSession(there(), "b".into()))
    };
    // (where the keys are, the row selected, the environment)
    let cases = [
        (Area::Console, None, there()),
        (
            Area::Sidebar,
            Some(Item::Session(here(), "a".into())),
            here(),
        ),
        (Area::Sidebar, Some(Item::Environment(there())), there()),
    ];
    for (area, row, environment) in cases {
        let run = showing_there()
            .send(Msg::TreeSelect(row.clone()))
            .send(Msg::Focus(area))
            .send(perform(Action::DaemonRestart, None, Probe::default()));
        assert_eq!(
            run.state().overlays.confirm,
            Some(Ask::RestartDaemon(environment.clone())),
            "{area:?} {row:?}"
        );
    }
}

#[test]
fn an_environment_no_longer_kept_takes_its_daemon_s_banner_away() {
    use crate::testing::{shows_there, there};

    let run = shows_there(showing(Area::Sidebar), [session("b")])
        .send(Msg::DaemonStanding {
            environment: there(),
            note: Some("old".into()),
        })
        .send(Msg::EnvironmentRemoved(there()));
    assert_eq!(run.state().overlays.banner.shown(), None);
}
