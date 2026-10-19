//! The log drawn as a graph: which lane each change sits in, and the lines
//! that join it to the changes above and below it.

/// How many lane colours there are; a lane's colour is an index below it.
pub const COLORS: usize = 4;

/// How far apart lanes are, in pixels.
pub const LANE: f32 = 14.0;

/// How tall a row is, in pixels; every row is, so lines meet across rows.
pub const ROW: f32 = 40.0;

/// A line within half a row, from a lane at its top to a lane at its bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub color: usize,
}

/// One change of the log as drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The lane its node sits in.
    pub lane: usize,
    pub color: usize,
    /// The lines from the row's top down to the node's height.
    pub top: Vec<Edge>,
    /// The lines from the node's height down to the row's bottom.
    pub bottom: Vec<Edge>,
}

impl Row {
    /// How many lanes it spans.
    pub fn width(&self) -> usize {
        self.top
            .iter()
            .chain(&self.bottom)
            .flat_map(|e| [e.from, e.to])
            .chain([self.lane])
            .max()
            .map_or(0, |l| l + 1)
    }
}

/// Lay out changes given as their node and parents, each before its
/// parents. A parent not among them leaves its lane running off the bottom.
pub fn layout<'a>(changes: impl IntoIterator<Item = (&'a str, &'a [String])>) -> Vec<Row> {
    // What each lane expects next below the row laid out, and its colour.
    let mut lanes: Vec<Option<(&str, usize)>> = Vec::new();
    let mut colors = 0;
    let mut fresh = || {
        let c = colors % COLORS;
        colors += 1;
        c
    };
    let free = |lanes: &mut Vec<Option<(&str, usize)>>| match lanes.iter().position(Option::is_none)
    {
        Some(l) => l,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    };
    let mut rows = Vec::new();
    for (node, parents) in changes {
        let hits: Vec<usize> = (0..lanes.len())
            .filter(|&l| lanes[l].is_some_and(|(id, _)| id == node))
            .collect();
        let (lane, color) = match hits.first() {
            Some(&l) => (l, lanes[l].map_or(0, |(_, c)| c)),
            None => (free(&mut lanes), fresh()),
        };
        let mut top: Vec<Edge> = lanes
            .iter()
            .enumerate()
            .filter_map(|(l, e)| {
                let (_, c) = (*e)?;
                let to = if hits.contains(&l) { lane } else { l };
                Some(Edge {
                    from: l,
                    to,
                    color: c,
                })
            })
            .collect();
        for &l in &hits {
            lanes[l] = None;
        }
        let passing: Vec<(usize, usize)> = lanes
            .iter()
            .enumerate()
            .filter_map(|(l, e)| e.map(|(_, c)| (l, c)))
            .collect();
        let mut bottom = Vec::new();
        let mut rehomed = Vec::new();
        let mut seen = Vec::new();
        for (i, parent) in parents.iter().enumerate() {
            let parent = parent.as_str();
            if seen.contains(&parent) {
                continue;
            }
            seen.push(parent);
            let expected = lanes
                .iter()
                .position(|e| e.is_some_and(|(id, _)| id == parent));
            // A line joining a lane already there ends this change's own
            // line, so it keeps this change's colour, as does its first
            // parent's; only a lane opened for another parent has its own.
            let (to, c) = match expected {
                // The first parent keeps to the leftmost lane, so the line
                // the stack grows on stays left rather than drifting right.
                Some(j) if i == 0 && j > lane && lanes[lane].is_none() => {
                    lanes[j] = None;
                    lanes[lane] = Some((parent, color));
                    rehomed.push(j);
                    (lane, color)
                }
                Some(j) => (j, color),
                None if i == 0 && lanes[lane].is_none() => {
                    lanes[lane] = Some((parent, color));
                    (lane, color)
                }
                None => {
                    let (l, c) = (free(&mut lanes), fresh());
                    lanes[l] = Some((parent, c));
                    (l, c)
                }
            };
            bottom.push(Edge {
                from: lane,
                to,
                color: c,
            });
        }
        for (l, c) in passing {
            let to = if rehomed.contains(&l) { lane } else { l };
            bottom.push(Edge {
                from: l,
                to,
                color: c,
            });
        }
        top.sort_by_key(|e| (e.from, e.to));
        bottom.sort_by_key(|e| (e.from, e.to));
        rows.push(Row {
            lane,
            color,
            top,
            bottom,
        });
    }
    rows
}

/// The x of a lane's centre.
pub fn x(lane: usize) -> f32 {
    LANE / 2.0 + LANE * lane as f32
}

/// The SVG path of `edge` in the top half of a row, or the bottom: straight
/// down when it keeps its lane, an S-curve when it moves between lanes.
pub fn path(edge: &Edge, bottom: bool) -> String {
    let (y0, y1) = if bottom {
        (ROW / 2.0, ROW)
    } else {
        (0.0, ROW / 2.0)
    };
    let (x0, x1) = (x(edge.from), x(edge.to));
    if edge.from == edge.to {
        format!("M{x0} {y0}V{y1}")
    } else {
        let ym = (y0 + y1) / 2.0;
        format!("M{x0} {y0}C{x0} {ym} {x1} {ym} {x1} {y1}")
    }
}

/// How a change's node is marked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Marks {
    pub current: bool,
    pub empty: bool,
    pub conflict: bool,
}

/// A row drawn as SVG `lanes` lanes wide, its lines and node classed by
/// colour for the stylesheet to paint.
pub fn svg(row: &Row, lanes: usize, marks: Marks) -> String {
    let width = LANE * lanes.max(row.width()) as f32;
    let mut out = format!(
        r#"<svg class="graph" width="{width}" height="{ROW}" viewBox="0 0 {width} {ROW}">"#
    );
    let halves = row.top.iter().map(|e| (e, false));
    for (e, bottom) in halves.chain(row.bottom.iter().map(|e| (e, true))) {
        out += &format!(
            r#"<path class="lane-{}" d="{}"/>"#,
            e.color,
            path(e, bottom)
        );
    }
    let mut class = format!("node lane-{}", row.color);
    for (on, name) in [
        (marks.current, " current"),
        (marks.empty, " empty"),
        (marks.conflict, " conflict"),
    ] {
        if on {
            class += name;
        }
    }
    let r = if marks.current { 5 } else { 4 };
    out += &format!(
        r#"<circle class="{class}" cx="{}" cy="{}" r="{r}"/></svg>"#,
        x(row.lane),
        ROW / 2.0
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    type Lines = &'static [(usize, usize)];

    /// A row as its lane and the lines of its top and bottom halves.
    type Shape = (usize, Vec<(usize, usize)>, Vec<(usize, usize)>);

    /// Lay out changes written as `node parent parent…`.
    fn lay(changes: &[&str]) -> Vec<Row> {
        let owned: Vec<(String, Vec<String>)> = changes
            .iter()
            .map(|c| {
                let mut w = c.split_whitespace().map(str::to_string);
                (w.next().unwrap(), w.collect())
            })
            .collect();
        layout(owned.iter().map(|(n, p)| (n.as_str(), p.as_slice())))
    }

    fn shape(rows: &[Row]) -> Vec<Shape> {
        let pairs = |es: &[Edge]| es.iter().map(|e| (e.from, e.to)).collect();
        rows.iter()
            .map(|r| (r.lane, pairs(&r.top), pairs(&r.bottom)))
            .collect()
    }

    fn want(rows: &[(usize, Lines, Lines)]) -> Vec<Shape> {
        rows.iter()
            .map(|(l, t, b)| (*l, t.to_vec(), b.to_vec()))
            .collect()
    }

    #[test]
    fn a_chain_runs_down_one_lane_and_stops_at_a_root() {
        let rows = lay(&["c b", "b a", "a"]);
        assert_eq!(
            shape(&rows),
            want(&[
                (0, &[], &[(0, 0)]),
                (0, &[(0, 0)], &[(0, 0)]),
                (0, &[(0, 0)], &[])
            ])
        );
    }

    #[test]
    fn a_parent_off_the_list_leaves_its_lane_running_off_the_bottom() {
        let rows = lay(&["b a", "a trunk"]);
        assert_eq!(
            rows[1].bottom,
            [Edge {
                from: 0,
                to: 0,
                color: 0
            }]
        );
    }

    #[test]
    fn a_fork_opens_a_lane_that_joins_its_parent_s() {
        let rows = lay(&["b a", "c a", "a"]);
        assert_eq!(
            shape(&rows),
            want(&[
                (0, &[], &[(0, 0)]),
                (1, &[(0, 0)], &[(0, 0), (1, 0)]),
                (0, &[(0, 0)], &[]),
            ])
        );
    }

    #[test]
    fn a_merge_of_two_opens_a_lane_for_its_second_parent() {
        let rows = lay(&["m x y", "x b", "y b", "b"]);
        assert_eq!(
            shape(&rows),
            want(&[
                (0, &[], &[(0, 0), (0, 1)]),
                (0, &[(0, 0), (1, 1)], &[(0, 0), (1, 1)]),
                (1, &[(0, 0), (1, 1)], &[(0, 0), (1, 0)]),
                (0, &[(0, 0)], &[]),
            ])
        );
    }

    #[test]
    fn a_first_parent_expected_further_right_pulls_its_lane_left() {
        // `y` and `x` share a parent; `x` is the first parent of the merge.
        let rows = lay(&["m x y", "y b", "x b", "b"]);
        assert_eq!(
            shape(&rows),
            want(&[
                (0, &[], &[(0, 0), (0, 1)]),
                (1, &[(0, 0), (1, 1)], &[(0, 0), (1, 1)]),
                (0, &[(0, 0), (1, 1)], &[(0, 0), (1, 0)]),
                (0, &[(0, 0)], &[]),
            ])
        );
    }

    #[test]
    fn a_lane_freed_is_taken_again_in_a_colour_of_its_own() {
        let rows = lay(&["a b c", "c b", "b d e"]);
        assert_eq!(
            shape(&rows),
            want(&[
                (0, &[], &[(0, 0), (0, 1)]),
                (1, &[(0, 0), (1, 1)], &[(0, 0), (1, 0)]),
                (0, &[(0, 0)], &[(0, 0), (0, 1)]),
            ])
        );
        let reused = rows[2].bottom[1].color;
        assert_ne!(reused, rows[1].color);
        assert_ne!(reused, rows[2].color);
    }

    #[test]
    fn a_lane_keeps_its_colour_down_to_where_it_joins_another() {
        let rows = lay(&["m x y", "x b", "y b", "b"]);
        assert_eq!(rows[0].color, 0);
        assert_eq!(
            rows[0].bottom,
            [
                Edge {
                    from: 0,
                    to: 0,
                    color: 0
                },
                Edge {
                    from: 0,
                    to: 1,
                    color: 1
                },
            ]
        );
        assert_eq!(rows[2].color, 1);
        assert_eq!(
            rows[2].bottom[1],
            Edge {
                from: 1,
                to: 0,
                color: 1
            }
        );
        assert_eq!(rows[3].color, 0);
    }

    #[test]
    fn a_stack_of_three_parent_merges_stays_on_the_left_lane() {
        // Recorded from this repository's stack with `jj log --no-graph`, the
        // commit ids shortened; `9ba5` is trunk, off the list.
        let rows = lay(&[
            "1172 15da ae18",
            "15da 7d3d",
            "ae18 7d3d",
            "7d3d ce12 7ea2 c9d1",
            "c9d1 f10f",
            "7ea2 f10f",
            "ce12 f10f",
            "f10f a6e4",
            "a6e4 7e52 22ec 3967",
            "3967 b3a9",
            "7e52 b3a9",
            "22ec b3a9",
            "b3a9 c81f 0fea d7b7",
            "d7b7 9ba5",
            "0fea c397",
            "c397 c25e",
            "c25e 9ba5",
            "c81f 9ba5",
        ]);
        assert_eq!(
            shape(&rows),
            want(&[
                (0, &[], &[(0, 0), (0, 1)]),
                (0, &[(0, 0), (1, 1)], &[(0, 0), (1, 1)]),
                (1, &[(0, 0), (1, 1)], &[(0, 0), (1, 0)]),
                (0, &[(0, 0)], &[(0, 0), (0, 1), (0, 2)]),
                (2, &[(0, 0), (1, 1), (2, 2)], &[(0, 0), (1, 1), (2, 2)]),
                (1, &[(0, 0), (1, 1), (2, 2)], &[(0, 0), (1, 1), (2, 1)]),
                (0, &[(0, 0), (1, 1)], &[(0, 0), (1, 0)]),
                (0, &[(0, 0)], &[(0, 0)]),
                (0, &[(0, 0)], &[(0, 0), (0, 1), (0, 2)]),
                (2, &[(0, 0), (1, 1), (2, 2)], &[(0, 0), (1, 1), (2, 2)]),
                (0, &[(0, 0), (1, 1), (2, 2)], &[(0, 0), (1, 1), (2, 0)]),
                (1, &[(0, 0), (1, 1)], &[(0, 0), (1, 0)]),
                (0, &[(0, 0)], &[(0, 0), (0, 1), (0, 2)]),
                (2, &[(0, 0), (1, 1), (2, 2)], &[(0, 0), (1, 1), (2, 2)]),
                (1, &[(0, 0), (1, 1), (2, 2)], &[(0, 0), (1, 1), (2, 2)]),
                (1, &[(0, 0), (1, 1), (2, 2)], &[(0, 0), (1, 1), (2, 2)]),
                (1, &[(0, 0), (1, 1), (2, 2)], &[(0, 0), (1, 1), (2, 1)]),
                (0, &[(0, 0), (1, 1)], &[(0, 0), (1, 0)]),
            ])
        );
        assert_eq!(rows.iter().map(Row::width).max(), Some(3));
    }

    #[test]
    fn a_line_keeping_its_lane_is_straight_and_one_moving_curves() {
        let straight = Edge {
            from: 1,
            to: 1,
            color: 0,
        };
        assert_eq!(path(&straight, false), "M21 0V20");
        assert_eq!(path(&straight, true), "M21 20V40");
        let moving = Edge {
            from: 0,
            to: 2,
            color: 0,
        };
        assert_eq!(path(&moving, true), "M7 20C7 30 35 30 35 40");
    }

    #[test]
    fn a_row_draws_as_wide_as_the_widest_row_with_its_node_over_its_lines() {
        let rows = lay(&["m x y", "x b", "y b", "b"]);
        let got = svg(&rows[3], 2, Marks::default());
        assert_eq!(
            got,
            r#"<svg class="graph" width="28" height="40" viewBox="0 0 28 40"><path class="lane-0" d="M7 0V20"/><circle class="node lane-0" cx="7" cy="20" r="4"/></svg>"#
        );
    }

    #[test]
    fn the_working_copy_s_node_is_larger_and_each_mark_classes_it() {
        let rows = lay(&["a"]);
        let marks = Marks {
            current: true,
            empty: true,
            conflict: true,
        };
        assert!(svg(&rows[0], 1, marks).contains(
            r#"<circle class="node lane-0 current empty conflict" cx="7" cy="20" r="5"/>"#
        ));
    }
}
