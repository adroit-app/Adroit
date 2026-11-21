//! The tree's files copied, cut and pasted through the system clipboard,
//! and trashed, as fern does: the clipboard carries the paths, and the
//! tree remembers whether those it put there were cut.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use adroit_model::EnvironmentId;

use crate::chrome::status_line;
use crate::effect::Effect;
use crate::overlay::confirm::Ask;
use crate::state::ViewState;
use crate::tree::nav::Item;

/// The paths the tree last put on the clipboard, of `environment`'s work
/// machine, to be moved where they are pasted when `moving`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    pub environment: EnvironmentId,
    pub paths: Vec<PathBuf>,
    pub moving: bool,
}

/// Mark the file or directory `row` is, or unmark it when it is marked.
/// A row outside the files is never marked.
pub(crate) fn mark(state: &mut ViewState, row: &Item) -> Vec<Effect> {
    if matches!(row, Item::File { .. }) {
        let marks = &mut state.tree.marks;
        match marks.iter().position(|m| m == row) {
            Some(at) => {
                marks.remove(at);
            }
            None => marks.push(row.clone()),
        }
    }
    Vec::new()
}

/// What an action on the files takes from `row`: every file marked, those
/// of the environment the first was marked in, or else the file `row` is.
fn targets(state: &ViewState, row: &Item) -> Option<(EnvironmentId, Vec<PathBuf>)> {
    let file = |item: &Item| match item {
        Item::File {
            environment, path, ..
        } => Some((environment.clone(), path.clone())),
        _ => None,
    };
    let Some((environment, _)) = state.tree.marks.first().and_then(file) else {
        return file(row).map(|(environment, path)| (environment, vec![path]));
    };
    let paths = state
        .tree
        .marks
        .iter()
        .filter_map(file)
        .filter(|(e, _)| *e == environment)
        .map(|(_, path)| path)
        .collect();
    Some((environment, paths))
}

/// What is marked, or else the file or directory `row` is, put on the
/// clipboard to be copied, or moved when `moving`; the marks are cleared.
/// A row outside the files with nothing marked puts nothing there.
pub(crate) fn hold(state: &mut ViewState, row: &Item, moving: bool) -> Vec<Effect> {
    let Some((environment, paths)) = targets(state, row) else {
        return Vec::new();
    };
    let name = named(&paths);
    let done = if moving {
        format!("Cut {name}: paste to move there")
    } else {
        format!("Copied {name}")
    };
    state.tree.marks.clear();
    state.tree.clipboard = Some(Held {
        environment: environment.clone(),
        paths: paths.clone(),
        moving,
    });
    vec![Effect::Clip {
        environment,
        paths,
        done,
    }]
}

/// Paste from `row`: the clipboard is read for the directory a new file
/// made from there goes in.
pub(crate) fn paste(row: &Item) -> Vec<Effect> {
    match super::entries_dir(row) {
        Some((environment, worktree, dir)) => vec![Effect::ReadClip {
            environment,
            worktree,
            dir,
        }],
        None => Vec::new(),
    }
}

/// The clipboard read for pasting into `dir`, holding `paths`: copied
/// there, or moved after asking when they are what the tree cut.
pub(crate) fn clipped(
    state: &mut ViewState,
    environment: EnvironmentId,
    worktree: PathBuf,
    dir: PathBuf,
    paths: Vec<PathBuf>,
) -> Vec<Effect> {
    if paths.is_empty() {
        return status_line::notice(
            &mut state.chrome.status,
            "The clipboard holds no files to paste".into(),
        );
    }
    let cut = state.tree.clipboard.as_ref().is_some_and(|held| {
        held.moving
            && held.environment == environment
            && held.paths.iter().collect::<BTreeSet<_>>() == paths.iter().collect()
    });
    if cut {
        state.overlays.confirm = Some(Ask::MoveEntries {
            environment,
            worktree,
            dir,
            sources: paths,
        });
        return Vec::new();
    }
    vec![Effect::PasteEntries {
        environment,
        worktree,
        dir,
        sources: paths,
        moving: false,
    }]
}

/// The work host pasted `sources` into `dir`: what was moved is no longer
/// held, and the first pasted is selected once it is listed.
pub(crate) fn pasted(
    state: &mut ViewState,
    environment: &EnvironmentId,
    worktree: &Path,
    dir: &Path,
    sources: &[PathBuf],
    moving: bool,
) -> Vec<Effect> {
    if moving {
        state.tree.clipboard = None;
    }
    let verb = if moving { "Moved" } else { "Pasted" };
    let told = format!("{verb} {}", named(sources));
    let mut effects = status_line::notice(&mut state.chrome.status, told);
    if let Some(name) = sources.first().and_then(|s| s.file_name()) {
        effects.extend(super::made(
            &mut state.tree,
            environment,
            worktree,
            &dir.join(name),
            false,
        ));
    }
    effects
}

/// What is marked, or else the file or directory `row` is, to be put in
/// the trash once the user agrees to it.
pub(crate) fn trash(state: &mut ViewState, row: &Item) -> Vec<Effect> {
    if let Some((environment, paths)) = targets(state, row) {
        state.overlays.confirm = Some(Ask::TrashEntries { environment, paths });
    }
    Vec::new()
}

/// `paths` as the status line says them: the name of the one, or a count.
fn named(paths: &[PathBuf]) -> String {
    match paths {
        [one] => shown(one),
        many => format!("{} items", many.len()),
    }
}

/// The paths a clipboard's text names, one a line, as a file manager
/// copies them as paths or as `file:` URLs; none unless every line is
/// one, so a clipboard holding other text pastes nothing.
pub fn paths_in(text: &str) -> Vec<PathBuf> {
    let paths: Option<Vec<PathBuf>> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let path = match line.strip_prefix("file://") {
                Some(url) => {
                    PathBuf::from(unescaped(url.strip_prefix("localhost").unwrap_or(url))?)
                }
                None => PathBuf::from(line),
            };
            (path.starts_with("/") || path.is_absolute()).then_some(path)
        })
        .collect();
    paths.unwrap_or_default()
}

/// `text` with its `%XX` escapes decoded, as a URL's path writes bytes
/// outside ASCII; `None` for a broken escape or bytes that are not UTF-8.
fn unescaped(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// What a path is called in what the status line says: its name.
fn shown(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_of_the_clipboard_is_a_path_or_a_file_url() {
        assert_eq!(
            paths_in("/w/a.rs\n\nfile:///w/%E6%97%A5%E6%9C%AC%20b.md\nfile://localhost/w/c\n"),
            [
                PathBuf::from("/w/a.rs"),
                PathBuf::from("/w/日本 b.md"),
                PathBuf::from("/w/c"),
            ]
        );
    }

    #[test]
    fn a_clipboard_holding_other_text_names_no_paths() {
        for text in [
            "hello",
            "/w/a.rs\nrelative.rs",
            "file:///w/%E6",
            "file:///w/%zz",
            "",
        ] {
            assert_eq!(paths_in(text), Vec::<PathBuf>::new(), "{text}");
        }
    }
}
