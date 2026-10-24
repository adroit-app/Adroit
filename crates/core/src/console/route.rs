//! Where a key, a text typed or a paste goes, by the area that has the keys
//! and what the console shows there, and what the footer says of it.

use adroit_model::Key;

use crate::content::{Area, Tab};
use crate::keymap::{Chord, Context};

/// Whether every key but the window's own goes to the program in the
/// console: a shell's from the console, and a session waiting on a choice
/// from the prompt as well, so its choices move without leaving the prompt.
pub fn passes(area: Area, session: bool, waits: bool) -> bool {
    match area {
        Area::Console => !session || waits,
        Area::Prompt => session && waits,
        Area::Sidebar | Area::Details => false,
    }
}

/// Where typed or pasted text goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typed {
    Prompt,
    /// The program in the console.
    Program,
    Nowhere,
}

/// Where text typed or pasted in `area` goes: text for a `session` is
/// written in the prompt, but while the console passes keys `direct`ly or
/// the session `waits` on an answer.
pub fn typed_into(area: Area, session: bool, direct: bool, waits: bool) -> Typed {
    match area {
        Area::Console if session && !direct && !waits => Typed::Prompt,
        Area::Console => Typed::Program,
        Area::Prompt if passes(area, session, waits) => Typed::Program,
        Area::Prompt => Typed::Prompt,
        Area::Sidebar | Area::Details => Typed::Nowhere,
    }
}

/// Whether a key that ends no binding starts reading the console. Read
/// already, the key broke off a `global` sequence, which is not the reader's:
/// `Ctrl+q s` would split its window.
pub fn starts_reading(area: Area, session: bool, passing: bool, reading: bool) -> bool {
    area == Area::Console && session && !passing && !reading
}

/// Whether `key` is one of the window's own wherever the keys are: held with
/// Command or with `Mod`.
pub fn app_key(key: &Key) -> bool {
    key.meta || Chord::held_mod(key)
}

/// The session an empty prompt sent passes Ctrl+Enter to: the one whose tab
/// it was written for; a draft or a shell gets nothing from an empty one.
pub fn passes_enter(tab: Option<&Tab>, text: &str) -> Option<String> {
    match tab {
        Some(Tab::Session(id)) if text.trim().is_empty() => Some(id.clone()),
        _ => None,
    }
}

/// The areas whose bindings a key is looked up in, nearest first: the
/// window's own alone while the keys are `passing` on or a `windowing`
/// sequence goes on, and the prompt's alone while Neovim is inserting, so a
/// global key types. A draft's bindings come before all of them.
pub fn contexts(
    area: Area,
    shown: Option<&Tab>,
    passing: bool,
    windowing: bool,
    inserting: bool,
) -> Vec<Context> {
    let here: &[Context] = match area {
        _ if passing || windowing => &[Context::Global],
        Area::Prompt if inserting => &[Context::Prompt],
        _ => return drafting(shown, &help_contexts(area, shown)),
    };
    drafting(shown, here)
}

/// `here` with the filer's bindings first while the tree's selection is
/// a file or a directory and the tree has the keys.
pub fn filing(mut here: Vec<Context>, area: Area, on_file: bool) -> Vec<Context> {
    if area == Area::Sidebar && on_file {
        let at = here.iter().position(|c| *c == Context::Tree).unwrap_or(0);
        here.insert(at, Context::Filer);
    }
    here
}

/// The areas whose bindings the keys help shows for `area`, nearest first.
pub fn help_contexts(area: Area, shown: Option<&Tab>) -> Vec<Context> {
    let here: &[Context] = match area {
        Area::Console if matches!(shown, Some(Tab::Viewer(_))) => {
            &[Context::Viewer, Context::Global]
        }
        _ => footer_contexts(area),
    };
    here.to_vec()
}

/// The areas whose keys the footer names for `area`, nearest first.
pub fn footer_contexts(area: Area) -> &'static [Context] {
    match area {
        Area::Sidebar => &[Context::Tree, Context::Global],
        Area::Details => &[Context::Details, Context::Tree, Context::Global],
        Area::Console => &[Context::Console, Context::Global],
        Area::Prompt => &[Context::Prompt, Context::Global],
    }
}

/// What the footer says the keys do, and the class it is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footing<'a> {
    /// The picker is open, which holds every key.
    pub picking: bool,
    pub area: Area,
    pub shown: Option<&'a Tab>,
    /// The console passes every key on.
    pub direct: bool,
    /// The session shown waits on an answer, or shows a dialog.
    pub answering: bool,
    /// The reader's mode, while the console is read.
    pub reading: Option<&'a str>,
    /// The prompt's mode, as its class and its word.
    pub prompt: (&'static str, &'static str),
}

impl Footing<'_> {
    pub fn mode(&self) -> (String, &'static str) {
        let (word, class) = match self.area {
            _ if self.picking => ("PICK", "normal"),
            Area::Console if self.direct => ("DIRECT", "pass"),
            Area::Console if self.answering || matches!(self.shown, Some(Tab::Terminal(_))) => {
                ("PASS", "pass")
            }
            Area::Console if let Some(mode) = self.reading => {
                if mode.starts_with("visual") || mode.starts_with("select") {
                    ("VISUAL", "normal")
                } else {
                    ("READ", "normal")
                }
            }
            Area::Console if matches!(self.shown, Some(Tab::Viewer(_))) => ("VIEWER", "normal"),
            Area::Console => ("CONSOLE", "normal"),
            Area::Prompt if self.answering => ("PASS", "pass"),
            Area::Prompt => (self.prompt.1, self.prompt.0),
            Area::Sidebar => ("TREE", "normal"),
            Area::Details => ("DETAILS", "normal"),
        };
        (word.to_string(), class)
    }
}

fn drafting(shown: Option<&Tab>, here: &[Context]) -> Vec<Context> {
    crate::content::draft::can_choose(shown)
        .then_some(Context::Draft)
        .into_iter()
        .chain(here.iter().copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_prompt_passes_ctrl_enter_to_its_session_alone() {
        let session = Tab::Session("s1".into());
        assert_eq!(passes_enter(Some(&session), "  \n"), Some("s1".to_string()));
        assert_eq!(passes_enter(Some(&session), "hi"), None);
        let draft = Tab::draft("/w");
        assert_eq!(passes_enter(Some(&draft), ""), None);
        assert_eq!(passes_enter(None, ""), None);
    }

    #[test]
    fn an_unbound_key_in_a_session_s_console_starts_reading_unless_it_is_read_already() {
        // (area, a session shown, its keys passed on, reading) -> reading starts
        let table = [
            (Area::Console, true, false, false, true),
            (Area::Console, true, false, true, false),
            (Area::Console, true, true, false, false),
            (Area::Console, false, false, false, false),
            (Area::Prompt, true, false, false, false),
            (Area::Sidebar, true, false, false, false),
        ];
        for (area, session, passing, reading, want) in table {
            assert_eq!(
                starts_reading(area, session, passing, reading),
                want,
                "{area:?} {session} {passing} {reading}"
            );
        }
    }

    #[test]
    fn a_shell_takes_the_console_s_keys_and_a_waiting_session_the_prompt_s_too() {
        // (area, a session shown, it waits) -> the keys go to the console's program
        let table = [
            (Area::Console, false, false, true),
            (Area::Console, true, false, false),
            (Area::Console, true, true, true),
            (Area::Prompt, false, false, false),
            (Area::Prompt, true, false, false),
            (Area::Prompt, true, true, true),
            (Area::Sidebar, true, true, false),
            (Area::Details, true, true, false),
        ];
        for (area, session, waits, want) in table {
            assert_eq!(
                passes(area, session, waits),
                want,
                "{area:?} {session} {waits}"
            );
        }
    }

    #[test]
    fn text_for_a_session_is_written_in_the_prompt_and_the_rest_goes_to_the_console_s_program() {
        use Typed::*;
        // (area, a session shown, keys passed directly, it waits) -> where text goes
        let table = [
            (Area::Prompt, true, false, false, Prompt),
            (Area::Prompt, false, false, false, Prompt),
            (Area::Prompt, true, false, true, Program),
            (Area::Console, true, false, false, Prompt),
            (Area::Console, true, true, false, Program),
            (Area::Console, true, false, true, Program),
            (Area::Console, false, false, false, Program),
            (Area::Sidebar, true, false, false, Nowhere),
            (Area::Details, false, true, true, Nowhere),
        ];
        for (area, session, direct, waits, want) in table {
            assert_eq!(
                typed_into(area, session, direct, waits),
                want,
                "{area:?} {session} {direct} {waits}"
            );
        }
    }

    #[test]
    fn a_key_is_looked_up_where_the_keys_are_and_only_globally_while_they_pass_on() {
        use Context::*;
        let session = Tab::Session("s".into());
        let viewer = Tab::Viewer("/a.png".into());
        let draft = Tab::draft("/w");
        // (area, shown, passing, windowing, inserting) -> contexts
        type Case<'a> = (Area, Option<&'a Tab>, bool, bool, bool, Vec<Context>);
        let table: [Case; 10] = [
            (
                Area::Sidebar,
                Some(&session),
                false,
                false,
                false,
                vec![Tree, Global],
            ),
            (
                Area::Details,
                Some(&session),
                false,
                false,
                false,
                vec![Details, Tree, Global],
            ),
            (
                Area::Console,
                Some(&session),
                false,
                false,
                false,
                vec![Console, Global],
            ),
            (
                Area::Console,
                Some(&viewer),
                false,
                false,
                false,
                vec![Viewer, Global],
            ),
            (
                Area::Console,
                Some(&session),
                true,
                false,
                false,
                vec![Global],
            ),
            (
                Area::Console,
                Some(&session),
                false,
                true,
                false,
                vec![Global],
            ),
            (
                Area::Prompt,
                Some(&session),
                false,
                false,
                false,
                vec![Prompt, Global],
            ),
            (
                Area::Prompt,
                Some(&session),
                false,
                false,
                true,
                vec![Prompt],
            ),
            (
                Area::Prompt,
                Some(&draft),
                false,
                false,
                true,
                vec![Draft, Prompt],
            ),
            (
                Area::Console,
                None,
                false,
                false,
                false,
                vec![Draft, Console, Global],
            ),
        ];
        for (area, shown, passing, windowing, inserting, want) in table {
            assert_eq!(
                contexts(area, shown, passing, windowing, inserting),
                want,
                "{area:?} {shown:?} {passing} {windowing} {inserting}"
            );
        }
        assert_eq!(help_contexts(Area::Console, None), vec![Console, Global]);
    }

    #[test]
    fn the_footer_names_what_the_keys_do_where_they_are() {
        let session = Tab::Session("s".into());
        let shell = Tab::Terminal(1);
        let viewer = Tab::Viewer("/a.png".into());
        let at = |area, shown| Footing {
            picking: false,
            area,
            shown,
            direct: false,
            answering: false,
            reading: None,
            prompt: ("insert", "INSERT"),
        };
        let cases = [
            (
                Footing {
                    picking: true,
                    ..at(Area::Console, Some(&session))
                },
                ("PICK", "normal"),
            ),
            (
                Footing {
                    direct: true,
                    ..at(Area::Console, Some(&session))
                },
                ("DIRECT", "pass"),
            ),
            (
                Footing {
                    answering: true,
                    ..at(Area::Console, Some(&session))
                },
                ("PASS", "pass"),
            ),
            (at(Area::Console, Some(&shell)), ("PASS", "pass")),
            (
                Footing {
                    reading: Some("normal"),
                    ..at(Area::Console, Some(&session))
                },
                ("READ", "normal"),
            ),
            (
                Footing {
                    reading: Some("visual"),
                    ..at(Area::Console, Some(&session))
                },
                ("VISUAL", "normal"),
            ),
            (at(Area::Console, Some(&viewer)), ("VIEWER", "normal")),
            (at(Area::Console, Some(&session)), ("CONSOLE", "normal")),
            (
                Footing {
                    answering: true,
                    ..at(Area::Prompt, Some(&session))
                },
                ("PASS", "pass"),
            ),
            (at(Area::Prompt, Some(&session)), ("INSERT", "insert")),
            (at(Area::Sidebar, None), ("TREE", "normal")),
            (at(Area::Details, None), ("DETAILS", "normal")),
        ];
        for (footing, (word, class)) in cases {
            assert_eq!(footing.mode(), (word.to_string(), class), "{footing:?}");
        }
    }
}
