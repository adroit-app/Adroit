//! What the content holds: tabs, each split into windows as Neovim splits its
//! tabs, every window showing one thing. Windows only show; what they show
//! runs on whether or not a window shows it.

use crate::focus::Dir;

/// A window's identity, unique across every tab of the content.
pub type WindowId = u64;

/// The least share of a split a part is resized or dragged to.
pub const MIN_SIZE: f64 = 0.05;

/// The edge shared by windows that touch, as a share of the content.
const EPS: f64 = 1e-6;

/// How a split lays its parts out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Side by side, as Neovim's `:vsplit` lays them.
    Row,
    /// One above the other, as Neovim's `:split` lays them.
    Column,
}

impl Axis {
    fn of(dir: Dir) -> Self {
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

    fn right(&self) -> f64 {
        self.left + self.width
    }

    fn bottom(&self) -> f64 {
        self.top + self.height
    }
}

impl<T> Node<T> {
    fn windows<'a>(&'a self, out: &mut Vec<&'a Window<T>>) {
        match self {
            Node::Window(w) => out.push(w),
            Node::Split(_, parts) => parts.iter().for_each(|(_, n)| n.windows(out)),
        }
    }

    fn first(&self) -> WindowId {
        match self {
            Node::Window(w) => w.id,
            Node::Split(_, parts) => parts[0].1.first(),
        }
    }

    fn last(&self) -> WindowId {
        match self {
            Node::Window(w) => w.id,
            Node::Split(_, parts) => parts[parts.len() - 1].1.last(),
        }
    }

    /// The parts to follow from here to the window `id`.
    fn path(&self, id: WindowId) -> Option<Vec<usize>> {
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

    fn at(&self, path: &[usize]) -> &Node<T> {
        match (self, path.split_first()) {
            (Node::Split(_, parts), Some((i, rest))) => parts[*i].1.at(rest),
            _ => self,
        }
    }

    fn at_mut(&mut self, path: &[usize]) -> &mut Node<T> {
        match path.split_first() {
            Some((i, rest)) => match self {
                Node::Split(_, parts) => parts[*i].1.at_mut(rest),
                Node::Window(_) => self,
            },
            None => self,
        }
    }

    fn rects(&self, at: Rect, out: &mut Vec<(WindowId, Rect)>) {
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

    fn equalize(&mut self) {
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
    fn normalized(self) -> Node<T> {
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
    fn borders(&self, path: Vec<usize>, at: Rect, out: &mut Vec<Border>) {
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

/// A tab of the content: its windows, and which of them has the focus.
#[derive(Debug, Clone, PartialEq)]
pub struct ContentTab<T> {
    id: u64,
    root: Node<T>,
    focus: WindowId,
    previous: Option<WindowId>,
}

impl<T: Clone + PartialEq> ContentTab<T> {
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

    fn has(&self, id: WindowId) -> bool {
        self.root.path(id).is_some()
    }

    fn focus_on(&mut self, id: WindowId) {
        if id != self.focus {
            self.previous = Some(self.focus);
            self.focus = id;
        }
    }

    /// Take the window `id` out of a tab that has another, focusing the
    /// window it leaves its room to unless it was not focused.
    fn remove(&mut self, id: WindowId) -> Option<Window<T>> {
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
        let root = std::mem::replace(&mut self.root, Node::Split(Axis::Row, Vec::new()));
        self.root = root.normalized();
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

    fn insert_beside(&mut self, axis: Axis, window: Window<T>) {
        let path = self.root.path(self.focus).expect("the focus is in the tab");
        let leaf = self.root.at_mut(&path);
        let old = std::mem::replace(leaf, Node::Split(axis, Vec::new()));
        let id = window.id;
        *leaf = Node::Split(axis, vec![(0.5, old), (0.5, Node::Window(window))]);
        let root = std::mem::replace(&mut self.root, Node::Split(Axis::Row, Vec::new()));
        self.root = root.normalized();
        self.focus_on(id);
    }

    /// The splits along `axis` the focused window lies in, deepest first, and
    /// which of their parts it lies in.
    fn splits_along(&self, axis: Axis) -> Vec<(Vec<usize>, usize)> {
        let path = self.root.path(self.focus).unwrap_or_default();
        (0..path.len())
            .rev()
            .filter(|&k| matches!(self.root.at(&path[..k]), Node::Split(a, _) if *a == axis))
            .map(|k| (path[..k].to_vec(), path[k]))
            .collect()
    }
}

/// The content's tabs, the one shown, and the one shown before it.
#[derive(Debug, Clone, PartialEq)]
pub struct Content<T> {
    tabs: Vec<ContentTab<T>>,
    current: usize,
    last: Option<u64>,
    next: u64,
}

impl<T> Default for Content<T> {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            current: 0,
            last: None,
            next: 1,
        }
    }
}

impl<T: Clone + PartialEq> Content<T> {
    pub fn tabs(&self) -> &[ContentTab<T>] {
        &self.tabs
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// Which tab is shown, by its place among the tabs.
    pub fn current_index(&self) -> usize {
        self.current
    }

    pub fn current(&self) -> Option<&ContentTab<T>> {
        self.tabs.get(self.current)
    }

    fn current_mut(&mut self) -> Option<&mut ContentTab<T>> {
        self.tabs.get_mut(self.current)
    }

    pub fn focused_window(&self) -> Option<WindowId> {
        self.current().map(|t| t.focus)
    }

    /// What the shown tab's focused window shows.
    pub fn focused(&self) -> Option<&T> {
        self.current().map(|t| &t.focused().shows)
    }

    /// What the window `id` shows, in any tab.
    pub fn shows(&self, id: WindowId) -> Option<&T> {
        self.tabs
            .iter()
            .flat_map(|t| t.windows())
            .find(|w| w.id == id)
            .map(|w| &w.shows)
    }

    /// Everything some window shows, in any tab.
    pub fn shown(&self) -> Vec<&T> {
        let mut out: Vec<&T> = Vec::new();
        for w in self.tabs.iter().flat_map(|t| t.windows()) {
            if !out.contains(&&w.shows) {
                out.push(&w.shows);
            }
        }
        out
    }

    /// Whether some window of any tab shows `thing`.
    pub fn shows_anywhere(&self, thing: &T) -> bool {
        self.tabs
            .iter()
            .any(|t| t.windows().iter().any(|w| w.shows == *thing))
    }

    fn window(&mut self, shows: T) -> Window<T> {
        let id = self.next;
        self.next += 1;
        Window { id, shows }
    }

    fn set_current(&mut self, at: usize) {
        if at != self.current && at < self.tabs.len() {
            self.last = self.current().map(|t| t.id);
            self.current = at;
        }
    }

    /// Show `thing` in a new tab after the one shown.
    pub fn open_tab(&mut self, thing: T) {
        let w = self.window(thing);
        let tab = ContentTab {
            id: w.id,
            focus: w.id,
            root: Node::Window(w),
            previous: None,
        };
        let at = if self.tabs.is_empty() {
            0
        } else {
            self.current + 1
        };
        self.tabs.insert(at, tab);
        if self.tabs.len() == 1 {
            self.current = 0;
        } else {
            self.set_current(at);
        }
    }

    /// Show `thing` where the user is: in the window of the shown tab that
    /// already shows it, or else in the focused window in place of what it
    /// shows.
    pub fn open(&mut self, thing: T) {
        let Some(tab) = self.current_mut() else {
            return self.open_tab(thing);
        };
        let already = tab
            .windows()
            .iter()
            .find(|w| w.shows == thing)
            .map(|w| w.id);
        match already {
            Some(id) => tab.focus_on(id),
            None => {
                let path = tab.root.path(tab.focus).expect("the focus is in the tab");
                if let Node::Window(w) = tab.root.at_mut(&path) {
                    w.shows = thing;
                }
            }
        }
    }

    /// Split the focused window along `axis`, the new window after it showing
    /// `thing` and taking the focus.
    pub fn split(&mut self, axis: Axis, thing: T) {
        if self.tabs.is_empty() {
            return self.open_tab(thing);
        }
        let w = self.window(thing);
        if let Some(tab) = self.current_mut() {
            tab.insert_beside(axis, w);
        }
    }

    /// Go to where `thing` is shown: a window of the shown tab, or else of
    /// the first tab that shows it; `false` when none does.
    pub fn reveal(&mut self, thing: &T) -> bool {
        let here = self
            .current()
            .and_then(|t| t.windows().into_iter().find(|w| w.shows == *thing))
            .map(|w| w.id);
        let anywhere = || {
            self.tabs
                .iter()
                .flat_map(|t| t.windows())
                .find(|w| w.shows == *thing)
                .map(|w| w.id)
        };
        match here.or_else(anywhere) {
            Some(id) => self.focus(id),
            None => false,
        }
    }

    /// Focus the window `id`, showing its tab.
    pub fn focus(&mut self, id: WindowId) -> bool {
        let Some(at) = self.tabs.iter().position(|t| t.has(id)) else {
            return false;
        };
        self.set_current(at);
        self.tabs[at].focus_on(id);
        true
    }

    /// Close the focused window, and its tab with it when it was the last.
    pub fn close_window(&mut self) {
        let Some(tab) = self.current_mut() else {
            return;
        };
        if tab.windows().len() == 1 {
            return self.close_tab();
        }
        let id = tab.focus;
        tab.remove(id);
    }

    /// Close the shown tab and every window in it, showing the tab after it,
    /// or before it when it was the last.
    pub fn close_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.tabs.remove(self.current);
        self.current = self.current.min(self.tabs.len().saturating_sub(1));
        self.keep_last();
    }

    fn keep_last(&mut self) {
        let shown = self.current().map(|t| t.id);
        if self
            .last
            .is_some_and(|l| Some(l) == shown || !self.tabs.iter().any(|t| t.id == l))
        {
            self.last = None;
        }
    }

    /// Close every window but the focused one.
    pub fn only(&mut self) {
        if let Some(tab) = self.current_mut() {
            let w = tab.focused().clone();
            tab.root = Node::Window(w);
            tab.previous = None;
        }
    }

    /// Move the focused window to the far side of its tab, as wide or as high
    /// as the tab.
    pub fn move_to_edge(&mut self, dir: Dir) {
        let Some(tab) = self.current_mut() else {
            return;
        };
        let id = tab.focus;
        let previous = tab.previous;
        let Some(w) = tab.remove(id) else {
            return;
        };
        let axis = Axis::of(dir);
        let along = match &tab.root {
            Node::Split(a, parts) if *a == axis => parts.len(),
            _ => 1,
        };
        let size = 1.0 / (along as f64 + 1.0);
        let rest = std::mem::replace(&mut tab.root, Node::Split(axis, Vec::new()));
        let mut parts = vec![(1.0 - size, rest), (size, Node::Window(w))];
        if matches!(dir, Dir::Left | Dir::Up) {
            parts.reverse();
        }
        tab.root = Node::Split(axis, parts).normalized();
        tab.focus = id;
        tab.previous = previous.filter(|p| *p != id);
    }

    /// Exchange the focused window with the next one in its split, or the one
    /// before it when it is the last, the focus staying where it was. A
    /// window beside a split stays.
    pub fn exchange(&mut self) {
        let Some(tab) = self.current_mut() else {
            return;
        };
        let path = tab.root.path(tab.focus).unwrap_or_default();
        let Some((at, parent)) = path.split_last() else {
            return;
        };
        let Node::Split(_, parts) = tab.root.at_mut(parent) else {
            return;
        };
        let other = if at + 1 < parts.len() { at + 1 } else { at - 1 };
        let Node::Window(w) = &parts[other].1 else {
            return;
        };
        let id = w.id;
        let (a, b) = (*at.min(&other), *at.max(&other));
        let (head, tail) = parts.split_at_mut(b);
        std::mem::swap(&mut head[a].1, &mut tail[0].1);
        tab.focus_on(id);
    }

    /// Rotate the windows of the focused window's split forward, the last
    /// becoming the first, the focus going with its window. A split holding
    /// another split stays.
    pub fn rotate(&mut self) {
        let Some(tab) = self.current_mut() else {
            return;
        };
        let path = tab.root.path(tab.focus).unwrap_or_default();
        let Some((_, parent)) = path.split_last() else {
            return;
        };
        let Node::Split(_, parts) = tab.root.at_mut(parent) else {
            return;
        };
        if parts.iter().any(|(_, n)| matches!(n, Node::Split(..))) {
            return;
        }
        let sizes: Vec<f64> = parts.iter().map(|(s, _)| *s).collect();
        parts.rotate_right(1);
        for (part, size) in parts.iter_mut().zip(sizes) {
            part.0 = size;
        }
    }

    /// Give every part of every split of the shown tab the same share.
    pub fn equalize(&mut self) {
        if let Some(tab) = self.current_mut() {
            tab.root.equalize();
        }
    }

    /// Grow the focused window by `by` of its split along `axis`, or shrink
    /// it when `by` is negative, from the part after it, or before it when it
    /// is the last; no part goes below [`MIN_SIZE`].
    pub fn resize(&mut self, axis: Axis, by: f64) {
        let Some(tab) = self.current_mut() else {
            return;
        };
        let Some((split, at)) = tab.splits_along(axis).into_iter().next() else {
            return;
        };
        let Node::Split(_, parts) = tab.root.at_mut(&split) else {
            return;
        };
        let other = if at + 1 < parts.len() { at + 1 } else { at - 1 };
        let pair = parts[at].0 + parts[other].0;
        let size =
            (parts[at].0 + by).clamp(MIN_SIZE.min(pair / 2.0), (pair - MIN_SIZE).max(pair / 2.0));
        parts[at].0 = size;
        parts[other].0 = pair - size;
    }

    /// Make the focused window as large along `axis` as the others allow.
    pub fn maximize(&mut self, axis: Axis) {
        let Some(tab) = self.current_mut() else {
            return;
        };
        for (split, at) in tab.splits_along(axis) {
            let Node::Split(_, parts) = tab.root.at_mut(&split) else {
                continue;
            };
            let n = parts.len() as f64;
            let least = MIN_SIZE.min(0.5 / n);
            for (i, part) in parts.iter_mut().enumerate() {
                part.0 = if i == at {
                    1.0 - least * (n - 1.0)
                } else {
                    least
                };
            }
        }
    }

    /// Move the border after part `border` of the split at `split` in the
    /// shown tab to `at`, a share of that split, keeping both parts beside
    /// it at least [`MIN_SIZE`].
    pub fn drag(&mut self, split: &[usize], border: usize, at: f64) {
        let Some(tab) = self.current_mut() else {
            return;
        };
        let Node::Split(_, parts) = tab.root.at_mut(split) else {
            return;
        };
        if border + 1 >= parts.len() {
            return;
        }
        let before: f64 = parts[..border].iter().map(|(s, _)| s).sum();
        let pair = parts[border].0 + parts[border + 1].0;
        let size = (at - before).clamp(MIN_SIZE.min(pair / 2.0), (pair - MIN_SIZE).max(pair / 2.0));
        parts[border].0 = size;
        parts[border + 1].0 = pair - size;
    }

    /// Move the focused window to a new tab after the shown one, unless it is
    /// the only window of its tab.
    pub fn to_new_tab(&mut self) {
        let Some(tab) = self.current_mut() else {
            return;
        };
        let id = tab.focus;
        let Some(w) = tab.remove(id) else {
            return;
        };
        let tab_id = self.next;
        self.next += 1;
        let at = self.current + 1;
        self.tabs.insert(
            at,
            ContentTab {
                id: tab_id,
                root: Node::Window(w),
                focus: id,
                previous: None,
            },
        );
        self.set_current(at);
    }

    /// Show the tab `delta` after the shown one, going round at the ends.
    pub fn cycle_tab(&mut self, delta: isize) {
        let n = self.tabs.len();
        if n > 0 {
            self.set_current((self.current as isize + delta).rem_euclid(n as isize) as usize);
        }
    }

    /// Show the tab at `at`, counted from 0, or the last tab when there are
    /// not that many.
    pub fn go_to_tab(&mut self, at: usize) {
        if !self.tabs.is_empty() {
            self.set_current(at.min(self.tabs.len() - 1));
        }
    }

    /// Show the tab shown before the one shown now.
    pub fn go_to_last_tab(&mut self) {
        if let Some(at) = self.tabs.iter().position(|t| Some(t.id) == self.last) {
            self.set_current(at);
        }
    }

    /// The window beside the focused one toward `dir`: of those touching it
    /// there, the one focused before it, or else the one sharing the most of
    /// its side.
    pub fn neighbour(&self, dir: Dir) -> Option<WindowId> {
        let tab = self.current()?;
        let rects = tab.rects();
        let (_, r) = *rects.iter().find(|(id, _)| *id == tab.focus)?;
        let near = |a: f64, b: f64| (a - b).abs() < EPS;
        let overlap = |a: (f64, f64), b: (f64, f64)| a.1.min(b.1) - a.0.max(b.0);
        let touching: Vec<(WindowId, f64)> = rects
            .iter()
            .filter(|(id, _)| *id != tab.focus)
            .filter_map(|(id, c)| {
                let (touches, shared) = match dir {
                    Dir::Left => (
                        near(c.right(), r.left),
                        overlap((c.top, c.bottom()), (r.top, r.bottom())),
                    ),
                    Dir::Right => (
                        near(c.left, r.right()),
                        overlap((c.top, c.bottom()), (r.top, r.bottom())),
                    ),
                    Dir::Up => (
                        near(c.bottom(), r.top),
                        overlap((c.left, c.right()), (r.left, r.right())),
                    ),
                    Dir::Down => (
                        near(c.top, r.bottom()),
                        overlap((c.left, c.right()), (r.left, r.right())),
                    ),
                };
                (touches && shared > EPS).then_some((*id, shared))
            })
            .collect();
        if let Some(p) = tab
            .previous
            .filter(|p| touching.iter().any(|(id, _)| id == p))
        {
            return Some(p);
        }
        touching
            .iter()
            .fold(None::<(WindowId, f64)>, |best, &(id, shared)| match best {
                Some((_, s)) if s + EPS >= shared => best,
                _ => Some((id, shared)),
            })
            .map(|(id, _)| id)
    }

    /// The window a motion coming in from the side `dir` points away from
    /// lands in: the focused window if it lies along that side, or else the
    /// one focused before it, or else the first there.
    pub fn entered_from(&self, dir: Dir) -> Option<WindowId> {
        let tab = self.current()?;
        let rects = tab.rects();
        let along: Vec<WindowId> = rects
            .iter()
            .filter(|(_, r)| match dir {
                Dir::Right => r.left < EPS,
                Dir::Left => r.right() > 1.0 - EPS,
                Dir::Down => r.top < EPS,
                Dir::Up => r.bottom() > 1.0 - EPS,
            })
            .map(|(id, _)| *id)
            .collect();
        [Some(tab.focus), tab.previous]
            .into_iter()
            .flatten()
            .find(|id| along.contains(id))
            .or_else(|| along.first().copied())
    }

    /// Show `to` in every window that shows `from`.
    pub fn replace(&mut self, from: &T, to: T) {
        for tab in &mut self.tabs {
            replace_in(&mut tab.root, from, &to);
        }
    }

    /// Close every window whose thing `gone` says is gone, and every tab
    /// left with none.
    pub fn close_showing(&mut self, gone: impl Fn(&T) -> bool) {
        let shown = self.current;
        let mut kept = Vec::new();
        // How many tabs kept came before the one shown.
        let mut before = 0;
        for (at, mut tab) in std::mem::take(&mut self.tabs).into_iter().enumerate() {
            let doomed: Vec<WindowId> = tab
                .windows()
                .iter()
                .filter(|w| gone(&w.shows))
                .map(|w| w.id)
                .collect();
            if doomed.len() == tab.windows().len() {
                continue;
            }
            for id in doomed {
                tab.remove(id);
            }
            if at < shown {
                before += 1;
            }
            kept.push(tab);
        }
        self.tabs = kept;
        self.current = before.min(self.tabs.len().saturating_sub(1));
        self.keep_last();
    }
}

fn replace_in<T: Clone + PartialEq>(node: &mut Node<T>, from: &T, to: &T) {
    match node {
        Node::Window(w) if w.shows == *from => w.shows = to.clone(),
        Node::Window(_) => {}
        Node::Split(_, parts) => parts.iter_mut().for_each(|(_, n)| replace_in(n, from, to)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn opening_with_nothing_open_makes_a_tab_with_one_window() {
        let c = one("a");
        assert_eq!(c.tabs().len(), 1);
        assert_eq!(c.focused(), Some(&"a"));
    }

    #[test]
    fn opening_shows_the_thing_in_the_focused_window_in_place_of_what_it_showed() {
        let mut c = one("a");
        c.split(Axis::Row, "b");
        c.open("c");
        assert_eq!(drawn(&c), "(a | c)");
        assert_eq!(c.focused(), Some(&"c"));
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
    fn opening_what_only_another_tab_shows_shows_it_here_too() {
        let mut c = one("a");
        c.open_tab("b");
        c.open("a");
        assert_eq!(c.tabs().len(), 2);
        assert_eq!(c.current_index(), 1);
        assert_eq!(drawn(&c), "a");
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
    fn exchanging_swaps_with_the_next_window_or_the_one_before_the_last_and_keeps_the_focus_in_place()
     {
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
    fn what_is_shown_is_listed_once_however_many_windows_show_it() {
        let mut c = one("a");
        c.split(Axis::Row, "a");
        c.open_tab("b");
        assert_eq!(c.shown(), vec![&"a", &"b"]);
        assert!(c.shows_anywhere(&"b"));
        assert!(!c.shows_anywhere(&"c"));
    }
}
