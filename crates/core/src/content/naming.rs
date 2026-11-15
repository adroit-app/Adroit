//! What a thing a window shows is called on its tab and its window.

use std::path::Path;

use adroit_model::{SessionCode, TerminalSession, Tree};

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
        Tab::Session(_, id) => nav::find(tree, id)
            .map(|(_, s)| s.name.clone())
            .unwrap_or_else(|| id.clone()),
        Tab::Terminal(_, id) => terminals
            .iter()
            .find(|s| s.id == *id)
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        Tab::Draft { .. } => "new session".into(),
        Tab::Viewer(_, path) => file_name(path),
        Tab::Drawn { width, height, .. } => format!("Image {width} × {height}"),
    }
}

/// The code the session `thing` shows keeps, for a tab that shows a session
/// that has one.
pub fn thing_code(tree: &Tree, thing: &Tab) -> Option<SessionCode> {
    match thing {
        Tab::Session(_, id) => nav::find(tree, id).and_then(|(_, s)| s.code.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::here;
    use adroit_model::testing;

    use super::*;

    #[test]
    fn a_thing_is_called_by_its_session_s_shell_s_or_file_s_name_and_a_drawn_image_by_its_size() {
        let tree = testing::tree()
            .session(testing::session("a").name("fix").cwd("/r"))
            .build();
        let shell = TerminalSession {
            name: "zsh".into(),
            ..testing::shell(7, "/r")
        };
        // (thing, its name)
        let cases = [
            (Tab::Session(here(), "a".into()), "fix"),
            (Tab::Session(here(), "gone".into()), "gone"),
            (Tab::Terminal(here(), 7), "zsh"),
            (Tab::Terminal(here(), 8), ""),
            (Tab::draft(here(), "/r"), "new session"),
            (Tab::Viewer(here(), "/r/src/a.rs".into()), "a.rs"),
            (
                Tab::Drawn {
                    environment: here(),
                    session: "a".into(),
                    id: 1,
                    width: 64,
                    height: 32,
                },
                "Image 64 × 32",
            ),
        ];
        for (thing, want) in cases {
            assert_eq!(
                thing_name(&tree, std::slice::from_ref(&shell), &thing),
                want,
                "{thing:?}"
            );
        }
    }

    #[test]
    fn a_tab_shows_the_code_of_the_session_it_shows_and_nothing_else_has_one() {
        let tree = testing::tree()
            .session(testing::session("a").code("k7f3").cwd("/r"))
            .session(testing::session("b").cwd("/r"))
            .build();
        let code = |thing| thing_code(&tree, &thing).map(|c| c.to_string());

        assert_eq!(
            code(Tab::Session(here(), "a".into())).as_deref(),
            Some("k7f3")
        );
        assert_eq!(code(Tab::Session(here(), "b".into())), None);
        assert_eq!(code(Tab::Session(here(), "gone".into())), None);
        assert_eq!(code(Tab::Terminal(here(), 7)), None);
    }
}
