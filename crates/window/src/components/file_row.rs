use std::path::Path;

use dioxus::prelude::*;

use super::{FileGlyph, ListRow};
use crate::file_icons;

/// How far a file tree's rows step in for each level.
pub const FILE_INDENT: usize = 14;

/// What a file row looks like: a directory, one standing back as hidden or
/// ignored ones do, the one the focused window shows, one a Neovim has
/// open.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileLook {
    pub dir: bool,
    pub back: bool,
    pub shown: bool,
    pub editing: bool,
}

impl FileLook {
    fn kind(self) -> String {
        [
            Some("file"),
            self.dir.then_some("dir"),
            self.back.then_some("back"),
            self.shown.then_some("shown"),
            self.editing.then_some("editing"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
    }
}

/// The name a file row gives `path`: `label` when one is given, or else its
/// file name, a directory's with a `/` after it.
fn row_name(path: &Path, dir: bool, label: Option<&str>) -> String {
    if let Some(label) = label {
        return label.to_string();
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if dir { format!("{name}/") } else { name }
}

/// A file or a directory of a file tree, the tree's files and the picker's
/// directories alike: stepped in `depth` levels, its Nerd Font icon in the
/// colour vim-glyph-palette gives it, open or shut, and its name. What
/// follows the name, such as the tabs showing it or its version control
/// mark, comes in `children`.
#[component]
pub fn FileRow(
    path: std::path::PathBuf,
    depth: usize,
    look: FileLook,
    #[props(default)] open: bool,
    #[props(default)] label: Option<String>,
    #[props(default)] selected: bool,
    #[props(default)] title: Option<String>,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
    #[props(default)] oncontextmenu: Option<EventHandler<MouseEvent>>,
    children: Element,
) -> Element {
    let kind = if look.dir {
        file_icons::Kind::Directory { open }
    } else {
        file_icons::Kind::File
    };
    let icon = file_icons::find(&path, kind);
    let colour = icon
        .colour
        .map(|c| format!("color:{}", c.css()))
        .unwrap_or_default();
    let name = row_name(&path, look.dir, label.as_deref());
    rsx! {
        ListRow {
            kind: look.kind(),
            selected,
            style: "padding-left:{8 + FILE_INDENT * depth}px",
            title,
            onclick: move |e| {
                if let Some(onclick) = onclick {
                    onclick.call(e);
                }
            },
            oncontextmenu: move |e| {
                if let Some(oncontextmenu) = oncontextmenu {
                    oncontextmenu.call(e);
                }
            },
            FileGlyph { glyph: icon.glyph, style: colour }
            span { class: "list-row-name", "{name}" }
            {children}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_row_is_classed_by_every_look_it_has() {
        assert_eq!(FileLook::default().kind(), "file");
        let all = FileLook {
            dir: true,
            back: true,
            shown: true,
            editing: true,
        };
        assert_eq!(all.kind(), "file dir back shown editing");
    }

    #[test]
    fn a_directory_is_named_with_a_slash_after_it_unless_it_is_labelled() {
        assert_eq!(row_name(Path::new("/r/src"), true, None), "src/");
        assert_eq!(row_name(Path::new("/r/a.rs"), false, None), "a.rs");
        assert_eq!(row_name(Path::new("/Users/me"), true, Some("~")), "~");
    }
}
