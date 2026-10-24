use std::collections::BTreeSet;
use std::path::PathBuf;

use adroit_model::Key;
use adroit_model::files::Entry;
use adroit_protocol::{Offers, PickChange, PickSight, Request, TreeChange};

use super::*;
use crate::content::{Area, Tab};
use crate::effect::Effect;
use crate::msg::Msg;
use crate::scenario::Scenario;
use crate::testing::{listing, session, session_tab};

const HOME: &str = "/Users/me";

fn dirs(dir: &str, children: &[&str]) -> TreeChange {
    TreeChange::Entries {
        dir: dir.into(),
        entries: children
            .iter()
            .map(|c| Entry {
                path: PathBuf::from(dir).join(c),
                dir: true,
            })
            .chain([Entry {
                path: PathBuf::from(dir).join("a-file"),
                dir: false,
            }])
            .collect(),
    }
}

fn offers(recent: &[&str], ghq: &[&str], roots: &[&str]) -> PickChange {
    let paths = |ps: &[&str]| ps.iter().map(PathBuf::from).collect();
    PickChange::Offers {
        offers: Offers {
            recent: paths(recent),
            ghq: paths(ghq),
            listed: Vec::new(),
            roots: paths(roots),
        },
    }
}

fn key(name: &str) -> Msg {
    Msg::PickerKey(Key::new(name))
}

fn ctrl(name: &str) -> Msg {
    Msg::PickerKey(Key::new(name).ctrl())
}

/// A window with a session `a` running in `/r/a`, the picker open on it.
fn picking() -> Scenario {
    Scenario::new()
        .send(Msg::Opened { home: HOME.into() })
        .event(listing([session("a")]))
        .send(Msg::PickerOpen)
}

fn picker(run: &Scenario) -> &Picker {
    match &run.state().picker.open {
        Some(Picking::Go(p)) => p,
        other => panic!("the picker is not open: {other:?}"),
    }
}

/// The rows the picker lists, as drawn.
fn names(run: &Scenario) -> Vec<String> {
    let view = &run.state().picker;
    lines(
        &go_rows(picker(run), &view.home, run.state().host.work.as_ref()),
        &view.home,
    )
    .into_iter()
    .map(|l| l.name)
    .collect()
}

fn asked(run: &Scenario) -> Option<Option<PickSight>> {
    run.effects()
        .iter()
        .filter_map(|e| match e {
            Effect::Host(Request::Watch(sight)) => Some(sight.pick.clone()),
            _ => None,
        })
        .next_back()
}

fn listed(dirs: &[&str]) -> Option<Option<PickSight>> {
    Some(Some(PickSight {
        listed: dirs.iter().map(PathBuf::from).collect::<BTreeSet<_>>(),
    }))
}

#[test]
fn opening_the_picker_asks_the_host_for_the_offers_and_the_home_directory() {
    let run = picking();
    assert_eq!(asked(&run), listed(&[HOME]));
    assert_eq!(
        names(&run),
        ["New scratch session", "a-name", "/r/a"],
        "the sessions and where they run are offered at once"
    );
}

#[test]
fn closing_the_picker_stops_the_host_listing_for_it() {
    for close in [key("Escape"), ctrl("c"), Msg::PickerClose] {
        let run = picking().send(close.clone());
        assert_eq!(asked(&run), Some(None), "{close:?}");
        assert_eq!(run.state().picker.open, None, "{close:?}");
    }
}

#[test]
fn the_offers_follow_the_running_directories_each_once() {
    let run = picking().tell(offers(&["/r/a", "/Users/me/src/x"], &["/g/y"], &["/g"]));
    assert_eq!(
        names(&run),
        ["New scratch session", "a-name", "/r/a", "~/src/x", "/g/y"]
    );
}

#[test]
fn the_cursor_stays_on_its_row_as_the_offers_come() {
    let run = picking().send(Msg::PickerQuery("a".into()));
    let last = names(&run).len() - 1;
    let run = run.send_all(std::iter::repeat_n(key("ArrowDown"), last));
    let on = names(&run)[picker(&run).cursor].clone();
    assert_eq!(on, "/r/a");
    let run = run.tell(offers(&["/a", "/Users/me/a"], &[], &[]));
    assert_eq!(names(&run).len(), last + 3);
    assert_eq!(names(&run)[picker(&run).cursor], on);
}

#[test]
fn the_list_s_keys_move_round_its_rows() {
    // (keys, cursor)
    let cases = [
        (vec![key("ArrowDown")], 1),
        (vec![key("Tab")], 1),
        (vec![ctrl("n")], 1),
        (vec![ctrl("j")], 1),
        (vec![key("ArrowUp")], 2),
        (vec![ctrl("p")], 2),
        (vec![ctrl("k")], 2),
        (vec![Msg::PickerKey(Key::new("Tab").shift())], 2),
        (
            vec![key("ArrowDown"), key("ArrowDown"), key("ArrowDown")],
            0,
        ),
    ];
    for (keys, want) in cases {
        let run = picking().send_all(keys.clone());
        assert_eq!(picker(&run).cursor, want, "{keys:?}");
    }
}

#[test]
fn typing_narrows_the_list_and_puts_the_cursor_back_on_the_first_row() {
    let run = picking()
        .send(key("ArrowDown"))
        .send(Msg::PickerQuery("r/a".into()));
    assert_eq!(names(&run), ["/r/a"]);
    assert_eq!(picker(&run).cursor, 0);
}

#[test]
fn choosing_a_directory_or_the_scratch_row_starts_a_draft_there_to_write_in() {
    let scratch = |run: Scenario| {
        run.send(Msg::ContentOpened {
            kept: Default::default(),
            scratch: "/scratch".into(),
        })
    };
    // (rows passed over, where the draft is)
    let cases = [(0, "/scratch"), (2, "/r/a")];
    for (down, want) in cases {
        let run = scratch(picking())
            .send_all(std::iter::repeat_n(key("ArrowDown"), down))
            .send(key("Enter"));
        assert_eq!(run.state().picker.open, None, "{want}");
        assert!(
            matches!(
                &run.state().content.showing().tab,
                Some(Tab::Draft { worktree, .. }) if worktree == &PathBuf::from(want)
            ),
            "{want}"
        );
        assert_eq!(run.state().content.focus, Area::Prompt, "{want}");
        assert!(run.last_effects().contains(&Effect::StartWriting), "{want}");
    }
}

#[test]
fn choosing_a_session_shows_it() {
    let run = picking().send(Msg::PickerChoose(1));
    assert_eq!(run.state().picker.open, None);
    assert_eq!(run.state().content.showing().tab, Some(session_tab("a")));
}

#[test]
fn a_row_past_the_list_chooses_nothing() {
    let run = picking().send(Msg::PickerChoose(9));
    assert!(run.state().picker.open.is_some());
    assert!(run.last_effects().is_empty());
}

#[test]
fn ctrl_f_turns_to_the_directory_tree_and_back() {
    let run = picking().send(ctrl("f"));
    assert_eq!(picker(&run).mode, Mode::Tree);
    let run = run.send(ctrl("f"));
    assert_eq!(picker(&run).mode, Mode::List);
}

/// The picker on its tree, the home directory listed.
fn in_tree() -> Scenario {
    let run = picking().send(ctrl("f"));
    run.tell(dirs(HOME, &["src", ".config", "docs"]))
}

fn cursor(run: &Scenario) -> Option<PathBuf> {
    picker(run).filer.cursor().map(|r| r.path.clone())
}

#[test]
fn the_tree_starts_inside_the_home_directory_once_it_is_listed() {
    let run = picking().send(ctrl("f"));
    assert_eq!(cursor(&run), Some(HOME.into()));
    let run = run.tell(dirs(HOME, &["src", "docs"]));
    assert_eq!(cursor(&run), Some("/Users/me/docs".into()));
    let rows: Vec<_> = tree_rows(picker(&run), run.state().host.work.as_ref())
        .into_iter()
        .map(|r| r.path)
        .collect();
    assert_eq!(
        rows,
        [HOME, "/Users/me/docs", "/Users/me/src"].map(PathBuf::from)
    );
}

#[test]
fn opening_a_directory_asks_for_it_and_moves_into_it_once_it_is_listed() {
    let run = in_tree().send(key("j")).send(key("l"));
    assert_eq!(cursor(&run), Some("/Users/me/src".into()));
    assert_eq!(asked(&run), listed(&[HOME, "/Users/me/src"]));
    let run = run.tell(dirs("/Users/me/src", &["adroit"]));
    assert_eq!(cursor(&run), Some("/Users/me/src/adroit".into()));
}

#[test]
fn a_directory_listed_after_the_cursor_left_it_does_not_take_the_cursor() {
    let run = in_tree().send(key("j")).send(key("l")).send(key("k"));
    let run = run.tell(dirs("/Users/me/src", &["adroit"]));
    assert_eq!(cursor(&run), Some("/Users/me/docs".into()));
}

#[test]
fn the_tree_s_keys_move_close_and_show_the_hidden() {
    // (keys, the cursor after)
    let cases = [
        (vec![key("k")], HOME),
        (vec![key("ArrowDown")], "/Users/me/src"),
        (vec![key("h")], HOME),
        (vec![key("ArrowLeft")], HOME),
        (vec![key("!"), key("k")], "/Users/me/.config"),
    ];
    for (keys, want) in cases {
        let run = in_tree().send_all(keys.clone());
        assert_eq!(cursor(&run), Some(want.into()), "{keys:?}");
    }
    let shown = in_tree().send(key("!"));
    let rows = tree_rows(picker(&shown), shown.state().host.work.as_ref());
    assert!(rows.iter().any(|r| r.path.ends_with(".config")));
}

#[test]
fn the_offers_add_their_roots_to_the_tree() {
    let run = in_tree().tell(offers(&[], &[], &["/g", "/l"]));
    assert_eq!(
        picker(&run).filer.roots(),
        [HOME, "/g", "/l"].map(PathBuf::from)
    );
    assert_eq!(cursor(&run), Some("/Users/me/docs".into()));
}

#[test]
fn enter_in_the_tree_starts_a_draft_in_the_directory_under_the_cursor() {
    let run = in_tree().send(key("Enter"));
    assert_eq!(run.state().picker.open, None);
    assert!(matches!(
        &run.state().content.showing().tab,
        Some(Tab::Draft { worktree, .. }) if worktree == &PathBuf::from("/Users/me/docs")
    ));
}

#[test]
fn a_directory_clicked_in_the_tree_starts_a_draft_there() {
    let run = in_tree().send(Msg::PickerDir("/Users/me/src".into()));
    assert_eq!(run.state().picker.open, None);
    assert!(run.last_effects().contains(&Effect::StartWriting));
}

#[test]
fn opening_one_picker_closes_the_other() {
    let run = picking().send(Msg::ResumeOpen);
    assert!(matches!(run.state().picker.open, Some(Picking::Resume(_))));
    assert_eq!(asked(&run), Some(None));
    let run = run.send(Msg::PickerOpen);
    assert!(matches!(run.state().picker.open, Some(Picking::Go(_))));
}
