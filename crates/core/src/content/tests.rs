use std::collections::BTreeSet;

use adroit_model::{TerminalProgram, Wait};
use adroit_protocol::Request;

use super::*;
use crate::actions::Action;
use crate::details::Sheet;
use crate::layout::Side;
use crate::msg::Msg;
use crate::scenario::Scenario;
use crate::testing::{
    SessionBuilder, listed, listing, patch, session, session_tab, shell, show, snapshot,
};
use crate::tree::nav::Item;

fn place(worktree: &str) -> Option<Place> {
    Some(Place {
        worktree: worktree.into(),
    })
}

fn active(run: &Scenario) -> Option<Tab> {
    run.state().content.active().cloned()
}

fn shown(run: &Scenario) -> Vec<Tab> {
    run.state()
        .content
        .windows
        .shown()
        .into_iter()
        .cloned()
        .collect()
}

/// The window opened, having kept the content showing `kept` alone.
fn opened(kept: Option<Tab>) -> Msg {
    let mut windows = Content::default();
    if let Some(tab) = kept {
        windows.open(tab);
    }
    opened_with(&windows)
}

fn opened_with(windows: &Content<Tab>) -> Msg {
    Msg::ContentOpened {
        kept: windows.kept(kept_name),
        scratch: "/scratch".into(),
    }
}

/// The window as it stands once the first listing came: what it kept
/// had its chance to come back.
fn listed_with(sessions: impl IntoIterator<Item = SessionBuilder>) -> Scenario {
    Scenario::new().send(opened(None)).event(listing(sessions))
}

fn live(ids: &[&str]) -> Msg {
    Msg::PanesLive(ids.iter().map(|id| id.to_string()).collect())
}

#[test]
fn a_window_follows_its_session_once_it_is_taken_over() {
    let own = session("own").cwd("/r").session_id("pid-1");
    let run = listed_with([session("pid-1").cwd("/r")])
        .send(show(session_tab("pid-1")))
        .event(patch(2).listed(listed().session(own)));
    assert_eq!(shown(&run), [Tab::Session("own".into())]);
}

#[test]
fn a_session_that_ends_gives_its_windows_up_and_is_said_to_have_ended() {
    let run = listed_with([session("a").cwd("/r"), session("b").cwd("/r")])
        .send(show(session_tab("a")))
        .event(patch(2).listed(listed().session(session("b").cwd("/r"))));
    // The worktree's first session shows once nothing else is open.
    assert_eq!(shown(&run), [Tab::Session("b".into())]);
    assert_eq!(
        run.state().chrome.status.line.as_deref(),
        Some("a-name ended")
    );
}

#[test]
fn a_session_live_here_stays_though_it_is_not_listed() {
    // (what holds it, whether it stays shown)
    let cases: Vec<(Vec<Msg>, bool)> = vec![(vec![], false), (vec![live(&["a"])], true)];
    for (held, stays) in cases {
        let run = listed_with([session("a").cwd("/r")])
            .send(show(session_tab("a")))
            .send_all(held.clone())
            .event(patch(2).listed(listed()));
        assert_eq!(
            shown(&run).contains(&Tab::Session("a".into())),
            stays,
            "{held:?}"
        );
    }
}

#[test]
fn a_session_no_longer_live_is_given_up_and_said_to_have_ended_once_told() {
    let run = listed_with([session("a").cwd("/r")])
        .send(show(session_tab("a")))
        .send(live(&["a"]))
        .event(patch(2).listed(listed()));
    assert_eq!(shown(&run), [Tab::Session("a".into())]);
    let gone = run.send(live(&[]));
    assert_eq!(shown(&gone), Vec::<Tab>::new());
    assert_eq!(
        gone.state().chrome.status.line.as_deref(),
        Some("a-name ended")
    );
}

#[test]
fn the_panes_kept_are_those_of_what_a_new_listing_lists() {
    let run = listed_with([session("a").cwd("/r")]);
    let kept = run
        .last_effects()
        .iter()
        .filter(|e| matches!(e, Effect::KeepPanes { .. }))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        kept,
        [Effect::KeepPanes {
            sessions: BTreeSet::from(["a".to_string()]),
            terminals: BTreeSet::new(),
        }]
    );
    // A pane started since the listing stays until the host lists anew.
    let told = run.send(live(&["new"]));
    assert!(
        !told
            .last_effects()
            .iter()
            .any(|e| matches!(e, Effect::KeepPanes { .. }))
    );
}

#[test]
fn the_tab_kept_from_the_last_run_comes_back_once_listed() {
    let a = Tab::Session("a".into());
    // (opened before the listing, what shows after it)
    let cases = [
        (None, Some(a.clone())),
        (
            Some(Tab::Session("b".into())),
            Some(Tab::Session("b".into())),
        ),
    ];
    for (before, after) in cases {
        let run = Scenario::new()
            .send(opened(Some(a.clone())))
            .send_all(before.clone().map(show))
            .event(listing([session("a"), session("b")]));
        assert_eq!(active(&run), after, "{before:?}");
        if before.is_none() {
            assert_eq!(run.state().content.place, place("/r/a"));
            assert_eq!(run.state().tree.selected, Some(Item::Session("a".into())));
        }
    }
}

#[test]
fn a_kept_tab_comes_back_when_the_sessions_were_listed_before_the_window_opened() {
    let kept = [Tab::Session("a".into()), Tab::Terminal(7)];
    for tab in kept {
        let run = Scenario::new()
            .event(snapshot().listed(listed().session(session("a")).terminal(shell(7, "/r/a"))))
            .send(opened(Some(tab.clone())));
        assert_eq!(active(&run), Some(tab.clone()), "{tab:?}");
    }
}

#[test]
fn the_prompt_of_a_kept_tab_that_comes_back_is_that_session_s() {
    let run = Scenario::new()
        .event(snapshot().listed(listed().session(session("a"))))
        .send(opened(Some(Tab::Session("a".into()))));
    let told: Vec<&Option<Tab>> = run
        .effects()
        .iter()
        .filter_map(|e| match e {
            Effect::PromptShow(tab) => Some(tab),
            _ => None,
        })
        .collect();
    assert_eq!(told, [&Some(Tab::Session("a".into()))]);
}

#[test]
fn a_kept_tab_not_listed_at_first_is_given_up() {
    let run = Scenario::new()
        .send(opened(Some(Tab::Session("a".into()))))
        .event(snapshot().listed(listed()))
        .event(patch(2).listed(listed().session(session("a").cwd("/r"))));
    assert_eq!(active(&run), None);
}

fn kept_content(run: &Scenario) -> Vec<Kept<String>> {
    run.last_effects()
        .iter()
        .filter_map(|e| match e {
            Effect::KeepContent(k) => Some(k.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_content_is_kept_for_the_next_run_as_it_changes_once_the_kept_content_had_its_chance() {
    let shows = |run: &Scenario| vec![run.state().content.windows.kept(kept_name)];
    let before = Scenario::new()
        .send(opened(None))
        .send(show(session_tab("a")));
    assert!(
        !before
            .effects()
            .iter()
            .any(|e| matches!(e, Effect::KeepContent(_)))
    );
    let listed = before.event(listing([session("a").cwd("/r")]));
    assert_eq!(kept_content(&listed), shows(&listed));
    let split = listed.send(Msg::Window(Action::WindowVsplit));
    assert_eq!(kept_content(&split), shows(&split));
    assert_eq!(shows(&split)[0].tabs.len(), 1);
    let border = split.state().content.windows.current().unwrap().borders()[0].clone();
    let dragging = split
        .send(Msg::WindowDragStart {
            border,
            from: 500.0,
            px: 1000.0,
        })
        .send(Msg::WindowDragMove(600.0));
    assert!(kept_content(&dragging).is_empty(), "kept once dropped");
    let dropped = dragging.send(Msg::WindowDragEnd);
    assert_eq!(kept_content(&dropped), shows(&dropped));
    let closed = dropped.send(Msg::Window(Action::TabClose));
    assert_eq!(kept_content(&closed), [Kept::default()]);
}

#[test]
fn the_content_kept_from_the_last_run_comes_back_once_listed_without_what_cannot() {
    let mut was = Content::default();
    was.open(Tab::Session("a".into()));
    was.split(Axis::Row, Tab::Session("gone".into()));
    was.split(Axis::Column, Tab::Terminal(7));
    was.open_tab(Tab::Session("b".into()));
    was.open_tab(Tab::Session("gone".into()));
    was.go_to_tab(0);
    let run = Scenario::new().send(opened_with(&was)).event(
        snapshot().listed(
            listed()
                .sessions([session("a"), session("b")])
                .terminal(shell(7, "/r/a")),
        ),
    );
    let mut ended = was.clone();
    ended.close_showing(|t| *t == Tab::Session("gone".into()));
    let windows = &run.state().content.windows;
    assert_eq!(windows.kept(kept_name), ended.kept(kept_name));
    assert_eq!(windows.tabs().len(), 2);
    assert_eq!(active(&run), Some(Tab::Terminal(7)));
    assert_eq!(run.state().content.place, place("/r/a"));
}

#[test]
fn a_kept_content_of_which_nothing_can_come_back_opens_empty() {
    let mut was = Content::default();
    was.open(Tab::Session("gone".into()));
    was.open_tab(Tab::Terminal(9));
    let run = Scenario::new()
        .send(opened_with(&was))
        .event(listing([session("a")]));
    assert!(run.state().content.windows.is_empty());
}

#[test]
fn a_session_that_starts_waiting_is_told_of_after_the_first_listing() {
    let waits = session("a").cwd("/r").waiting(Wait::Permission);
    let notifying = || {
        Scenario::new().event(crate::config::testing::configured(
            "notify_waiting = true\n",
        ))
    };
    let first = notifying()
        .send(opened(None))
        .event(listing([waits.clone()]));
    assert!(
        !first
            .effects()
            .iter()
            .any(|e| matches!(e, Effect::NotifyWaiting(_)))
    );
    let later = notifying()
        .send(opened(None))
        .event(listing([session("a").cwd("/r")]))
        .event(patch(2).listed(listed().session(waits)));
    assert_eq!(
        later
            .last_effects()
            .iter()
            .filter(|e| matches!(e, Effect::NotifyWaiting(_)))
            .cloned()
            .collect::<Vec<_>>(),
        [Effect::NotifyWaiting(vec![(
            "a-name".into(),
            "permission prompt".into()
        )])]
    );
}

#[test]
fn the_middle_follows_what_the_focused_window_shows() {
    let run = listed_with([session("a"), session("b")]).send(show(session_tab("a")));
    assert_eq!(run.state().content.place, place("/r/a"));
    let b = run.send(show(session_tab("b")));
    assert_eq!(b.state().content.place, place("/r/b"));
    // A tab whose thing is placed nowhere leaves the middle where it is.
    let nowhere = b.send(show(Tab::Viewer("/elsewhere/a.png".into())));
    assert_eq!(nowhere.state().content.place, place("/r/b"));
}

#[test]
fn a_session_shown_before_it_is_listed_is_placed_once_it_is() {
    let run = listed_with([])
        .send(Msg::Show {
            thing: Tab::Session("new".into()),
            place: place("/r"),
            how: None,
        })
        .event(patch(2).listed(listed().session(session("new").cwd("/r/w"))));
    assert_eq!(run.state().content.place, place("/r/w"));
}

#[test]
fn the_session_last_shown_in_a_workspace_is_remembered() {
    let run = listed_with([session("a").cwd("/r"), session("b").cwd("/r")])
        .send(show(session_tab("a")))
        .send(show(session_tab("b")));
    assert_eq!(
        run.state()
            .content
            .last_shown
            .get(std::path::Path::new("/r")),
        Some(&"b".to_string())
    );
}

#[test]
fn a_draft_no_window_shows_is_given_up_with_its_options() {
    let run = Scenario::new().send(Msg::Draft {
        worktree: "/w".into(),
        write: false,
    });
    let draft = active(&run).unwrap();
    assert!(run.state().content.drafts.options.contains_key(&draft));
    let closed = run.send(Msg::Window(Action::TabClose));
    assert!(closed.state().content.drafts.options.is_empty());
}

#[test]
fn a_draft_takes_the_options_chosen_with_nothing_open() {
    let chosen = DraftOptions {
        model: Some("m".into()),
        ..Default::default()
    };
    let run = Scenario::new()
        .send(Msg::DraftOptions {
            tab: None,
            options: chosen.clone(),
        })
        .send(Msg::Draft {
            worktree: "/w".into(),
            write: true,
        });
    let draft = active(&run).unwrap();
    assert_eq!(
        run.state().content.drafts.options.get(&draft),
        Some(&chosen)
    );
    assert_eq!(run.state().content.drafts.next, None);
    assert_eq!(run.state().content.focus, Area::Prompt);
    assert!(run.last_effects().contains(&Effect::StartWriting));
    assert_eq!(run.state().content.place, place("/w"));
}

#[test]
fn closing_the_last_window_of_a_draft_hands_its_prompt_on() {
    // (shown besides the draft, where its prompt goes)
    let cases = [
        (None, None),
        (
            Some(Tab::Session("a".into())),
            Some(Tab::Session("a".into())),
        ),
        (Some(Tab::Terminal(1)), None),
    ];
    for (besides, to) in cases {
        let run = Scenario::new()
            .send_all(besides.clone().map(show))
            .send_all(besides.iter().map(|_| Msg::Window(Action::WindowSplit)))
            .send(Msg::Draft {
                worktree: "/w".into(),
                write: false,
            })
            .send(Msg::Window(Action::WindowQuit));
        assert!(
            run.last_effects()
                .contains(&Effect::PromptClose(to.clone())),
            "{besides:?}: {:?}",
            run.last_effects()
        );
    }
}

#[test]
fn hiding_the_panel_with_the_keys_gives_them_to_the_console() {
    // (the keys' area, the panel hidden, where the keys are after)
    let cases = [
        (Area::Sidebar, Side::Sidebar, Area::Console),
        (Area::Details, Side::Details, Area::Console),
        (Area::Sidebar, Side::Details, Area::Sidebar),
        (Area::Prompt, Side::Sidebar, Area::Prompt),
    ];
    for (area, side, after) in cases {
        let run = Scenario::new()
            .send(Msg::Focus(area))
            .send(Msg::PanelToggle(side));
        assert_eq!(run.state().content.focus, after, "{area:?} {side:?}");
    }
}

#[test]
fn a_panel_hidden_from_the_start_leaves_the_keys_where_they_are() {
    let layout = crate::layout::Layout {
        show_sidebar: false,
        ..Default::default()
    };
    let run = Scenario::new().send(Msg::PanelsKept(layout));
    assert_eq!(run.state().content.focus, Area::Sidebar);
}

#[test]
fn the_focused_window_is_marked_only_while_the_content_has_the_keys() {
    let shown = || Scenario::new().send(show(session_tab("a")));
    let window = shown().state().content.windows.focused_window();
    assert!(window.is_some());
    // (the keys' area, whether the focused window is marked)
    let cases = [
        (Area::Sidebar, false),
        (Area::Details, false),
        (Area::Console, true),
        (Area::Prompt, true),
    ];
    for (area, marked) in cases {
        let at = shown().send(Msg::Focus(area));
        let expected = if marked { window } else { None };
        assert_eq!(at.state().content.marked_window(), expected, "{area:?}");
    }
    // Coming back from the tree marks the window the keys left.
    let back = shown()
        .send(Msg::Focus(Area::Sidebar))
        .send(Msg::Focus(Area::Console));
    assert_eq!(back.state().content.marked_window(), window);
}

#[test]
fn a_shell_shown_takes_the_keys_from_the_prompt() {
    let run = Scenario::new()
        .send(show(session_tab("a")))
        .send(Msg::Focus(Area::Prompt))
        .send(show(Tab::Terminal(1)));
    assert_eq!(run.state().content.focus, Area::Console);
}

#[test]
fn coming_into_the_prompt_asks_the_console_for_what_was_typed() {
    let run = Scenario::new()
        .send(show(session_tab("a")))
        .send(Msg::Focus(Area::Console));
    assert!(!run.last_effects().contains(&Effect::ConsoleLive));
    let into = run.send(Msg::Focus(Area::Prompt));
    assert!(into.last_effects().contains(&Effect::ConsoleLive));
    let again = into.send(Msg::Focus(Area::Prompt));
    assert!(!again.last_effects().contains(&Effect::ConsoleLive));
}

#[test]
fn the_window_back_key_returns_to_where_the_keys_were() {
    let run = Scenario::new()
        .send(show(session_tab("a")))
        .send(Msg::Focus(Area::Console))
        .send(Msg::Focus(Area::Details))
        .send(Msg::Window(Action::WindowBack));
    assert_eq!(run.state().content.focus, Area::Console);
}

#[test]
fn the_window_keys_leave_a_hidden_panel_alone() {
    let run = Scenario::new()
        .send(show(session_tab("a")))
        .send(Msg::Focus(Area::Console))
        .send(Msg::PanelToggle(Side::Sidebar))
        .send(Msg::Window(Action::WindowLeft));
    assert_eq!(run.state().content.focus, Area::Console);
}

#[test]
fn the_window_keys_split_close_and_go_through_tabs() {
    let a = Tab::Session("a".into());
    let split = || {
        Scenario::new()
            .send(show(a.clone()))
            .send(Msg::Window(Action::WindowVsplit))
    };
    let two = split();
    assert_eq!(
        two.state()
            .content
            .windows
            .current()
            .unwrap()
            .windows()
            .len(),
        2
    );
    let one = split().send(Msg::Window(Action::WindowOnly));
    assert_eq!(
        one.state()
            .content
            .windows
            .current()
            .unwrap()
            .windows()
            .len(),
        1
    );
    let tabs = two
        .send(Msg::Window(Action::WindowToTab))
        .send(Msg::Window(Action::TabGo(1)));
    assert_eq!(tabs.state().content.windows.current_index(), 0);
    let next = tabs.send(Msg::Window(Action::TabNext));
    assert_eq!(next.state().content.windows.current_index(), 1);
}

#[test]
fn the_tab_keys_step_the_details_sheets_while_the_details_have_the_keys() {
    let run = Scenario::new()
        .send(show(session_tab("a")))
        .send(Msg::Focus(Area::Details))
        .send(Msg::Window(Action::TabNext));
    assert_eq!(run.state().details.drill.sheet(), Sheet::Changes);
}

#[test]
fn a_new_window_splits_off_a_draft_in_the_middle_s_worktree() {
    // (the middle, where the draft runs)
    let cases = [(place("/w"), "/w"), (None, "/scratch")];
    for (middle, at) in cases {
        let run = Scenario::new()
            .send(opened(None))
            .send(Msg::Show {
                thing: Tab::Terminal(1),
                place: middle.clone(),
                how: None,
            })
            .send(Msg::Window(Action::WindowNew));
        let draft = active(&run);
        assert!(
            matches!(&draft, Some(Tab::Draft { worktree, .. }) if worktree == std::path::Path::new(at)),
            "{middle:?}: {draft:?}"
        );
        assert_eq!(run.state().content.windows.shown().len(), 2);
        assert_eq!(run.state().content.focus, Area::Prompt);
    }
}

#[test]
fn a_new_session_or_shell_starts_in_the_selected_row_s_worktree_from_the_tree() {
    let base = || {
        listed_with([session("a"), session("b")])
            .send(show(session_tab("a")))
            .send(Msg::TreeSelect(Some(Item::Session("b".into()))))
    };
    // (where the keys are, the worktree)
    let cases = [(Area::Sidebar, "/r/b"), (Area::Console, "/r/a")];
    for (area, at) in cases {
        let shell = base()
            .send(Msg::Focus(area))
            .send(Msg::Window(Action::ShellOpen));
        assert_eq!(
            shell.last_effects(),
            [Effect::StartTerminal(TerminalAsk {
                place: place(at).unwrap(),
                runs: TerminalProgram::Shell,
                file: None,
                opening: Opening::Existing,
                focus: true,
            })],
            "{area:?}"
        );
        let new = base()
            .send(Msg::Focus(area))
            .send(Msg::Window(Action::SessionNew));
        assert!(
            matches!(active(&new), Some(Tab::Draft { worktree, .. }) if worktree == std::path::Path::new(at)),
            "{area:?}"
        );
    }
}

#[test]
fn opening_a_row_shows_what_it_stands_for_and_may_take_the_keys() {
    let run = listed_with([session("a").cwd("/r")]).send(Msg::OpenRow {
        item: Item::Session("a".into()),
        how: Some(Opening::Existing),
        take_keys: false,
    });
    assert_eq!(active(&run), Some(Tab::Session("a".into())));
    assert_eq!(run.state().content.focus, Area::Sidebar);
    assert_eq!(run.state().tree.selected, Some(Item::Session("a".into())));
    let keys = listed_with([session("a").cwd("/r")]).send(Msg::OpenRow {
        item: Item::Session("a".into()),
        how: Some(Opening::Existing),
        take_keys: true,
    });
    assert_eq!(keys.state().content.focus, Area::Console);
}

#[test]
fn a_row_clicked_opens_as_cursor_go_opens_it() {
    let item = Item::Session("a".into());
    let clicked = listed_with([session("a").cwd("/r")]).send(Msg::RowClicked(item.clone()));
    let gone = listed_with([session("a").cwd("/r")]).send(Msg::OpenRow {
        item,
        how: Some(Opening::Existing),
        take_keys: true,
    });
    assert_eq!(active(&clicked), active(&gone));
    assert_eq!(clicked.state().content.focus, Area::Console);
    assert_eq!(clicked.last_effects(), gone.last_effects());
}

#[test]
fn opening_a_file_s_row_starts_a_neovim_on_it() {
    let file = Item::File {
        worktree: "/r".into(),
        path: "/r/a.rs".into(),
        dir: false,
    };
    let run = listed_with([session("a").cwd("/r")]).send(Msg::OpenRow {
        item: file,
        how: Some(Opening::Tab),
        take_keys: true,
    });
    assert_eq!(
        run.last_effects()
            .iter()
            .filter(|e| matches!(e, Effect::StartTerminal(_)))
            .cloned()
            .collect::<Vec<_>>(),
        [Effect::StartTerminal(TerminalAsk {
            place: place("/r").unwrap(),
            runs: TerminalProgram::Neovim,
            file: Some("/r/a.rs".into()),
            opening: Opening::Tab,
            focus: true,
        })]
    );
}

#[test]
fn a_session_is_shown_wherever_it_runs_and_its_row_selected() {
    let run = listed_with([session("a"), session("b")])
        .send(show(session_tab("a")))
        .send(Msg::ShowSession("b".into()));
    assert_eq!(active(&run), Some(Tab::Session("b".into())));
    assert_eq!(run.state().content.place, place("/r/b"));
    assert_eq!(run.state().tree.selected, Some(Item::Session("b".into())));
    let next = run.send(Msg::Window(Action::SessionNext));
    assert_eq!(active(&next), Some(Tab::Session("a".into())));
}

#[test]
fn a_closed_shell_gives_its_windows_up_to_another_thing_of_its_worktree() {
    let run = Scenario::new()
        .send(opened(None))
        .event(
            snapshot().listed(
                listed()
                    .session(session("a").cwd("/r"))
                    .terminal(shell(1, "/r")),
            ),
        )
        .send(Msg::Show {
            thing: Tab::Terminal(1),
            place: place("/r"),
            how: None,
        })
        .send(Msg::ShellClosed(1));
    assert_eq!(active(&run), Some(Tab::Session("a".into())));
    assert!(
        run.last_effects()
            .contains(&Effect::Host(Request::TerminalClosed { id: 1 }))
    );
    assert!(
        run.last_effects()
            .contains(&Effect::PromptClose(Some(Tab::Session("a".into()))))
    );
}

#[test]
fn dragging_a_border_resizes_the_windows_beside_it() {
    let run = Scenario::new()
        .send(show(session_tab("a")))
        .send(Msg::Window(Action::WindowVsplit));
    let border = run.state().content.windows.current().unwrap().borders()[0].clone();
    let dragged = run
        .send(Msg::WindowDragStart {
            border: border.clone(),
            from: 500.0,
            px: 1000.0,
        })
        .send(Msg::WindowDragMove(600.0));
    let moved = dragged.state().content.windows.current().unwrap().borders()[0].at;
    assert!((moved - (border.at + 0.1)).abs() < 1e-9, "{moved}");
    let dropped = dragged.send(Msg::WindowDragEnd);
    assert_eq!(dropped.state().content.drag, None);
}

#[test]
fn the_details_describe_the_focused_window_s_worktree_and_nothing_while_nothing_is_open() {
    let run = Scenario::new().send(Msg::Show {
        thing: Tab::Session("s".into()),
        place: place("/w"),
        how: None,
    });
    assert_eq!(run.state().content.described(), place("/w").as_ref());
    let closed = run.send(Msg::Window(Action::TabClose));
    assert_eq!(closed.state().content.described(), None);
}

#[test]
fn the_host_reads_the_worktree_in_the_middle_while_it_is_there() {
    let watched = |run: &Scenario| {
        run.effects()
            .iter()
            .rev()
            .find_map(|e| match e {
                Effect::Host(Request::Watch(sight)) => Some(sight.place.clone()),
                _ => None,
            })
            .flatten()
    };
    let run = Scenario::new().event(listing([session("a")]));
    assert_eq!(watched(&run), None);
    let run = run.send(show(session_tab("a")));
    assert_eq!(
        watched(&run),
        Some(adroit_protocol::HostPath::local("/r/a"))
    );
}

#[test]
fn the_host_is_told_the_terminals_any_window_shows_so_it_can_quit_those_none_shows() {
    let watched = |run: &Scenario| {
        run.effects()
            .iter()
            .rev()
            .find_map(|e| match e {
                Effect::Host(Request::Watch(sight)) => Some(sight.terminals.clone()),
                _ => None,
            })
            .unwrap_or_default()
    };
    let terminal = |id| Msg::Show {
        thing: Tab::Terminal(id),
        place: place("/r"),
        how: Some(Opening::Tab),
    };
    let run = Scenario::new()
        .send(terminal(1))
        .send(terminal(2))
        .send(show(session_tab("a")));
    assert_eq!(watched(&run), BTreeSet::from([1, 2]));
    let closed = run
        .send(Msg::Window(Action::TabPrev))
        .send(Msg::Window(Action::TabClose));
    assert_eq!(watched(&closed), BTreeSet::from([1]));
}

#[test]
fn a_session_that_starts_waiting_is_not_told_of_when_the_settings_say_so() {
    let waits = session("a").cwd("/r").waiting(Wait::Permission);
    let run = Scenario::new()
        .event(crate::config::testing::configured(
            "notify_waiting = false\n",
        ))
        .send(opened(None))
        .event(listing([session("a").cwd("/r")]))
        .event(patch(2).listed(listed().session(waits)));
    assert!(
        !run.effects()
            .iter()
            .any(|e| matches!(e, Effect::NotifyWaiting(_)))
    );
}

#[test]
fn a_prompt_goes_to_the_session_s_agent_or_that_of_the_profile_chosen_for_a_draft() {
    use adroit_model::Agent;
    let codex = session("c").cwd("/r").agent(Agent::Codex);
    let draft = |dir: &str| Tab::Draft {
        worktree: dir.into(),
        id: Default::default(),
    };
    let run = Scenario::new()
        .event(crate::config::testing::configured(
            "profile = \"review\"\n[[profiles]]\nid = \"review\"\nagent = \"codex\"\n[[profiles]]\nid = \"work\"\nagent = \"claude\"\n",
        ))
        .send(opened(None))
        .event(listing([codex]));
    let state = run.state();
    assert_eq!(
        agent_of(state, Some(&Tab::Session("c".into()))),
        Agent::Codex
    );
    assert_eq!(agent_of(state, Some(&draft("/r"))), Agent::Codex);
    assert_eq!(agent_of(state, None), Agent::Codex);
    let run = run.send(Msg::DraftProfile {
        tab: Some(draft("/r")),
        profile: "work".parse().unwrap(),
    });
    let state = run.state();
    assert_eq!(agent_of(state, Some(&draft("/r"))), Agent::Claude);
    assert_eq!(agent_of(state, Some(&draft("/other"))), Agent::Claude);
    assert_eq!(agent_of(state, None), Agent::Claude);
    assert_eq!(
        agent_of(state, Some(&Tab::Session("c".into()))),
        Agent::Codex
    );
}
