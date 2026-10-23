//! A directory tree over several roots, moved through like fern: `j`/`k`
//! move, `l` opens, `h` closes or goes to the parent, `!` shows the hidden.
//!
//! Directories are read through a function the caller passes, so the tree
//! is tested without a disk and drawn from what the work host listed.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// One line of the tree. Its identity is the root it is listed under and
/// its path: one directory can be listed under two roots, open in one and
/// shut in the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub root: PathBuf,
    pub path: PathBuf,
    pub depth: usize,
}

impl Row {
    fn is(&self, root: &Path, path: &Path) -> bool {
        self.root == root && self.path == path
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Filer {
    roots: Vec<PathBuf>,
    expanded: HashSet<(PathBuf, PathBuf)>,
    show_hidden: bool,
    cursor: Option<Row>,
}

impl Filer {
    /// A tree over `roots`, each once. A root inside another is kept beside
    /// it, as a shorter way to the same directories; the two open apart. The
    /// cursor starts on the first root.
    pub fn new(given: impl IntoIterator<Item = impl Into<PathBuf>>) -> Self {
        let mut filer = Self::default();
        filer.add_roots(given);
        filer
    }

    /// List `more` roots after those already listed, each once, leaving what
    /// is open and the cursor as they are.
    pub fn add_roots(&mut self, more: impl IntoIterator<Item = impl Into<PathBuf>>) {
        for root in more.into_iter().map(Into::into) {
            if !self.roots.contains(&root) {
                self.roots.push(root);
            }
        }
        if self.cursor.is_none() {
            self.cursor = self.roots.first().map(|r| Row {
                root: r.clone(),
                path: r.clone(),
                depth: 0,
            });
        }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn show_hidden(&self) -> bool {
        self.show_hidden
    }

    pub fn cursor(&self) -> Option<&Row> {
        self.cursor.as_ref()
    }

    /// The tree without its cursor: all [`Filer::rows`] depends on.
    pub fn layout(&self) -> Self {
        Self {
            cursor: None,
            ..self.clone()
        }
    }

    /// Every directory open, under whichever root.
    pub fn expanded(&self) -> impl Iterator<Item = &Path> {
        self.expanded.iter().map(|(_, path)| path.as_path())
    }

    pub fn is_expanded(&self, row: &Row) -> bool {
        self.expanded
            .contains(&(row.root.clone(), row.path.clone()))
    }

    /// Every row, top to bottom: each root, then depth first the
    /// subdirectories of the expanded directories, sorted by name. `read`
    /// answers a directory's child directories.
    pub fn rows(&self, read: impl Fn(&Path) -> Vec<PathBuf>) -> Vec<Row> {
        let mut out = Vec::new();
        for root in &self.roots {
            self.collect(root, root, 0, &read, &mut out);
        }
        out
    }

    fn collect(
        &self,
        root: &Path,
        dir: &Path,
        depth: usize,
        read: &impl Fn(&Path) -> Vec<PathBuf>,
        out: &mut Vec<Row>,
    ) {
        out.push(Row {
            root: root.to_path_buf(),
            path: dir.to_path_buf(),
            depth,
        });
        if !self
            .expanded
            .contains(&(root.to_path_buf(), dir.to_path_buf()))
        {
            return;
        }
        for child in self.children(dir, read) {
            self.collect(root, &child, depth + 1, read, out);
        }
    }

    /// The child directories of `dir` as they are listed: sorted by name,
    /// the hidden left out unless shown.
    fn children(&self, dir: &Path, read: &impl Fn(&Path) -> Vec<PathBuf>) -> Vec<PathBuf> {
        let mut children: Vec<PathBuf> = read(dir)
            .into_iter()
            .filter(|c| self.show_hidden || !is_hidden(c))
            .collect();
        children.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
        children
    }

    /// Keep the cursor on a listed row after the rows were rebuilt: on its
    /// own row while it is listed, else on its nearest listed ancestor.
    pub fn settle(&mut self, rows: &[Row]) {
        let Some(cur) = &self.cursor else {
            return;
        };
        let found = cur
            .path
            .ancestors()
            .take_while(|p| p.starts_with(&cur.root))
            .find_map(|p| rows.iter().find(|r| r.is(&cur.root, p)));
        self.cursor = found.or(rows.first()).cloned();
    }

    /// The row below the cursor; the cursor stays on the last row.
    pub fn down(&mut self, rows: &[Row]) {
        self.settle_or_first(rows, |at| (at + 1).min(rows.len().saturating_sub(1)));
    }

    /// The row above the cursor; the cursor stays on the first row.
    pub fn up(&mut self, rows: &[Row]) {
        self.settle_or_first(rows, |at| at.saturating_sub(1));
    }

    fn settle_or_first(&mut self, rows: &[Row], to: impl Fn(usize) -> usize) {
        if self.cursor.is_none() {
            self.cursor = rows.first().cloned();
            return;
        }
        self.settle(rows);
        if let Some(at) = self.position(rows) {
            self.cursor = Some(rows[to(at)].clone());
        }
    }

    fn position(&self, rows: &[Row]) -> Option<usize> {
        let cur = self.cursor.as_ref()?;
        rows.iter().position(|r| r.is(&cur.root, &cur.path))
    }

    /// Open the directory under the cursor and move onto its first child,
    /// when it has one.
    pub fn open(&mut self, read: impl Fn(&Path) -> Vec<PathBuf>) {
        let Some(cur) = self.cursor.clone() else {
            return;
        };
        self.expanded.insert((cur.root.clone(), cur.path.clone()));
        // Only the directory opened is read: the whole tree would read every
        // open directory again, and the window reads it once more to draw.
        if let Some(first) = self.children(&cur.path, &read).into_iter().next() {
            self.cursor = Some(Row {
                root: cur.root,
                path: first,
                depth: cur.depth + 1,
            });
        }
    }

    /// Close the directory under the cursor when it is open, else move to
    /// its parent; a shut root stays where it is.
    pub fn close(&mut self, rows: &[Row]) {
        let Some(cur) = self.cursor.clone() else {
            return;
        };
        if self.expanded.remove(&(cur.root.clone(), cur.path.clone())) || cur.path == cur.root {
            return;
        }
        if let Some(parent) = cur
            .path
            .parent()
            .and_then(|p| rows.iter().find(|r| r.is(&cur.root, p)))
        {
            self.cursor = Some(parent.clone());
        }
    }

    /// Show or hide the directories whose names start with `.`.
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        if self.show_hidden {
            return;
        }
        // What stays listed is told by the path alone, nothing under a hidden
        // directory being listed, so the disk is not read for it.
        let Some(cur) = self.cursor.as_mut() else {
            return;
        };
        let shown = cur.path.ancestors().find_map(|p| {
            let rel = p.strip_prefix(&cur.root).ok()?;
            (!rel
                .components()
                .any(|c| is_hidden(Path::new(c.as_os_str()))))
            .then(|| (p.to_path_buf(), rel.components().count()))
        });
        if let Some((path, depth)) = shown {
            cur.path = path;
            cur.depth = depth;
        }
    }

    /// The directory under the cursor.
    pub fn chosen(&self) -> Option<PathBuf> {
        self.cursor.as_ref().map(|r| r.path.clone())
    }
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// A disk of directories, each listing its children in no useful order.
    fn disk(tree: &[(&str, &[&str])]) -> impl Fn(&Path) -> Vec<PathBuf> {
        let map: HashMap<PathBuf, Vec<PathBuf>> = tree
            .iter()
            .map(|(dir, kids)| {
                (
                    PathBuf::from(dir),
                    kids.iter().map(|k| Path::new(dir).join(k)).collect(),
                )
            })
            .collect();
        move |p: &Path| map.get(p).cloned().unwrap_or_default()
    }

    fn sample() -> impl Fn(&Path) -> Vec<PathBuf> {
        disk(&[
            ("/h", &["src", ".config", "docs"]),
            ("/h/src", &["b", "a"]),
            ("/h/.config", &["nvim"]),
            ("/g", &["github.com"]),
        ])
    }

    fn lines(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| format!("{}{}", "  ".repeat(r.depth), r.path.display()))
            .collect()
    }

    fn at(filer: &Filer) -> String {
        filer.cursor().unwrap().path.display().to_string()
    }

    #[test]
    fn a_new_tree_lists_each_root_shut_with_the_cursor_on_the_first() {
        let filer = Filer::new(["/h", "/g"]);
        assert_eq!(lines(&filer.rows(sample())), vec!["/h", "/g"]);
        assert_eq!(at(&filer), "/h");
    }

    #[test]
    fn a_repeated_root_is_left_out_and_one_inside_another_kept_beside_it() {
        let filer = Filer::new(["/h", "/h/src", "/g", "/h"]);
        assert_eq!(
            filer.roots(),
            &[PathBuf::from("/h"), "/h/src".into(), "/g".into()]
        );
    }

    #[test]
    fn opening_lists_the_subdirectories_by_name_without_the_hidden_and_steps_into_the_first() {
        let read = sample();
        let mut filer = Filer::new(["/h", "/g"]);
        filer.open(&read);
        assert_eq!(
            lines(&filer.rows(&read)),
            vec!["/h", "  /h/docs", "  /h/src", "/g"]
        );
        assert_eq!(at(&filer), "/h/docs");
    }

    #[test]
    fn roots_added_later_go_after_the_others_and_leave_the_tree_as_it_was() {
        let read = sample();
        let mut filer = Filer::new(["/h"]);
        filer.open(&read);
        filer.add_roots(["/h", "/g"]);
        assert_eq!(
            lines(&filer.rows(&read)),
            vec!["/h", "  /h/docs", "  /h/src", "/g"]
        );
        assert_eq!(at(&filer), "/h/docs");

        let mut empty = Filer::new(Vec::<PathBuf>::new());
        empty.add_roots(["/g"]);
        assert_eq!(at(&empty), "/g");
    }

    #[test]
    fn opening_a_directory_with_nothing_inside_leaves_the_cursor_on_it() {
        let read = sample();
        let mut filer = Filer::new(["/h"]);
        filer.open(&read);
        filer.open(&read);
        assert_eq!(at(&filer), "/h/docs");
        assert!(filer.is_expanded(filer.cursor().unwrap()));
    }

    #[test]
    fn open_directories_are_listed_depth_first_under_their_root() {
        let read = sample();
        let mut filer = Filer::new(["/h", "/g"]);
        filer.open(&read);
        filer.down(&filer.rows(&read));
        filer.open(&read);
        assert_eq!(at(&filer), "/h/src/a");
        assert_eq!(
            lines(&filer.rows(&read)),
            vec![
                "/h",
                "  /h/docs",
                "  /h/src",
                "    /h/src/a",
                "    /h/src/b",
                "/g"
            ]
        );
    }

    #[test]
    fn moving_stops_at_either_end_rather_than_wrapping() {
        let read = sample();
        let mut filer = Filer::new(["/h", "/g"]);
        let rows = filer.rows(&read);
        filer.up(&rows);
        assert_eq!(at(&filer), "/h");
        filer.down(&rows);
        filer.down(&rows);
        assert_eq!(at(&filer), "/g");
    }

    #[test]
    fn moving_without_a_cursor_selects_the_first_row() {
        let read = sample();
        let mut filer = Filer::new(["/h", "/g"]);
        filer.cursor = None;
        filer.down(&filer.rows(&read));
        assert_eq!(at(&filer), "/h");
        filer.cursor = None;
        filer.up(&filer.rows(&read));
        assert_eq!(at(&filer), "/h");
    }

    #[test]
    fn closing_a_shut_directory_moves_to_its_parent_and_closing_an_open_one_shuts_it() {
        let read = sample();
        let mut filer = Filer::new(["/h"]);
        filer.open(&read);
        filer.down(&filer.rows(&read));
        filer.open(&read);
        assert_eq!(at(&filer), "/h/src/a");

        filer.close(&filer.rows(&read));
        assert_eq!(at(&filer), "/h/src");
        filer.close(&filer.rows(&read));
        assert_eq!(at(&filer), "/h/src");
        assert_eq!(
            lines(&filer.rows(&read)),
            vec!["/h", "  /h/docs", "  /h/src"]
        );
        filer.close(&filer.rows(&read));
        filer.close(&filer.rows(&read));
        assert_eq!(at(&filer), "/h");
        assert_eq!(lines(&filer.rows(&read)), vec!["/h"]);
    }

    #[test]
    fn closing_on_a_shut_root_does_nothing() {
        let read = sample();
        let mut filer = Filer::new(["/h", "/g"]);
        let rows = filer.rows(&read);
        filer.down(&rows);
        filer.close(&rows);
        assert_eq!(at(&filer), "/g");
        assert_eq!(filer.rows(&read), rows);
    }

    #[test]
    fn a_directory_listed_under_two_roots_is_opened_under_one_only() {
        let read = disk(&[("/a", &["x"]), ("/b", &["x"]), ("/a/x", &["y"])]);
        let mut filer = Filer::new(["/a", "/b"]);
        filer.expanded.insert(("/b".into(), "/b".into()));
        filer.open(&read);
        let rows = filer.rows(&read);
        assert_eq!(lines(&rows), vec!["/a", "  /a/x", "/b", "  /b/x"]);
        assert_eq!(filer.cursor().unwrap().root, PathBuf::from("/a"));
    }

    #[test]
    fn hidden_directories_are_shown_on_request() {
        let read = sample();
        let mut filer = Filer::new(["/h"]);
        filer.open(&read);
        filer.toggle_hidden();
        assert!(filer.show_hidden());
        assert_eq!(
            lines(&filer.rows(&read)),
            vec!["/h", "  /h/.config", "  /h/docs", "  /h/src"]
        );
    }

    #[test]
    fn hiding_the_row_under_the_cursor_moves_it_to_the_nearest_listed_ancestor() {
        let read = sample();
        let mut filer = Filer::new(["/h"]);
        filer.open(&read);
        filer.toggle_hidden();
        filer.up(&filer.rows(&read));
        assert_eq!(at(&filer), "/h/.config");
        filer.open(&read);
        assert_eq!(at(&filer), "/h/.config/nvim");
        filer.toggle_hidden();
        assert_eq!(at(&filer), "/h");
    }

    #[test]
    fn the_cursor_keeps_its_row_when_rows_appear_above_it() {
        let read = sample();
        let mut filer = Filer::new(["/h"]);
        filer.open(&read);
        filer.down(&filer.rows(&read));
        assert_eq!(at(&filer), "/h/src");
        filer.toggle_hidden();
        assert_eq!(at(&filer), "/h/src");
    }

    #[test]
    fn moving_the_cursor_leaves_the_layout_as_it_was() {
        let read = sample();
        let mut filer = Filer::new(["/h", "/g"]);
        filer.open(&read);
        let before = filer.layout();
        filer.down(&filer.rows(&read));
        assert_eq!(filer.layout(), before);
        assert_eq!(before.rows(&read), filer.rows(&read));
        filer.toggle_hidden();
        assert_ne!(filer.layout(), before);
    }

    #[test]
    fn the_chosen_directory_is_the_one_under_the_cursor() {
        let read = sample();
        let mut filer = Filer::new(["/h"]);
        assert_eq!(filer.chosen(), Some(PathBuf::from("/h")));
        filer.open(&read);
        assert_eq!(filer.chosen(), Some(PathBuf::from("/h/docs")));
        assert_eq!(Filer::new(Vec::<PathBuf>::new()).chosen(), None);
    }
}
