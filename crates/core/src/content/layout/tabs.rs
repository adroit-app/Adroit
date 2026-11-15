//! Every tab of the content, and what is done to its tabs and windows.

use super::node::{Axis, EPS, MIN_SIZE, Node, Window, WindowId, share};
use super::tab::ContentTab;
use crate::content::Dir;
use crate::cursor;

/// Where something opened from the tree goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opening {
    /// To a window already showing it, in any tab, or else to a new tab
    /// after the one shown.
    Existing,
    /// In a new window split off the focused one along the axis.
    Split(Axis),
    /// In a new tab after the one shown.
    Tab,
    /// In place of what the window by this id shows, or else as
    /// [`Opening::Existing`] once that window is gone.
    InWindow(WindowId),
}

/// The content's tabs, the one shown, and the one shown before it.
#[derive(Debug, Clone, PartialEq)]
pub struct Content<T> {
    pub(super) tabs: Vec<ContentTab<T>>,
    pub(super) current: usize,
    pub(super) last: Option<u64>,
    pub(super) next: u64,
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

    /// The numbers, counted from 1, of the tabs with a window showing
    /// `thing`.
    pub fn tabs_showing(&self, thing: &T) -> Vec<usize> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(_, t)| t.windows().iter().any(|w| w.shows == *thing))
            .map(|(at, _)| at + 1)
            .collect()
    }

    pub(super) fn window(&mut self, shows: T) -> Window<T> {
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
        let at = if self.tabs.is_empty() {
            0
        } else {
            self.current + 1
        };
        self.tabs.insert(at, ContentTab::single(w.id, w));
        self.show_added(at);
    }

    /// Add `thing` in a new tab where [`Self::open_tab`] would, the tab shown
    /// staying shown; with nothing open, it is shown.
    pub fn open_behind(&mut self, thing: T) {
        if self.tabs.is_empty() {
            return self.open_tab(thing);
        }
        let w = self.window(thing);
        let at = self.current + 1;
        self.tabs.insert(at, ContentTab::single(w.id, w));
        if self.current >= at {
            self.current += 1;
        }
    }

    /// Show the tab just added at `at`; the only tab is shown with none
    /// shown before it.
    fn show_added(&mut self, at: usize) {
        if self.tabs.len() == 1 {
            self.current = 0;
        } else {
            self.set_current(at);
        }
    }

    /// Open `thing`: go to a window of any tab showing it, or else show it
    /// in a new tab.
    pub fn open(&mut self, thing: T) {
        self.open_in(thing, Opening::Existing);
    }

    /// Show `thing` where `how` says; in a new tab when nothing is open.
    pub fn open_in(&mut self, thing: T, how: Opening) {
        match how {
            Opening::Existing => {
                if !self.reveal(&thing) {
                    self.open_tab(thing);
                }
            }
            Opening::Split(axis) => self.split(axis, thing),
            Opening::Tab => self.open_tab(thing),
            Opening::InWindow(id) => {
                if !self.replace_window(id, thing.clone()) {
                    self.open_in(thing, Opening::Existing);
                }
            }
        }
    }

    /// The window `id` shows `thing` in place of what it showed, focused;
    /// whether there was such a window.
    pub fn replace_window(&mut self, id: WindowId, thing: T) -> bool {
        let Some(at) = self
            .tabs
            .iter()
            .position(|t| t.windows().iter().any(|w| w.id == id))
        else {
            return false;
        };
        let tab = &mut self.tabs[at];
        set_shows(&mut tab.root, id, &thing);
        self.set_current(at);
        self.focus(id);
        true
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
        self.current = cursor::within(self.current, self.tabs.len());
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
        let want = parts[at].0 + by;
        share(parts, at, other, want);
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
        share(parts, border, border + 1, at - before);
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
        self.tabs.insert(at, ContentTab::single(tab_id, w));
        self.set_current(at);
    }

    /// Move the window `id`, from any tab, to the `side` of the window
    /// `beside`, in whichever tab that is, focusing it and showing its tab.
    /// The tab it leaves closes when it held nothing else.
    pub fn place(&mut self, id: WindowId, beside: WindowId, side: Dir) {
        if id == beside {
            return;
        }
        let (Some(from), Some(to)) = (
            self.tabs.iter().position(|t| t.has(id)),
            self.tabs.iter().find(|t| t.has(beside)).map(|t| t.id),
        ) else {
            return;
        };
        let shown = self.current().map(|t| t.id);
        let window = if self.tabs[from].windows().len() == 1 {
            let tab = self.tabs.remove(from);
            match tab.root {
                Node::Window(w) => w,
                Node::Split(..) => unreachable!("a tab of one window"),
            }
        } else {
            self.tabs[from]
                .remove(id)
                .expect("the window is in the tab")
        };
        let at = self
            .tabs
            .iter()
            .position(|t| t.id == to)
            .expect("the tab placed into stays");
        let tab = &mut self.tabs[at];
        tab.insert_at(beside, side, window);
        if shown != Some(to) {
            self.last = shown;
        }
        self.current = at;
        self.keep_last();
    }

    /// Merge the tab at `from` into the tab at `to`: every window of it, as
    /// it is split, right of every window there, its focused window keeping
    /// the focus and its tab shown. The tab merged closes.
    pub fn merge_tab(&mut self, from: usize, to: usize) {
        if from == to || from >= self.tabs.len() || to >= self.tabs.len() {
            return;
        }
        let into = self.tabs[to].id;
        self.take_tab(from, |tab, merged| tab.attach(merged, Dir::Right), into);
    }

    /// Move every window of the tab at `from`, as it is split, to the `side`
    /// of the window `beside` in another tab, as [`Content::merge_tab`]
    /// merges it. Placed on a window of its own, the tab shown goes to that
    /// side of every window of the tab [`Content::merge_into_last_tab`]
    /// merges it into; any other tab stays.
    pub fn place_tab(&mut self, from: usize, beside: WindowId, side: Dir) {
        let Some(into) = self.tabs.iter().find(|t| t.has(beside)).map(|t| t.id) else {
            return;
        };
        let Some(tab) = self.tabs.get(from) else {
            return;
        };
        if tab.id == into {
            if from != self.current {
                return;
            }
            if let Some(to) = self.tab_before() {
                let into = self.tabs[to].id;
                self.take_tab(from, |tab, merged| tab.attach(merged, side), into);
            }
            return;
        }
        self.take_tab(
            from,
            |tab, merged| tab.insert_node_at(beside, side, merged.root, merged.focus),
            into,
        );
    }

    /// Take the tab at `from` out and `put` it into the tab `into`, showing
    /// that one.
    fn take_tab(
        &mut self,
        from: usize,
        put: impl FnOnce(&mut ContentTab<T>, ContentTab<T>),
        into: u64,
    ) {
        let shown = self.current().map(|t| t.id);
        let merged = self.tabs.remove(from);
        let at = self
            .tabs
            .iter()
            .position(|t| t.id == into)
            .expect("the tab merged into stays");
        let tab = &mut self.tabs[at];
        put(tab, merged);
        if shown != Some(into) {
            self.last = shown;
        }
        self.current = at;
        self.keep_last();
    }

    /// Merge the shown tab into the tab numbered `n` from 1, or the last
    /// when there are fewer, 9 being the last however many there are.
    pub fn merge_into_numbered_tab(&mut self, n: u8) {
        if self.tabs.is_empty() {
            return;
        }
        let last = self.tabs.len() - 1;
        let at = match n {
            9 => last,
            n => usize::from(n).saturating_sub(1).min(last),
        };
        self.merge_tab(self.current, at);
    }

    /// Merge the shown tab into the tab shown before it, or else into the
    /// tab beside it, the one before it unless it is the first.
    pub fn merge_into_last_tab(&mut self) {
        if let Some(to) = self.tab_before() {
            self.merge_tab(self.current, to);
        }
    }

    /// The tab shown before the one shown now, or else the tab beside it,
    /// the one before it unless it is the first; none when it is alone.
    fn tab_before(&self) -> Option<usize> {
        let before = self.tabs.iter().position(|t| Some(t.id) == self.last);
        let beside = match self.current {
            0 => 1,
            n => n - 1,
        };
        Some(before.unwrap_or(beside)).filter(|to| *to != self.current && *to < self.tabs.len())
    }

    /// Move the tab at `from` to the place `to` among the tabs, the shown tab
    /// staying shown.
    pub fn move_tab(&mut self, from: usize, to: usize) {
        let n = self.tabs.len();
        if from == to || from >= n || to >= n {
            return;
        }
        let shown = self.current().map(|t| t.id);
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        if let Some(at) = self.tabs.iter().position(|t| Some(t.id) == shown) {
            self.current = at;
        }
    }

    /// Move the shown tab `delta` places along the tabs, stopping at the
    /// first and the last.
    pub fn shift_tab(&mut self, delta: isize) {
        let to = cursor::clamp(self.current, delta, self.tabs.len());
        self.move_tab(self.current, to);
    }

    /// Show the tab `delta` after the shown one, going round at the ends.
    pub fn cycle_tab(&mut self, delta: isize) {
        let n = self.tabs.len();
        if n > 0 {
            self.set_current(cursor::wrap(self.current, delta, n));
        }
    }

    /// Show the tab at `at`, counted from 0, or the last tab when there are
    /// not that many.
    pub fn go_to_tab(&mut self, at: usize) {
        if !self.tabs.is_empty() {
            self.set_current(cursor::within(at, self.tabs.len()));
        }
    }

    /// Show the tab numbered `n` from 1, or the last when there are fewer;
    /// 9 is the last however many there are.
    pub fn go_to_numbered_tab(&mut self, n: u8) {
        match n {
            9 => self.go_to_tab(usize::MAX),
            n => self.go_to_tab(usize::from(n).saturating_sub(1)),
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
        self.current = cursor::within(before, self.tabs.len());
        self.keep_last();
    }
}

fn set_shows<T: Clone>(node: &mut Node<T>, id: WindowId, thing: &T) {
    match node {
        Node::Window(w) if w.id == id => w.shows = thing.clone(),
        Node::Window(_) => {}
        Node::Split(_, parts) => parts.iter_mut().for_each(|(_, n)| set_shows(n, id, thing)),
    }
}

fn replace_in<T: Clone + PartialEq>(node: &mut Node<T>, from: &T, to: &T) {
    match node {
        Node::Window(w) if w.shows == *from => w.shows = to.clone(),
        Node::Window(_) => {}
        Node::Split(_, parts) => parts.iter_mut().for_each(|(_, n)| replace_in(n, from, to)),
    }
}
