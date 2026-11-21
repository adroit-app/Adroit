//! A file or a directory of a worktree being named in the tree, before the
//! work host makes it.

use std::path::{Component, Path, PathBuf};

use adroit_model::EnvironmentId;

use crate::chrome::status_line;
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::nav::{Heading, Item};

/// What a name typed makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Directory,
}

/// A new entry being named, and the name typed for it so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Naming {
    pub environment: EnvironmentId,
    pub worktree: PathBuf,
    /// The directory the name is made in.
    pub dir: PathBuf,
    pub kind: Kind,
    pub text: String,
}

/// What the work host is asked to make of a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Made {
    pub path: PathBuf,
    pub dir: bool,
}

impl Naming {
    /// Start naming an entry of `kind` from the row `row`: in the directory
    /// selected, in the one the file selected is in, or at the top of the
    /// worktree from its Files heading. `None` for a row outside the files.
    pub fn at(row: &Item, kind: Kind) -> Option<Self> {
        let (environment, worktree, dir) = crate::tree::entries_dir(row)?;
        Some(Self {
            environment,
            worktree,
            dir,
            kind,
            text: String::new(),
        })
    }

    /// The row the field is drawn under, first among its entries: the
    /// directory's own, or the Files heading for the top of the worktree.
    pub fn under(&self) -> Item {
        if self.dir == self.worktree {
            Item::Heading(
                self.environment.clone(),
                self.worktree.clone(),
                Heading::Files,
            )
        } else {
            Item::File {
                environment: self.environment.clone(),
                worktree: self.worktree.clone(),
                path: self.dir.clone(),
                dir: true,
            }
        }
    }

    /// What the name typed makes: `None` for no name, and why not for one
    /// that would reach outside the directory it is typed in.
    pub fn made(&self) -> Result<Option<Made>, String> {
        let text = self.text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        let dir = self.kind == Kind::Directory || text.ends_with('/');
        let name = Path::new(text.trim_end_matches('/'));
        let stays = name.components().any(|c| matches!(c, Component::Normal(_)))
            && name
                .components()
                .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
        if !stays {
            return Err(format!(
                "{text} is not a name inside {}",
                self.dir.display()
            ));
        }
        Ok(Some(Made {
            path: self.dir.join(name),
            dir,
        }))
    }
}

/// Start typing the name of a new entry of `kind` from the tree's row
/// `row`, its directory opened to hold the field. A row outside the files
/// names none.
pub(crate) fn start_naming(state: &mut ViewState, row: &Item, kind: Kind) -> Vec<Effect> {
    let Some(naming) = Naming::at(row, kind) else {
        return Vec::new();
    };
    state.tree.folds.set(&naming.under(), true);
    state.overlays.naming = Some(naming);
    Vec::new()
}

pub(crate) fn type_naming(state: &mut ViewState, text: String) -> Vec<Effect> {
    if let Some(naming) = state.overlays.naming.as_mut() {
        naming.text = text;
    }
    Vec::new()
}

/// The field naming a new entry left: the work host is asked to make the
/// name typed, unless it was given up, or says why it cannot be made.
pub(crate) fn end_naming(state: &mut ViewState, commit: bool) -> Vec<Effect> {
    let Some(naming) = state.overlays.naming.take().filter(|_| commit) else {
        return Vec::new();
    };
    match naming.made() {
        Ok(Some(made)) => vec![Effect::MakeEntry {
            environment: naming.environment,
            worktree: naming.worktree,
            path: made.path,
            dir: made.dir,
        }],
        Ok(None) => Vec::new(),
        Err(text) => status_line::notice(&mut state.chrome.status, text),
    }
}

/// A new entry being named goes when the row it is drawn under is no
/// longer shown.
pub(crate) fn keep_naming(state: &mut ViewState) {
    let Some(naming) = &state.overlays.naming else {
        return;
    };
    let shown =
        state.chrome.panels.layout.show_sidebar && state.tree.rows().contains(&naming.under());
    if !shown {
        state.overlays.naming = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn here() -> EnvironmentId {
        EnvironmentId::new("here")
    }

    fn file(path: &str, dir: bool) -> Item {
        Item::File {
            environment: here(),
            worktree: "/r".into(),
            path: path.into(),
            dir,
        }
    }

    fn named(row: &Item, kind: Kind, text: &str) -> Result<Option<Made>, String> {
        let mut naming = Naming::at(row, kind).expect("a row of the files");
        naming.text = text.into();
        naming.made()
    }

    fn made(path: &str, dir: bool) -> Result<Option<Made>, String> {
        Ok(Some(Made {
            path: path.into(),
            dir,
        }))
    }

    #[test]
    fn a_name_is_made_in_the_directory_selected_or_the_one_the_file_selected_is_in() {
        let heading = Item::Heading(here(), "/r".into(), Heading::Files);
        let cases = [
            (heading.clone(), "/r", heading.clone()),
            (file("/r/src", true), "/r/src", file("/r/src", true)),
            (file("/r/src/lib.rs", false), "/r/src", file("/r/src", true)),
            (file("/r/README.md", false), "/r", heading),
        ];
        for (row, dir, under) in cases {
            let naming = Naming::at(&row, Kind::File).expect("a row of the files");
            assert_eq!(naming.dir, PathBuf::from(dir), "{row:?}");
            assert_eq!(naming.under(), under, "{row:?}");
            assert_eq!(naming.text, "");
        }
    }

    #[test]
    fn only_the_rows_of_the_files_name_one() {
        for row in [
            Item::Heading(here(), "/r".into(), Heading::Sessions),
            Item::Worktree(here(), "/r".into()),
            Item::Session(here(), "a".into()),
        ] {
            assert_eq!(Naming::at(&row, Kind::File), None, "{row:?}");
        }
    }

    #[test]
    fn a_name_with_slashes_reaches_the_directories_it_passes_through() {
        let src = file("/r/src", true);
        assert_eq!(
            named(&src, Kind::File, "lib.rs"),
            made("/r/src/lib.rs", false)
        );
        assert_eq!(
            named(&src, Kind::File, " a/b/c.rs "),
            made("/r/src/a/b/c.rs", false)
        );
        assert_eq!(
            named(&src, Kind::Directory, "a/b"),
            made("/r/src/a/b", true)
        );
        // A name ending in `/` is a directory, whichever was asked for.
        assert_eq!(named(&src, Kind::File, "a/b/"), made("/r/src/a/b", true));
    }

    #[test]
    fn no_name_makes_nothing() {
        let src = file("/r/src", true);
        assert_eq!(named(&src, Kind::File, ""), Ok(None));
        assert_eq!(named(&src, Kind::Directory, "   "), Ok(None));
    }

    #[test]
    fn a_name_reaching_outside_its_directory_is_refused() {
        let src = file("/r/src", true);
        for text in ["../x", "a/../../x", "/etc/passwd", "/", "./"] {
            assert!(named(&src, Kind::File, text).is_err(), "{text}");
        }
    }
}
