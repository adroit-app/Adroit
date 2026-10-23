//! What a thing a window shows is called on its tab and its window.

use std::path::Path;

use adroit_model::{TerminalSession, Tree};

use super::Tab;
use crate::tree::nav;

/// The last part of `path`, as a file is called.
pub fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// What `thing` is called in a tab and on its windows.
pub fn thing_name(tree: &Tree, terminals: &[TerminalSession], thing: &Tab) -> String {
    match thing {
        Tab::Session(id) => nav::find(tree, id)
            .map(|(_, s)| s.name.clone())
            .unwrap_or_else(|| id.clone()),
        Tab::Terminal(id) => terminals
            .iter()
            .find(|s| s.id == *id)
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        Tab::Draft { .. } => "new session".into(),
        Tab::Viewer(path) => file_name(path),
    }
}

#[cfg(test)]
mod tests {
    use adroit_model::testing;

    use super::*;

    #[test]
    fn a_thing_is_called_by_its_session_s_shell_s_or_file_s_name() {
        let tree = testing::tree()
            .session(testing::session("a").name("fix").cwd("/r"))
            .build();
        let shell = TerminalSession {
            id: 7,
            name: "zsh".into(),
            ..Default::default()
        };
        // (thing, its name)
        let cases = [
            (Tab::Session("a".into()), "fix"),
            (Tab::Session("gone".into()), "gone"),
            (Tab::Terminal(7), "zsh"),
            (Tab::Terminal(8), ""),
            (Tab::draft("/r"), "new session"),
            (Tab::Viewer("/r/src/a.rs".into()), "a.rs"),
        ];
        for (thing, want) in cases {
            assert_eq!(
                thing_name(&tree, std::slice::from_ref(&shell), &thing),
                want,
                "{thing:?}"
            );
        }
    }
}
