use super::*;
use crate::content::Dir;

type C = Content<&'static str>;

/// The shown tab's layout, written as `a` for a window, `(a | b)` side by
/// side and `(a / b)` one above the other.
fn drawn(c: &C) -> String {
    fn of(n: &Node<&'static str>) -> String {
        match n {
            Node::Window(w) => w.shows.to_string(),
            Node::Split(axis, parts) => {
                let sep = if *axis == Axis::Row { " | " } else { " / " };
                let inner: Vec<String> = parts.iter().map(|(_, n)| of(n)).collect();
                format!("({})", inner.join(sep))
            }
        }
    }
    c.current().map(|t| of(t.root())).unwrap_or_default()
}

fn sizes(c: &C) -> Vec<f64> {
    match c.current().map(|t| t.root()) {
        Some(Node::Split(_, parts)) => parts
            .iter()
            .map(|(s, _)| (s * 100.0).round() / 100.0)
            .collect(),
        _ => vec![1.0],
    }
}

fn one(thing: &'static str) -> C {
    let mut c = C::default();
    c.open(thing);
    c
}

#[test]
fn a_thing_is_found_by_the_numbers_of_the_tabs_showing_it_counted_from_one() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Row, "a");
    c.open_tab("b");
    c.open_tab("c");
    assert_eq!(c.tabs_showing(&"a"), vec![1]);
    assert_eq!(c.tabs_showing(&"b"), vec![1, 2]);
    assert_eq!(c.tabs_showing(&"z"), Vec::<usize>::new());
}

#[test]
fn opening_with_nothing_open_makes_a_tab_with_one_window() {
    let c = one("a");
    assert_eq!(c.tabs().len(), 1);
    assert_eq!(c.focused(), Some(&"a"));
}

#[test]
fn opening_what_no_window_shows_adds_a_tab_after_the_shown_one() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.open_tab("z");
    c.go_to_tab(0);
    c.open("c");
    assert_eq!((c.tabs().len(), c.current_index()), (3, 1));
    assert_eq!(drawn(&c), "c");
    c.go_to_tab(0);
    assert_eq!(drawn(&c), "(a | b)");
}

#[test]
fn opening_behind_adds_a_tab_after_the_shown_one_and_keeps_it_shown() {
    let mut c = one("a");
    c.open_tab("b");
    c.go_to_tab(0);
    c.open_behind("z");
    assert_eq!((c.tabs().len(), c.current_index()), (3, 0));
    assert_eq!(c.focused(), Some(&"a"));
    c.go_to_tab(1);
    assert_eq!(drawn(&c), "z");
}

#[test]
fn opening_behind_with_nothing_open_shows_it() {
    let mut c = Content::<&str>::default();
    c.open_behind("z");
    assert_eq!(c.focused(), Some(&"z"));
}

#[test]
fn opening_what_the_tab_already_shows_focuses_the_window_showing_it() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.open("a");
    assert_eq!(drawn(&c), "(a | b)");
    assert_eq!(c.focused(), Some(&"a"));
}

#[test]
fn opening_what_only_another_tab_shows_goes_to_that_tab() {
    let mut c = one("a");
    c.open_tab("b");
    c.open("a");
    assert_eq!(c.tabs().len(), 2);
    assert_eq!(c.current_index(), 0);
    assert_eq!(drawn(&c), "a");
}

#[test]
fn tab_nine_is_the_last_however_many_there_are_as_in_browsers() {
    let mut c = one("a");
    for thing in ["b", "c", "d", "e", "f", "g", "h", "i", "j", "k"] {
        c.open(thing);
    }
    c.go_to_numbered_tab(8);
    assert_eq!(c.current_index(), 7);
    c.go_to_numbered_tab(9);
    assert_eq!(c.current_index(), 10);
    let mut few = one("a");
    few.open("b");
    few.go_to_numbered_tab(8);
    assert_eq!(few.current_index(), 1);
}

#[test]
fn a_window_takes_another_thing_in_place_and_the_others_keep_theirs() {
    let mut c = one("a");
    c.split(Axis::Row, "a");
    let id = c.focused_window().unwrap();
    assert!(c.replace_window(id, "b"));
    assert_eq!(drawn(&c), "(a | b)");
    assert_eq!(c.focused_window(), Some(id));
    c.close_window();
    assert!(!c.replace_window(id, "c"));
    assert_eq!(drawn(&c), "a");
}

#[test]
fn opened_in_a_window_a_thing_takes_its_place_or_else_opens_as_it_would() {
    let mut c = one("a");
    c.split(Axis::Row, "x");
    let id = c.focused_window().unwrap();
    c.open_in("b", Opening::InWindow(id));
    assert_eq!(drawn(&c), "(a | b)");
    c.close_window();
    c.open_in("y", Opening::InWindow(id));
    assert_eq!(c.tabs().len(), 2);
    assert_eq!(c.focused(), Some(&"y"));
}

#[test]
fn a_split_shows_its_thing_after_the_focused_window_and_takes_the_focus() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    assert_eq!(drawn(&c), "(a | b)");
    assert_eq!(c.focused(), Some(&"b"));
    c.split(Axis::Column, "c");
    assert_eq!(drawn(&c), "(a | (b / c))");
}

#[test]
fn a_split_may_show_what_the_window_it_splits_shows() {
    let mut c = one("a");
    c.split(Axis::Column, "a");
    assert_eq!(drawn(&c), "(a / a)");
}

#[test]
fn a_split_along_the_split_it_is_in_shares_the_window_s_room() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Row, "c");
    assert_eq!(drawn(&c), "(a | b | c)");
    assert_eq!(sizes(&c), vec![0.5, 0.25, 0.25]);
}

#[test]
fn closing_a_window_gives_its_room_to_the_one_before_it_and_focuses_the_one_focused_before() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Row, "c");
    c.focus(c.current().unwrap().windows()[0].id);
    c.focus(c.current().unwrap().windows()[1].id);
    c.close_window();
    assert_eq!(drawn(&c), "(a | c)");
    assert_eq!(sizes(&c), vec![0.75, 0.25]);
    assert_eq!(c.focused(), Some(&"a"));
}

#[test]
fn closing_the_first_window_gives_its_room_to_the_next() {
    let mut c = one("a");
    c.split(Axis::Column, "b");
    c.focus(c.current().unwrap().windows()[0].id);
    c.close_window();
    assert_eq!(drawn(&c), "b");
    assert_eq!(c.focused(), Some(&"b"));
}

#[test]
fn closing_the_last_window_of_a_split_leaves_no_split_of_one() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Column, "c");
    c.close_window();
    assert_eq!(drawn(&c), "(a | b)");
    c.split(Axis::Column, "c");
    c.focus(c.current().unwrap().windows()[0].id);
    c.close_window();
    assert_eq!(drawn(&c), "(b / c)");
}

#[test]
fn closing_a_tab_s_last_window_closes_the_tab_and_shows_the_one_after_it() {
    let mut c = one("a");
    c.open_tab("b");
    c.open_tab("c");
    c.go_to_tab(1);
    c.close_window();
    assert_eq!(c.tabs().len(), 2);
    assert_eq!(c.focused(), Some(&"c"));
    c.close_tab();
    assert_eq!(c.focused(), Some(&"a"));
    c.close_window();
    assert!(c.is_empty());
    assert_eq!(c.focused(), None);
}

#[test]
fn only_leaves_the_focused_window_alone_in_its_tab() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Column, "c");
    c.only();
    assert_eq!(drawn(&c), "c");
}

#[test]
fn a_window_moved_to_an_edge_spans_the_tab_along_it() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Column, "c");
    c.move_to_edge(Dir::Up);
    assert_eq!(drawn(&c), "(c / (a | b))");
    assert_eq!(c.focused(), Some(&"c"));
    c.move_to_edge(Dir::Right);
    assert_eq!(drawn(&c), "(a | b | c)");
    assert_eq!(sizes(&c), vec![0.33, 0.33, 0.33]);
    c.move_to_edge(Dir::Left);
    assert_eq!(drawn(&c), "(c | a | b)");
    c.move_to_edge(Dir::Down);
    assert_eq!(drawn(&c), "((a | b) / c)");
}

#[test]
fn the_only_window_moved_to_an_edge_stays() {
    let mut c = one("a");
    c.move_to_edge(Dir::Left);
    assert_eq!(drawn(&c), "a");
}

#[test]
fn exchanging_swaps_with_the_next_window_or_the_one_before_the_last_and_keeps_the_focus_in_place() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Row, "c");
    c.focus(c.current().unwrap().windows()[0].id);
    c.exchange();
    assert_eq!(drawn(&c), "(b | a | c)");
    assert_eq!(c.focused(), Some(&"b"));
    c.focus(c.current().unwrap().windows()[2].id);
    c.exchange();
    assert_eq!(drawn(&c), "(b | c | a)");
    assert_eq!(c.focused(), Some(&"a"));
}

#[test]
fn a_window_beside_a_split_is_not_exchanged() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Column, "c");
    c.focus(c.current().unwrap().windows()[0].id);
    c.exchange();
    assert_eq!(drawn(&c), "(a | (b / c))");
}

#[test]
fn rotating_moves_every_window_of_the_split_forward_and_the_focus_with_its_window() {
    let mut c = one("a");
    c.split(Axis::Column, "b");
    c.split(Axis::Column, "c");
    c.rotate();
    assert_eq!(drawn(&c), "(c / a / b)");
    assert_eq!(c.focused(), Some(&"c"));
    assert_eq!(sizes(&c), vec![0.5, 0.25, 0.25]);
}

#[test]
fn a_split_holding_a_split_is_not_rotated() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Column, "c");
    c.focus(c.current().unwrap().windows()[0].id);
    c.rotate();
    assert_eq!(drawn(&c), "(a | (b / c))");
}

#[test]
fn equalizing_gives_every_part_the_same_share() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Row, "c");
    c.equalize();
    assert_eq!(sizes(&c), vec![0.33, 0.33, 0.33]);
}

#[test]
fn resizing_takes_from_the_next_part_or_the_one_before_the_last() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Row, "c");
    c.equalize();
    c.resize(Axis::Row, 0.1);
    assert_eq!(sizes(&c), vec![0.33, 0.23, 0.43]);
    c.focus(c.current().unwrap().windows()[0].id);
    c.resize(Axis::Row, -0.1);
    assert_eq!(sizes(&c), vec![0.23, 0.33, 0.43]);
}

#[test]
fn resizing_leaves_every_part_at_least_its_least_share() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.resize(Axis::Row, 1.0);
    assert_eq!(sizes(&c), vec![0.05, 0.95]);
    c.resize(Axis::Row, -1.0);
    assert_eq!(sizes(&c), vec![0.95, 0.05]);
}

#[test]
fn resizing_along_an_axis_the_window_is_not_split_along_does_nothing() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.resize(Axis::Column, 0.1);
    assert_eq!(sizes(&c), vec![0.5, 0.5]);
}

#[test]
fn resizing_a_window_in_a_nested_split_resizes_the_nearest_split_along_the_axis() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Column, "c");
    c.resize(Axis::Row, 0.1);
    assert_eq!(sizes(&c), vec![0.4, 0.6]);
}

#[test]
fn maximizing_leaves_the_others_their_least_share() {
    let mut c = one("a");
    c.split(Axis::Column, "b");
    c.split(Axis::Column, "c");
    c.maximize(Axis::Column);
    assert_eq!(sizes(&c), vec![0.05, 0.05, 0.9]);
    c.maximize(Axis::Row);
    assert_eq!(sizes(&c), vec![0.05, 0.05, 0.9]);
}

#[test]
fn every_border_between_parts_is_found_where_it_is_drawn() {
    // (a | (b / c)) at equal shares
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Column, "c");
    let borders = c.current().unwrap().borders();
    assert_eq!(borders.len(), 2);
    let between_a_and_the_rest = &borders[0];
    assert_eq!(between_a_and_the_rest.split, Vec::<usize>::new());
    assert_eq!(between_a_and_the_rest.after, 0);
    assert_eq!(between_a_and_the_rest.axis, Axis::Row);
    assert_eq!(between_a_and_the_rest.at, 0.5);
    assert_eq!(between_a_and_the_rest.within, Rect::WHOLE);
    let between_b_and_c = &borders[1];
    assert_eq!(between_b_and_c.split, vec![1]);
    assert_eq!(between_b_and_c.axis, Axis::Column);
    assert_eq!(between_b_and_c.at, 0.5);
    assert_eq!(
        between_b_and_c.within,
        Rect {
            left: 0.5,
            top: 0.0,
            width: 0.5,
            height: 1.0
        }
    );
}

#[test]
fn dragging_a_border_moves_it_between_the_parts_beside_it() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Row, "c");
    c.equalize();
    c.drag(&[], 0, 0.5);
    assert_eq!(sizes(&c), vec![0.5, 0.17, 0.33]);
    c.drag(&[], 1, 0.0);
    assert_eq!(sizes(&c), vec![0.5, 0.05, 0.45]);
}

#[test]
fn a_window_moved_to_a_new_tab_leaves_its_tab_and_is_focused_there() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.to_new_tab();
    assert_eq!(c.tabs().len(), 2);
    assert_eq!(c.current_index(), 1);
    assert_eq!(drawn(&c), "b");
    c.go_to_tab(0);
    assert_eq!(drawn(&c), "a");
}

#[test]
fn the_only_window_of_a_tab_is_not_moved_to_a_new_tab() {
    let mut c = one("a");
    c.to_new_tab();
    assert_eq!(c.tabs().len(), 1);
}

/// The id of the window showing `thing` in any tab.
fn id_of(c: &C, thing: &str) -> WindowId {
    c.tabs()
        .iter()
        .flat_map(|t| t.windows())
        .find(|w| w.shows == thing)
        .map(|w| w.id)
        .expect("a window shows it")
}

#[test]
fn a_tab_merged_into_a_numbered_tab_lies_right_of_every_window_there_as_it_was_split() {
    let mut c = one("a");
    c.open_tab("b");
    c.split(Axis::Column, "c");
    c.open_tab("d");
    c.split(Axis::Row, "e");
    c.focus(id_of(&c, "d"));
    c.merge_into_numbered_tab(2);
    assert_eq!(c.tabs().len(), 2);
    assert_eq!(c.current_index(), 1);
    assert_eq!(drawn(&c), "((b / c) | d | e)");
    assert_eq!(c.focused(), Some(&"d"));
    assert_eq!(sizes(&c), vec![0.33, 0.33, 0.33]);
}

#[test]
fn a_tab_is_not_merged_into_itself() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.open_tab("c");
    c.go_to_tab(0);
    c.merge_into_numbered_tab(1);
    assert_eq!(drawn(&c), "(a | b)");
    assert_eq!(c.tabs().len(), 2);
}

#[test]
fn a_number_past_the_last_tab_merges_into_the_last() {
    for n in [8, 9] {
        let mut c = one("a");
        c.open_tab("b");
        c.open_tab("c");
        c.go_to_tab(0);
        c.merge_into_numbered_tab(n);
        assert_eq!(drawn(&c), "(c | a)", "{n}");
    }
}

#[test]
fn a_tab_merged_into_the_last_tab_goes_into_the_tab_shown_before() {
    let mut c = one("a");
    c.open_tab("b");
    c.split(Axis::Row, "c");
    c.open_tab("d");
    c.go_to_tab(0);
    c.go_to_tab(1);
    c.merge_into_last_tab();
    assert_eq!(drawn(&c), "(a | b | c)");
    assert_eq!(c.tabs().len(), 2);
}

#[test]
fn with_no_tab_shown_before_a_tab_merges_into_the_tab_before_it_or_after_the_first() {
    let came_back = |current| {
        let tab = |name| KeptTab {
            root: KeptNode::Window(name),
            focus: 0,
        };
        let tabs = vec![tab("a"), tab("b")];
        C::from_kept(Kept { tabs, current }, Some)
    };
    let mut c = came_back(1);
    c.merge_into_last_tab();
    assert_eq!(drawn(&c), "(a | b)");
    let mut c = came_back(0);
    c.merge_into_last_tab();
    assert_eq!(drawn(&c), "(b | a)");
}

#[test]
fn the_only_tab_merges_nowhere() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.merge_into_last_tab();
    assert_eq!(drawn(&c), "(a | b)");
}

#[test]
fn a_tab_placed_on_a_side_of_a_window_puts_every_window_of_it_there() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.open_tab("c");
    c.split(Axis::Row, "d");
    let a = id_of(&c, "a");
    c.place_tab(1, a, Dir::Down);
    assert_eq!(c.tabs().len(), 1);
    assert_eq!(drawn(&c), "((a / (c | d)) | b)");
    assert_eq!(c.focused(), Some(&"d"));
}

#[test]
fn a_tab_not_shown_placed_on_one_of_its_own_windows_stays() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.open_tab("c");
    let a = id_of(&c, "a");
    c.place_tab(0, a, Dir::Left);
    assert_eq!(c.tabs().len(), 2);
}

#[test]
fn the_tab_shown_placed_on_its_own_window_merges_into_the_tab_before_on_that_side() {
    let mut c = one("a");
    c.split(Axis::Column, "b");
    c.open_tab("c");
    c.split(Axis::Row, "d");
    c.open_tab("e");
    c.go_to_tab(0);
    c.go_to_tab(1);
    let c_id = id_of(&c, "c");
    c.place_tab(1, c_id, Dir::Up);
    assert_eq!(c.tabs().len(), 2);
    assert_eq!(drawn(&c), "((c | d) / a / b)");
    assert_eq!(c.focused(), Some(&"d"));
    let mut c = one("a");
    c.open_tab("b");
    let b = id_of(&c, "b");
    c.place_tab(1, b, Dir::Left);
    assert_eq!(drawn(&c), "(b | a)");
}

#[test]
fn the_only_tab_placed_on_its_own_window_stays() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    let a = id_of(&c, "a");
    c.place_tab(0, a, Dir::Left);
    assert_eq!(drawn(&c), "(a | b)");
}

#[test]
fn a_window_placed_on_a_side_of_another_goes_there_in_that_tab() {
    let mut c = one("a");
    c.open_tab("b");
    c.split(Axis::Row, "c");
    let (b, a) = (id_of(&c, "b"), id_of(&c, "a"));
    c.place(b, a, Dir::Up);
    assert_eq!(drawn(&c), "(b / a)");
    assert_eq!(c.focused(), Some(&"b"));
    let c_id = id_of(&c, "c");
    c.place(c_id, a, Dir::Left);
    assert_eq!(drawn(&c), "(b / (c | a))");
    assert_eq!(c.tabs().len(), 1);
}

#[test]
fn a_window_placed_on_a_side_of_another_in_its_own_tab_moves_there() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Row, "c");
    let (a, c_id) = (id_of(&c, "a"), id_of(&c, "c"));
    c.place(a, c_id, Dir::Down);
    assert_eq!(drawn(&c), "(b | (c / a))");
}

#[test]
fn a_window_placed_on_itself_stays() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    let a = id_of(&c, "a");
    c.place(a, a, Dir::Left);
    assert_eq!(drawn(&c), "(a | b)");
}

#[test]
fn tabs_go_round_and_a_number_past_the_last_goes_to_the_last() {
    let mut c = one("a");
    c.open_tab("b");
    c.open_tab("c");
    c.cycle_tab(1);
    assert_eq!(c.focused(), Some(&"a"));
    c.cycle_tab(-1);
    assert_eq!(c.focused(), Some(&"c"));
    c.go_to_tab(1);
    assert_eq!(c.focused(), Some(&"b"));
    c.go_to_tab(8);
    assert_eq!(c.focused(), Some(&"c"));
}

fn order(c: &C) -> Vec<&'static str> {
    c.tabs().iter().map(|t| t.focused().shows).collect()
}

#[test]
fn a_tab_moved_takes_the_place_given_and_stays_shown() {
    let mut c = one("a");
    c.open_tab("b");
    c.open_tab("c");
    c.go_to_tab(0);
    c.move_tab(0, 2);
    assert_eq!(order(&c), ["b", "c", "a"]);
    assert_eq!(c.focused(), Some(&"a"));
    c.move_tab(1, 0);
    assert_eq!(order(&c), ["c", "b", "a"]);
    assert_eq!(c.focused(), Some(&"a"));
}

#[test]
fn the_shown_tab_steps_along_the_tabs_and_stops_at_the_ends() {
    let mut c = one("a");
    c.open_tab("b");
    c.open_tab("c");
    c.go_to_tab(1);
    c.shift_tab(1);
    assert_eq!(order(&c), ["a", "c", "b"]);
    c.shift_tab(1);
    assert_eq!(order(&c), ["a", "c", "b"]);
    c.shift_tab(-1);
    c.shift_tab(-1);
    c.shift_tab(-1);
    assert_eq!(order(&c), ["b", "a", "c"]);
    assert_eq!(c.focused(), Some(&"b"));
}

#[test]
fn the_tab_shown_before_is_still_the_same_tab_after_tabs_move() {
    let mut c = one("a");
    c.open_tab("b");
    c.open_tab("c");
    c.go_to_tab(0);
    c.move_tab(2, 0);
    c.go_to_last_tab();
    assert_eq!(c.focused(), Some(&"c"));
}

#[test]
fn the_last_tab_is_the_one_shown_before() {
    let mut c = one("a");
    c.open_tab("b");
    c.open_tab("c");
    c.go_to_tab(0);
    c.go_to_last_tab();
    assert_eq!(c.focused(), Some(&"c"));
    c.go_to_last_tab();
    assert_eq!(c.focused(), Some(&"a"));
}

#[test]
fn a_closed_tab_is_no_last_tab() {
    let mut c = one("a");
    c.open_tab("b");
    c.go_to_tab(0);
    c.go_to_tab(1);
    c.go_to_tab(0);
    c.cycle_tab(1);
    c.close_tab();
    c.go_to_last_tab();
    assert_eq!(c.focused(), Some(&"a"));
}

#[test]
fn every_tab_keeps_its_own_focused_window() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.focus(c.current().unwrap().windows()[0].id);
    c.open_tab("c");
    c.cycle_tab(1);
    assert_eq!(c.focused(), Some(&"a"));
}

#[test]
fn a_neighbour_is_the_window_touching_the_focused_one_on_that_side() {
    // (a | (b / c)), focus on c
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Column, "c");
    let id = |c: &C, at: usize| c.current().unwrap().windows()[at].id;
    assert_eq!(c.neighbour(Dir::Up), Some(id(&c, 1)));
    assert_eq!(c.neighbour(Dir::Left), Some(id(&c, 0)));
    assert_eq!(c.neighbour(Dir::Right), None);
    assert_eq!(c.neighbour(Dir::Down), None);
}

#[test]
fn of_several_neighbours_the_one_focused_before_wins() {
    // (a | (b / c)) with b taller than c
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Column, "c");
    c.resize(Axis::Column, -0.2);
    let id = |c: &C, at: usize| c.current().unwrap().windows()[at].id;
    c.focus(id(&c, 0));
    assert_eq!(c.neighbour(Dir::Right), Some(id(&c, 2)));
    c.focus(id(&c, 1));
    c.focus(id(&c, 0));
    assert_eq!(c.neighbour(Dir::Right), Some(id(&c, 1)));
}

#[test]
fn of_several_neighbours_none_focused_before_the_one_sharing_most_wins() {
    let mut c = one("a");
    c.split(Axis::Column, "b");
    c.resize(Axis::Column, -0.2);
    c.split(Axis::Row, "c");
    c.split(Axis::Row, "d");
    let id = |c: &C, at: usize| c.current().unwrap().windows()[at].id;
    c.focus(id(&c, 2));
    c.close_window();
    c.move_to_edge(Dir::Right);
    assert_eq!(drawn(&c), "((a / b) | d)");
    assert_eq!(c.neighbour(Dir::Left), Some(id(&c, 0)));
}

#[test]
fn coming_in_from_a_side_lands_on_the_focused_window_if_it_lies_along_it() {
    // (a | (b / c)), focus on c
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Column, "c");
    let id = |c: &C, at: usize| c.current().unwrap().windows()[at].id;
    assert_eq!(c.entered_from(Dir::Left), Some(id(&c, 2)));
    assert_eq!(c.entered_from(Dir::Right), Some(id(&c, 0)));
    c.focus(id(&c, 1));
    c.focus(id(&c, 0));
    assert_eq!(c.entered_from(Dir::Left), Some(id(&c, 1)));
}

#[test]
fn replacing_a_thing_shows_the_new_one_in_every_window_and_tab() {
    let mut c = one("a");
    c.split(Axis::Row, "a");
    c.open_tab("a");
    c.replace(&"a", "z");
    assert_eq!(c.shown(), vec![&"z"]);
}

#[test]
fn revealing_goes_to_a_window_of_the_shown_tab_or_else_of_the_first_tab_showing_it() {
    let mut c = one("a");
    c.open_tab("b");
    c.split(Axis::Row, "c");
    c.open_tab("a");
    assert!(c.reveal(&"c"));
    assert_eq!((c.current_index(), c.focused()), (1, Some(&"c")));
    assert!(c.reveal(&"a"));
    assert_eq!(c.current_index(), 0);
    assert!(!c.reveal(&"z"));
    assert_eq!(c.focused(), Some(&"a"));
}

#[test]
fn a_thing_gone_closes_every_window_showing_it_and_every_tab_left_empty() {
    let mut c = one("a");
    c.split(Axis::Row, "gone");
    c.open_tab("gone");
    c.open_tab("b");
    c.go_to_tab(1);
    c.close_showing(|t| *t == "gone");
    assert_eq!(c.tabs().len(), 2);
    assert_eq!(c.focused(), Some(&"b"));
    c.go_to_tab(0);
    assert_eq!(drawn(&c), "a");
}

#[test]
fn a_thing_gone_from_the_last_tab_shown_shows_the_tab_before() {
    let mut c = one("a");
    c.open_tab("gone");
    c.close_showing(|t| *t == "gone");
    assert_eq!(c.focused(), Some(&"a"));
    c.close_showing(|_| true);
    assert!(c.is_empty());
}

#[test]
fn opening_where_it_is_goes_to_a_window_of_any_tab_showing_it() {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.open_tab("c");
    c.open_in("b", Opening::Existing);
    assert_eq!((c.current_index(), c.focused()), (0, Some(&"b")));
    assert_eq!(c.tabs().len(), 2);
}

#[test]
fn opening_in_a_split_puts_a_new_window_below_or_beside_the_focused_one() {
    let mut c = one("a");
    c.open_in("b", Opening::Split(Axis::Column));
    assert_eq!(drawn(&c), "(a / b)");
    c.open_in("c", Opening::Split(Axis::Row));
    assert_eq!(drawn(&c), "(a / (b | c))");
    assert_eq!(c.focused(), Some(&"c"));
}

#[test]
fn opening_in_a_tab_adds_one_after_the_shown_tab() {
    let mut c = one("a");
    c.open_tab("b");
    c.go_to_tab(0);
    c.open_in("a", Opening::Tab);
    assert_eq!((c.tabs().len(), c.current_index()), (3, 1));
    assert_eq!(c.focused(), Some(&"a"));
}

#[test]
fn opening_anyhow_with_nothing_open_opens_a_tab() {
    for how in [Opening::Existing, Opening::Split(Axis::Row), Opening::Tab] {
        let mut c = C::default();
        c.open_in("a", how);
        assert_eq!(drawn(&c), "a", "{how:?}");
    }
}

#[test]
fn what_is_shown_is_listed_once_however_many_windows_show_it() {
    let mut c = one("a");
    c.split(Axis::Row, "a");
    c.open_tab("b");
    assert_eq!(c.shown(), vec![&"a", &"b"]);
    assert!(c.shows_anywhere(&"b"));
    assert!(!c.shows_anywhere(&"c"));
}

fn all(thing: &&'static str) -> Option<&'static str> {
    Some(*thing)
}

fn back(kept: Kept<&'static str>, gone: &[&str]) -> C {
    C::from_kept(kept, |t| (!gone.contains(&t)).then_some(t))
}

/// `a | (b / c)` with the border dragged, `c` focused, and a second tab
/// showing `d`, the first tab shown.
fn laid_out() -> C {
    let mut c = one("a");
    c.split(Axis::Row, "b");
    c.split(Axis::Column, "c");
    c.drag(&[], 0, 0.3);
    c.open_tab("d");
    c.go_to_tab(0);
    c
}

#[test]
fn the_content_kept_comes_back_with_its_tabs_splits_sizes_and_focus() {
    let c = laid_out();
    let restored = back(c.kept(all), &[]);
    assert_eq!(drawn(&restored), "(a | (b / c))");
    assert_eq!(sizes(&restored), [0.3, 0.7]);
    assert_eq!(restored.focused(), Some(&"c"));
    assert_eq!(restored.tabs().len(), 2);
    assert_eq!(restored.current_index(), 0);
    assert_eq!(restored.kept(all), c.kept(all));
}

#[test]
fn a_window_whose_thing_cannot_come_back_is_closed_as_if_it_had_ended() {
    let c = laid_out();
    let restored = back(c.kept(all), &["c", "d"]);
    let mut ended = c.clone();
    ended.close_showing(|t| ["c", "d"].contains(t));
    assert_eq!(drawn(&restored), "(a | b)");
    assert_eq!(restored.focused(), ended.focused());
    assert_eq!(restored.kept(all), ended.kept(all));
}

#[test]
fn nothing_that_can_come_back_leaves_the_content_empty() {
    let restored = back(laid_out().kept(all), &["a", "b", "c", "d"]);
    assert!(restored.is_empty());
    assert!(back(Kept::default(), &[]).is_empty());
}

#[test]
fn a_window_that_keeps_nothing_is_left_out_of_what_is_kept_as_if_it_had_ended() {
    let c = laid_out();
    let kept = c.kept(|t| (*t != "b").then_some(*t));
    let mut ended = c.clone();
    ended.close_showing(|t| *t == "b");
    assert_eq!(kept, ended.kept(all));
}

#[test]
fn a_kept_content_out_of_shape_comes_back_in_shape() {
    let kept = Kept {
        tabs: vec![
            KeptTab {
                root: KeptNode::Split {
                    axis: Axis::Row,
                    parts: vec![
                        (f64::NAN, KeptNode::Window("a")),
                        (
                            0.5,
                            KeptNode::Split {
                                axis: Axis::Row,
                                parts: vec![(1.0, KeptNode::Window("b"))],
                            },
                        ),
                        (
                            0.5,
                            KeptNode::Split {
                                axis: Axis::Column,
                                parts: Vec::new(),
                            },
                        ),
                    ],
                },
                focus: 9,
            },
            KeptTab {
                root: KeptNode::Window("c"),
                focus: 0,
            },
        ],
        current: 5,
    };
    let restored = back(kept, &[]);
    assert_eq!(restored.tabs().len(), 2);
    assert_eq!(restored.current_index(), 1);
    assert_eq!(restored.focused(), Some(&"c"));
    let mut restored = restored;
    restored.go_to_tab(0);
    assert_eq!(drawn(&restored), "(a | b)");
    assert!((sizes(&restored).iter().sum::<f64>() - 1.0).abs() < 0.01);
    assert_eq!(restored.focused(), Some(&"b"));
}
