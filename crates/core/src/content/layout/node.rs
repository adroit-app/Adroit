//! A tab's windows as a tree of splits: the share of the content each
//! window is drawn in, and the borders between them.

use serde::{Deserialize, Serialize};

use crate::content::Dir;

/// A window's identity, unique across every tab of the content.
pub type WindowId = u64;

/// The least share of a split a part is resized or dragged to.
pub const MIN_SIZE: f64 = 0.05;

/// The edge shared by windows that touch, as a share of the content.
pub(super) const EPS: f64 = 1e-6;

/// How a split lays its parts out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    /// Side by side, as Neovim's `:vsplit` lays them.
    Row,
    /// One above the other, as Neovim's `:split` lays them.
    Column,
}

impl Axis {
    pub(super) fn of(dir: Dir) -> Self {
        match dir {
            Dir::Left | Dir::Right => Axis::Row,
            Dir::Up | Dir::Down => Axis::Column,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Window<T> {
    pub id: WindowId,
    pub shows: T,
}

/// A tab's layout: a window, or a split of parts, each with its share of the
/// split. A split has two parts or more, none of them split along its axis.
#[derive(Debug, Clone, PartialEq)]
pub enum Node<T> {
    Window(Window<T>),
    Split(Axis, Vec<(f64, Node<T>)>),
}

/// Where a window is drawn, as shares of the content's width and height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const WHOLE: Rect = Rect {
        left: 0.0,
        top: 0.0,
        width: 1.0,
        height: 1.0,
    };

    pub(super) fn right(&self) -> f64 {
        self.left + self.width
    }

    pub(super) fn bottom(&self) -> f64 {
        self.top + self.height
    }
}

impl<T> Node<T> {
    pub(super) fn windows<'a>(&'a self, out: &mut Vec<&'a Window<T>>) {
        match self {
            Node::Window(w) => out.push(w),
            Node::Split(_, parts) => parts.iter().for_each(|(_, n)| n.windows(out)),
        }
    }

    pub(super) fn first(&self) -> WindowId {
        match self {
            Node::Window(w) => w.id,
            Node::Split(_, parts) => parts[0].1.first(),
        }
    }

    pub(super) fn last(&self) -> WindowId {
        match self {
            Node::Window(w) => w.id,
            Node::Split(_, parts) => parts[parts.len() - 1].1.last(),
        }
    }

    /// The parts to follow from here to the window `id`.
    pub(super) fn path(&self, id: WindowId) -> Option<Vec<usize>> {
        match self {
            Node::Window(w) => (w.id == id).then(Vec::new),
            Node::Split(_, parts) => parts.iter().enumerate().find_map(|(i, (_, n))| {
                n.path(id).map(|mut p| {
                    p.insert(0, i);
                    p
                })
            }),
        }
    }

    pub(super) fn at(&self, path: &[usize]) -> &Node<T> {
        match (self, path.split_first()) {
            (Node::Split(_, parts), Some((i, rest))) => parts[*i].1.at(rest),
            _ => self,
        }
    }

    pub(super) fn at_mut(&mut self, path: &[usize]) -> &mut Node<T> {
        match path.split_first() {
            Some((i, rest)) => match self {
                Node::Split(_, parts) => parts[*i].1.at_mut(rest),
                Node::Window(_) => self,
            },
            None => self,
        }
    }

    pub(super) fn rects(&self, at: Rect, out: &mut Vec<(WindowId, Rect)>) {
        match self {
            Node::Window(w) => out.push((w.id, at)),
            Node::Split(axis, parts) => {
                let mut start = 0.0;
                for (size, n) in parts {
                    let r = match axis {
                        Axis::Row => Rect {
                            left: at.left + start * at.width,
                            width: size * at.width,
                            ..at
                        },
                        Axis::Column => Rect {
                            top: at.top + start * at.height,
                            height: size * at.height,
                            ..at
                        },
                    };
                    n.rects(r, out);
                    start += size;
                }
            }
        }
    }

    pub(super) fn equalize(&mut self) {
        if let Node::Split(_, parts) = self {
            let n = parts.len() as f64;
            for (size, node) in parts {
                *size = 1.0 / n;
                node.equalize();
            }
        }
    }

    /// The layout with no split of one part and no split inside another
    /// along the same axis, its shares adding up to the whole.
    pub(super) fn normalized(self) -> Node<T> {
        let Node::Split(axis, parts) = self else {
            return self;
        };
        let mut out = Vec::new();
        for (size, node) in parts {
            match node.normalized() {
                Node::Split(inner, sub) if inner == axis => {
                    out.extend(sub.into_iter().map(|(s, n)| (s * size, n)));
                }
                node => out.push((size, node)),
            }
        }
        if out.len() == 1 {
            return out.pop().map(|(_, n)| n).expect("one part");
        }
        let total: f64 = out.iter().map(|(s, _)| s).sum();
        for (size, _) in &mut out {
            *size /= total;
        }
        Node::Split(axis, out)
    }
}

/// A border between two parts of a split, which a drag moves.
#[derive(Debug, Clone, PartialEq)]
pub struct Border {
    /// The parts to follow to the split.
    pub split: Vec<usize>,
    /// The part the border comes after.
    pub after: usize,
    pub axis: Axis,
    /// Where the split is drawn.
    pub within: Rect,
    /// Where the border lies, as a share of the split.
    pub at: f64,
}

impl<T> Node<T> {
    pub(super) fn borders(&self, path: Vec<usize>, at: Rect, out: &mut Vec<Border>) {
        let Node::Split(axis, parts) = self else {
            return;
        };
        let mut start = 0.0;
        for (i, (size, _)) in parts.iter().enumerate().take(parts.len() - 1) {
            start += size;
            out.push(Border {
                split: path.clone(),
                after: i,
                axis: *axis,
                within: at,
                at: start,
            });
        }
        let mut rects = Vec::new();
        let mut start = 0.0;
        for (size, _) in parts {
            rects.push(match axis {
                Axis::Row => Rect {
                    left: at.left + start * at.width,
                    width: size * at.width,
                    ..at
                },
                Axis::Column => Rect {
                    top: at.top + start * at.height,
                    height: size * at.height,
                    ..at
                },
            });
            start += size;
        }
        for (i, ((_, n), r)) in parts.iter().zip(rects).enumerate() {
            let mut p = path.clone();
            p.push(i);
            n.borders(p, r, out);
        }
    }
}

/// Part `a` of `parts` given `want` of the room it shares with part `b`, the
/// rest left to `b`; neither goes below [`MIN_SIZE`] unless the room is too
/// small for both, when neither goes below half of it.
pub(super) fn share<T>(parts: &mut [(f64, Node<T>)], a: usize, b: usize, want: f64) {
    let pair = parts[a].0 + parts[b].0;
    let size = want.clamp(MIN_SIZE.min(pair / 2.0), (pair - MIN_SIZE).max(pair / 2.0));
    parts[a].0 = size;
    parts[b].0 = pair - size;
}
