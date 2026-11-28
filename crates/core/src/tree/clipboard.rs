//! The tree's files copied, cut and pasted through the system clipboard,
//! and trashed, as fern does: the clipboard carries the paths, and the
//! tree remembers whether those it put there were cut.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use adroit_model::EnvironmentId;
use adroit_protocol::PasteEntry;

use crate::chrome::status_line;
use crate::effect::Effect;
use crate::overlay::confirm::Ask;
use crate::state::ViewState;
use crate::tree::nav::Item;

/// The paths the tree last put on the clipboard, of `environment`'s work
/// machine, and of them those cut and not moved yet, which are moved where
/// they are pasted; none for a copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    pub environment: EnvironmentId,
    pub paths: Vec<PathBuf>,
    pub unmoved: Vec<PathBuf>,
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
        unmoved: if moving { paths.clone() } else { Vec::new() },
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
    let held = state
        .tree
        .clipboard
        .as_ref()
        .filter(|held| held.paths.iter().collect::<BTreeSet<_>>() == paths.iter().collect());
    // The paths name files of the environment they were copied in; on another
    // work machine the same paths are other files, or none.
    if held.is_some_and(|held| held.environment != environment) {
        return status_line::notice(
            &mut state.chrome.status,
            "The clipboard holds files of another environment, which cannot be pasted here".into(),
        );
    }
    if let Some(held) = held.filter(|held| !held.unmoved.is_empty()) {
        state.overlays.confirm = Some(Ask::MoveEntries {
            environment,
            worktree,
            dir,
            sources: held.unmoved.clone(),
        });
        return Vec::new();
    }
    vec![Effect::PasteEntries {
        environment,
        worktree,
        entries: into(&dir, &paths),
        moving: false,
    }]
}

/// Each of `sources` pasted into `dir` by its own name.
pub(crate) fn into(dir: &Path, sources: &[PathBuf]) -> Vec<PasteEntry> {
    sources
        .iter()
        .filter_map(|source| {
            Some(PasteEntry {
                source: source.clone(),
                target: dir.join(source.file_name()?),
                replace: false,
            })
        })
        .collect()
}

/// A paste whose target was there already, waiting to be asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clash {
    pub environment: EnvironmentId,
    pub worktree: PathBuf,
    pub entry: PasteEntry,
    pub moving: bool,
}

/// The work host pasted the entries `pasted`, and left alone the entries
/// `taken`, which are asked about one by one; the first pasted is selected
/// once it is listed. What was moved is cut no longer, and only that: what
/// failed to move, or was cut since, stays cut.
pub(crate) fn pasted(
    state: &mut ViewState,
    environment: EnvironmentId,
    worktree: PathBuf,
    pasted: Vec<PasteEntry>,
    taken: Vec<PasteEntry>,
    moving: bool,
) -> Vec<Effect> {
    if moving
        && let Some(held) = state.tree.clipboard.as_mut()
        && held.environment == environment
        && !held.unmoved.is_empty()
    {
        held.unmoved
            .retain(|path| !pasted.iter().any(|entry| entry.source == *path));
        if held.unmoved.is_empty() {
            state.tree.clipboard = None;
        }
    }
    let pasted: Vec<PathBuf> = pasted.into_iter().map(|entry| entry.target).collect();
    state
        .tree
        .clashes
        .extend(taken.into_iter().map(|entry| Clash {
            environment: environment.clone(),
            worktree: worktree.clone(),
            entry,
            moving,
        }));
    let Some(first) = pasted.first() else {
        return Vec::new();
    };
    let verb = if moving { "Moved" } else { "Pasted" };
    let told = format!("{verb} {}", named(&pasted));
    let mut effects = status_line::notice(&mut state.chrome.status, told);
    effects.extend(super::made(
        &mut state.tree,
        &environment,
        &worktree,
        first,
        false,
    ));
    effects
}

/// The next target taken is asked about once nothing else is being asked
/// or named.
pub(crate) fn ask_next_clash(state: &mut ViewState) {
    if state.overlays.confirm.is_some() || state.overlays.naming.is_some() {
        return;
    }
    if let Some(clash) = state.tree.clashes.pop_front() {
        state.overlays.confirm = Some(Ask::Clash(clash));
    }
}

/// The target taken being asked about is overwritten: what is there goes
/// to the trash and the source takes its place.
pub(crate) fn overwrite(state: &mut ViewState) -> Vec<Effect> {
    match state.overlays.confirm.take() {
        Some(Ask::Clash(clash)) => vec![Effect::PasteEntries {
            environment: clash.environment,
            worktree: clash.worktree,
            entries: vec![PasteEntry {
                replace: true,
                ..clash.entry
            }],
            moving: clash.moving,
        }],
        other => {
            state.overlays.confirm = other;
            Vec::new()
        }
    }
}

/// The target taken being asked about is named anew in place, its name
/// typed over; the source is pasted by the name typed.
pub(crate) fn rename(state: &mut ViewState) -> Vec<Effect> {
    match state.overlays.confirm.take() {
        Some(Ask::Clash(clash)) => {
            let naming = crate::overlay::naming::Naming::pasting(clash);
            state.tree.folds.set(&naming.under(), true);
            state.overlays.naming = Some(naming);
        }
        other => state.overlays.confirm = other,
    }
    Vec::new()
}

/// What is marked, or else the file or directory `row` is, to be put in
/// the trash once the user agrees to it.
pub(crate) fn trash(state: &mut ViewState, row: &Item) -> Vec<Effect> {
    if let Some((environment, paths)) = targets(state, row) {
        state.overlays.confirm = Some(Ask::TrashEntries { environment, paths });
    }
    Vec::new()
}

/// What is marked, or else the file or directory `row` is, each opened in
/// the app the system opens it with; the marks stay, as nothing moved.
pub(crate) fn open_with_system(state: &ViewState, row: &Item) -> Vec<Effect> {
    let Some((environment, paths)) = targets(state, row) else {
        return Vec::new();
    };
    paths
        .into_iter()
        .map(|path| Effect::Open {
            environment: environment.clone(),
            target: path.display().to_string(),
        })
        .collect()
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
