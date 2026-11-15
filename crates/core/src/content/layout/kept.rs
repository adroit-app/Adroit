//! The content as it is kept from one run to the next.

use serde::{Deserialize, Serialize};

use super::node::{Axis, MIN_SIZE, Node, Window};
use super::tab::ContentTab;
use super::tabs::Content;
use crate::cursor;

/// The content as it is kept from one run to the next: each tab's layout
/// with the window focused in it, by its place among the tab's windows, and
/// the tab shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Kept<T> {
    pub tabs: Vec<KeptTab<T>>,
    pub current: usize,
}

impl<T> Default for Kept<T> {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            current: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeptTab<T> {
    pub root: KeptNode<T>,
    pub focus: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeptNode<T> {
    Window(T),
    Split {
        axis: Axis,
        parts: Vec<(f64, KeptNode<T>)>,
    },
}

impl<T> Node<T> {
    fn map<U>(self, f: &impl Fn(T) -> U) -> Node<U> {
        match self {
            Node::Window(w) => Node::Window(Window {
                id: w.id,
                shows: f(w.shows),
            }),
            Node::Split(axis, parts) => Node::Split(
                axis,
                parts.into_iter().map(|(s, n)| (s, n.map(f))).collect(),
            ),
        }
    }

    fn kept(&self) -> KeptNode<T>
    where
        T: Clone,
    {
        match self {
            Node::Window(w) => KeptNode::Window(w.shows.clone()),
            Node::Split(axis, parts) => KeptNode::Split {
                axis: *axis,
                parts: parts.iter().map(|(s, n)| (*s, n.kept())).collect(),
            },
        }
    }
}

impl<T> Content<T> {
    fn map<U>(self, f: impl Fn(T) -> U) -> Content<U> {
        let tabs = self
            .tabs
            .into_iter()
            .map(|t| ContentTab {
                id: t.id,
                root: t.root.map(&f),
                focus: t.focus,
                previous: t.previous,
            })
            .collect();
        Content {
            tabs,
            current: self.current,
            last: self.last,
            next: self.next,
        }
    }
}

impl<T: Clone + PartialEq> Content<Option<T>> {
    /// The content with every window that shows nothing closed, as one
    /// whose thing ended is.
    fn closed_around(mut self) -> Content<T> {
        self.close_showing(Option::is_none);
        self.map(|t| t.expect("a window showing nothing is closed"))
    }

    fn node_from<U>(
        &mut self,
        node: KeptNode<U>,
        back: &impl Fn(U) -> Option<T>,
    ) -> Option<Node<Option<T>>> {
        match node {
            KeptNode::Window(thing) => Some(Node::Window(self.window(back(thing)))),
            KeptNode::Split { axis, parts } => {
                let parts: Vec<_> = parts
                    .into_iter()
                    .filter_map(|(size, n)| {
                        let size = if size.is_finite() && size > 0.0 {
                            size
                        } else {
                            MIN_SIZE
                        };
                        Some((size, self.node_from(n, back)?))
                    })
                    .collect();
                (!parts.is_empty()).then(|| Node::Split(axis, parts).normalized())
            }
        }
    }
}

impl<T: Clone + PartialEq> Content<T> {
    /// The content as it is kept for the next run, each window's thing as
    /// `keep` names it. A window `keep` names nothing for is left out as if
    /// what it showed had ended.
    pub fn kept<U: Clone + PartialEq>(&self, keep: impl Fn(&T) -> Option<U>) -> Kept<U> {
        let named = self.clone().map(|t| keep(&t)).closed_around();
        Kept {
            tabs: named
                .tabs
                .iter()
                .map(|t| KeptTab {
                    root: t.root.kept(),
                    focus: t
                        .windows()
                        .iter()
                        .position(|w| w.id == t.focus)
                        .unwrap_or_default(),
                })
                .collect(),
            current: named.current,
        }
    }

    /// The content `kept` for the last run, each window's thing as `back`
    /// gives it. A window `back` gives nothing for is closed as if what it
    /// showed had ended, and the layout closes around it.
    pub fn from_kept<U>(kept: Kept<U>, back: impl Fn(U) -> Option<T>) -> Self {
        let mut c = Content::<Option<T>>::default();
        for tab in kept.tabs {
            let Some(root) = c.node_from(tab.root, &back) else {
                continue;
            };
            let mut windows = Vec::new();
            root.windows(&mut windows);
            let focus = windows[tab.focus.min(windows.len() - 1)].id;
            let id = c.next;
            c.next += 1;
            c.tabs.push(ContentTab {
                id,
                root,
                focus,
                previous: None,
            });
        }
        c.current = cursor::within(kept.current, c.tabs.len());
        c.closed_around()
    }
}
