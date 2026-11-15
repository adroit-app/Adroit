//! Where a key, a text typed or a paste goes, by the area that has the keys
//! and what the console shows there, and what the footer says of it.

use adroit_model::{Agent, Key};

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
        Some(Tab::Session(_, id)) if text.trim().is_empty() => Some(id.clone()),
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

/// `here` with the bindings of `agent`'s console just before the console's,
/// where the console's are looked up.
pub fn for_agent(mut here: Vec<Context>, agent: Option<Agent>) -> Vec<Context> {
    if let Some(agent) = agent
        && let Some(at) = here.iter().position(|c| *c == Context::Console)
    {
        here.insert(at, Context::Agent(agent));
    }
    here
}

/// `key` as the reader takes it: Enter held with Ctrl is Enter.
pub fn plain_enter(key: Key) -> Key {
    match key {
        Key {
            ctrl: true,
            alt: false,
            meta: false,
            shift: false,
            ..
        } if key.key == "Enter" => Key::new("Enter"),
        other => other,
    }
}

/// The areas whose bindings the keys help shows for `area`, nearest first.
pub fn help_contexts(area: Area, shown: Option<&Tab>) -> Vec<Context> {
    let here: &[Context] = match area {
        Area::Console if matches!(shown, Some(Tab::Viewer(_, _) | Tab::GitHub { .. })) => {
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
    /// The last key of a binding that steps through something runs it again.
    pub repeating: bool,
}

impl Footing<'_> {
    pub fn mode(&self) -> (String, &'static str) {
        let (word, class) = match self.area {
            _ if self.picking => ("PICK", "normal"),
            _ if self.repeating => ("REPEAT", "pass"),
            Area::Console if self.direct => ("DIRECT", "pass"),
            Area::Console if let Some(mode) = self.reading => {
                if mode.starts_with("visual") || mode.starts_with("select") {
                    ("VISUAL", "normal")
                } else {
                    ("READ", "normal")
                }
            }
            Area::Console if self.answering || matches!(self.shown, Some(Tab::Terminal(_, _))) => {
                ("PASS", "pass")
            }
            Area::Console if matches!(self.shown, Some(Tab::Viewer(_, _) | Tab::GitHub { .. })) => {
                ("VIEWER", "normal")
            }
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
mod tests;
