//! One tab of the content: its windows, which of them has the focus, and how
//! a window is taken out of its layout or put into it.

use super::node::{Axis, Border, Node, Rect, Window, WindowId};
use crate::content::Dir;

/// A tab of the content: its windows, and which of them has the focus.
#[derive(Debug, Clone, PartialEq)]
pub struct ContentTab<T> {
    pub(super) id: u64,
    pub(super) root: Node<T>,
    pub(super) focus: WindowId,
    pub(super) previous: Option<WindowId>,
}

impl<T: Clone + PartialEq> ContentTab<T> {
    /// A tab, told apart by `id`, of the window `w` alone, focused.
    pub(super) fn single(id: u64, w: Window<T>) -> Self {
        ContentTab {
            id,
            focus: w.id,
            root: Node::Window(w),
            previous: None,
        }
    }

    pub fn root(&self) -> &Node<T> {
        &self.root
    }

    pub fn windows(&self) -> Vec<&Window<T>> {
        let mut out = Vec::new();
        self.root.windows(&mut out);
        out
    }

    pub fn focused(&self) -> &Window<T> {
        self.windows()
            .into_iter()
            .find(|w| w.id == self.focus)
            .expect("a tab's focus is one of its windows")
    }

    /// Where each window is drawn, in the order the next window visits them.
    pub fn rects(&self) -> Vec<(WindowId, Rect)> {
        let mut out = Vec::new();
        self.root.rects(Rect::WHOLE, &mut out);
        out
    }

    /// Every border between two parts of a split, outermost first.
    pub fn borders(&self) -> Vec<Border> {
        let mut out = Vec::new();
        self.root.borders(Vec::new(), Rect::WHOLE, &mut out);
        out
    }

    pub(super) fn has(&self, id: WindowId) -> bool {
        self.root.path(id).is_some()
    }

    pub(super) fn focus_on(&mut self, id: WindowId) {
        if id != self.focus {
            self.previous = Some(self.focus);
            self.focus = id;
        }
    }

    /// Take the window `id` out of a tab that has another, focusing the
    /// window it leaves its room to unless it was not focused.
    pub(super) fn remove(&mut self, id: WindowId) -> Option<Window<T>> {
        let path = self.root.path(id).filter(|p| !p.is_empty())?;
        let (at, parent) = path.split_last().expect("a path into a split");
        let Node::Split(_, parts) = self.root.at_mut(parent) else {
            unreachable!("a window's parent is a split");
        };
        let (size, node) = parts.remove(*at);
        let heir = if *at > 0 { *at - 1 } else { 0 };
        parts[heir].0 += size;
        let took = if *at > 0 {
            parts[heir].1.last()
        } else {
            parts[heir].1.first()
        };
        self.normalize();
        if self.previous == Some(id) {
            self.previous = None;
        }
        if self.focus == id {
            self.focus = match self.previous.take() {
                Some(p) if self.has(p) => p,
                _ => took,
            };
        }
        match node {
            Node::Window(w) => Some(w),
            Node::Split(..) => unreachable!("the path ends at a window"),
        }
    }

    fn normalize(&mut self) {
        let root = std::mem::replace(&mut self.root, Node::Split(Axis::Row, Vec::new()));
        self.root = root.normalized();
    }

    pub(super) fn insert_beside(&mut self, axis: Axis, window: Window<T>) {
        let side = match axis {
            Axis::Row => Dir::Right,
            Axis::Column => Dir::Down,
        };
        self.insert_at(self.focus, side, window);
    }

    /// Split the window `beside` toward `side`, `window` taking that half
    /// and the focus.
    pub(super) fn insert_at(&mut self, beside: WindowId, side: Dir, window: Window<T>) {
        let id = window.id;
        self.insert_node_at(beside, side, Node::Window(window), id);
    }

    /// Split the window `beside` toward `side`, the layout `node` taking that
    /// half and its window `focus` the focus.
    pub(super) fn insert_node_at(
        &mut self,
        beside: WindowId,
        side: Dir,
        node: Node<T>,
        focus: WindowId,
    ) {
        let path = self.root.path(beside).expect("the window is in the tab");
        let axis = Axis::of(side);
        let leaf = self.root.at_mut(&path);
        let old = std::mem::replace(leaf, Node::Split(axis, Vec::new()));
        let mut parts = vec![(0.5, old), (0.5, node)];
        if matches!(side, Dir::Left | Dir::Up) {
            parts.reverse();
        }
        *leaf = Node::Split(axis, parts);
        self.normalize();
        self.focus_on(focus);
    }

    /// How many parts the tab lies in along `axis` at its top.
    fn parts_along(&self, axis: Axis) -> usize {
        match &self.root {
            Node::Split(a, parts) if *a == axis => parts.len(),
            _ => 1,
        }
    }

    /// Put the layout of `other` on the `side` of every window, as large as
    /// its parts along that side's axis make it among the parts of both, its
    /// focused window taking the focus.
    pub(super) fn attach(&mut self, other: ContentTab<T>, side: Dir) {
        let axis = Axis::of(side);
        let (mine, theirs) = (
            self.parts_along(axis) as f64,
            other.parts_along(axis) as f64,
        );
        let share = theirs / (mine + theirs);
        let old = std::mem::replace(&mut self.root, Node::Split(axis, Vec::new()));
        let mut parts = vec![(1.0 - share, old), (share, other.root)];
        if matches!(side, Dir::Left | Dir::Up) {
            parts.reverse();
        }
        self.root = Node::Split(axis, parts);
        self.normalize();
        self.focus_on(other.focus);
    }

    /// The splits along `axis` the focused window lies in, deepest first, and
    /// which of their parts it lies in.
    pub(super) fn splits_along(&self, axis: Axis) -> Vec<(Vec<usize>, usize)> {
        let path = self.root.path(self.focus).unwrap_or_default();
        (0..path.len())
            .rev()
            .filter(|&k| matches!(self.root.at(&path[..k]), Node::Split(a, _) if *a == axis))
            .map(|k| (path[..k].to_vec(), path[k]))
            .collect()
    }
}
