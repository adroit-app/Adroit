//! Which action a key runs, from the bindings adroit ships and the user's own.

use std::fmt;

use adroit_model::{Agent, Key};

use crate::actions::Action;

pub mod by_action;
mod chord;
pub mod edit;
pub mod help;

pub use chord::*;

/// The bindings adroit ships, in the format of the user's file.
pub const DEFAULTS: &str = include_str!("keymap/defaults.toml");

/// A mistake in a file of bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub(crate) String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// The areas of the window that bindings belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// Every area, after its own bindings.
    Global,
    Tree,
    Details,
    /// The console while it shows a session.
    Console,
    Prompt,
    /// Before the area's bindings while a new session is shown.
    Draft,
    /// A viewer in the focused window.
    Viewer,
    /// The lightbox over the window, while it is open.
    Lightbox,
    /// What else is drawn over the window and holds the keys: a picker, a
    /// menu, a question, the keys help, the sheet adding an environment.
    Overlay,
    /// A question to answer, before the overlay's bindings.
    Confirm,
    /// Files and directories: the tree's, with a file or a directory
    /// selected, before its own bindings, and the picker's.
    Filer,
    /// The console while it shows a session of this agent, live or read,
    /// before the console's own bindings.
    Agent(Agent),
}

impl Context {
    pub const ALL: [Context; 15] = [
        Context::Global,
        Context::Tree,
        Context::Filer,
        Context::Details,
        Context::Console,
        Context::Prompt,
        Context::Draft,
        Context::Viewer,
        Context::Lightbox,
        Context::Overlay,
        Context::Confirm,
        Context::Agent(Agent::Claude),
        Context::Agent(Agent::Codex),
        Context::Agent(Agent::Pi),
        Context::Agent(Agent::OpenCode),
    ];

    pub fn name(self) -> &'static str {
        match self {
            Context::Global => "global",
            Context::Tree => "tree",
            Context::Details => "details",
            Context::Console => "console",
            Context::Prompt => "prompt",
            Context::Draft => "draft",
            Context::Viewer => "viewer",
            Context::Lightbox => "lightbox",
            Context::Overlay => "overlay",
            Context::Confirm => "confirm",
            Context::Filer => "filer",
            Context::Agent(Agent::Claude) => "console.claude",
            Context::Agent(Agent::Codex) => "console.codex",
            Context::Agent(Agent::Pi) => "console.pi",
            Context::Agent(Agent::OpenCode) => "console.opencode",
        }
    }
    /// What the Settings window heads the area's bindings with.
    pub fn title(self) -> &'static str {
        match self {
            Context::Global => "Global",
            Context::Tree => "Tree",
            Context::Details => "Details",
            Context::Console => "Console",
            Context::Prompt => "Prompt",
            Context::Draft => "New session",
            Context::Viewer => "Viewer",
            Context::Lightbox => "Lightbox",
            Context::Overlay => "Over the window",
            Context::Confirm => "Question",
            Context::Filer => "Files",
            Context::Agent(Agent::Claude) => "Console of Claude Code",
            Context::Agent(Agent::Codex) => "Console of Codex",
            Context::Agent(Agent::Pi) => "Console of Pi",
            Context::Agent(Agent::OpenCode) => "Console of OpenCode",
        }
    }

    /// Where in the window the area's bindings work, told in words.
    pub fn hint(self) -> &'static str {
        match self {
            Context::Global => "Everywhere, after the area's own bindings.",
            Context::Tree => "The tree of workspaces, their files and what runs in them.",
            Context::Details => "The panel on the right.",
            Context::Console => "The console while it shows a session.",
            Context::Prompt => "The prompt, before Neovim takes the key.",
            Context::Draft => "Before the area's bindings while a new session is shown.",
            Context::Viewer => "A viewer of an image, a movie, an HTML page or a PDF.",
            Context::Lightbox => {
                "The lightbox of a session's images, while it is open over the window."
            }
            Context::Overlay => {
                "A picker, a menu, a question, the keys help or the sheet adding an environment, while it is open over the window; a text field there types the keys it takes."
            }
            Context::Confirm => "A question to answer, before the bindings over the window.",
            Context::Filer => {
                "The tree's files and directories, before the tree's bindings, and the picker's directories."
            }
            Context::Agent(_) => {
                "The console while it shows a session of this agent, live or read, before the console's bindings."
            }
        }
    }
}

/// The tables of keys in `table`, each by its area's name: one nested in
/// another, as `[console.claude]` is in `[console]`, named by both.
fn areas(table: toml::Table) -> Result<Vec<(String, toml::Table)>, Error> {
    fn walk(name: String, table: toml::Table, out: &mut Vec<(String, toml::Table)>) {
        let mut keys = toml::Table::new();
        for (k, v) in table {
            match v {
                toml::Value::Table(inner) => walk(format!("{name}.{k}"), inner, out),
                other => {
                    keys.insert(k, other);
                }
            }
        }
        out.push((name, keys));
    }
    let mut out = Vec::new();
    for (area, entries) in table {
        let toml::Value::Table(entries) = entries else {
            return Err(Error(format!("[{area}] is not a table of keys")));
        };
        walk(area, entries, &mut out);
    }
    Ok(out)
}

/// What a key comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Run(Action),
    /// The keys so far start a longer binding.
    Wait,
    Nothing,
}

/// Every binding in force, area by area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    bindings: Vec<(Context, Vec<Chord>, Action)>,
}

impl Keymap {
    pub fn try_defaults() -> Result<Self, Error> {
        let mut map = Keymap {
            bindings: Vec::new(),
        };
        map.overlay(DEFAULTS)?;
        Ok(map)
    }

    pub fn defaults() -> Self {
        Self::try_defaults().expect("the default bindings read")
    }

    /// The defaults with the user's file over them.
    pub fn with_user(text: &str) -> Result<Self, Error> {
        let mut map = Self::defaults();
        map.overlay(text)?;
        Ok(map)
    }

    fn overlay(&mut self, text: &str) -> Result<(), Error> {
        let table: toml::Table = text.parse().map_err(|e| Error(format!("{e}")))?;
        for (area, entries) in areas(table)? {
            let ctx = Context::ALL
                .into_iter()
                .find(|c| c.name() == area)
                .ok_or_else(|| Error(format!("{area:?} is not an area of the window")))?;
            for (keys, action) in &entries {
                let seq = parse_sequence(keys)?;
                let name = action
                    .as_str()
                    .ok_or_else(|| Error(format!("{keys:?} in [{area}] is not bound to a name")))?;
                let action: Action = name.parse()?;
                self.bindings.retain(|(c, s, _)| !(*c == ctx && *s == seq));
                self.bindings.push((ctx, seq, action));
            }
        }
        Ok(())
    }

    /// What `key` does after `pending`, looking at `contexts` in order. A
    /// binding that ends here runs at once, before one that only starts
    /// here, and an earlier area's before a later one's. In a sequence begun
    /// with Ctrl held, a later key held with Ctrl means what it means
    /// without, as after Vim's `Ctrl-w`, unless bound so itself; Tab is
    /// left out, as Ctrl with it does not reach the window everywhere.
    pub fn resolve(&self, contexts: &[Context], pending: &[Key], key: &Key) -> Step {
        let pressed: Vec<Key> = pending.iter().chain(Some(key)).cloned().collect();
        let step = self.resolve_exactly(contexts, &pressed);
        if step != Step::Nothing || !pressed[0].ctrl {
            return step;
        }
        let loosened: Vec<Key> = pressed
            .iter()
            .enumerate()
            .map(|(at, k)| Key {
                ctrl: k.ctrl && (at == 0 || k.key == "Tab"),
                ..k.clone()
            })
            .collect();
        if loosened == pressed {
            return step;
        }
        self.resolve_exactly(contexts, &loosened)
    }

    /// What `key` does after `pending` as [`Keymap::resolve`] finds it, but
    /// with a later key held with Ctrl only meaning what it is bound to.
    pub fn resolve_as_bound(&self, contexts: &[Context], pending: &[Key], key: &Key) -> Step {
        let pressed: Vec<Key> = pending.iter().chain(Some(key)).cloned().collect();
        self.resolve_exactly(contexts, &pressed)
    }

    /// Whether `key` is held with Ctrl and begins a `global` binding: the
    /// window commands' prefix, or a key that is no character bound alone,
    /// as `Ctrl+Tab` is. Ctrl with a character bound alone stays out of
    /// Neovim's insert mode, which uses those keys itself.
    fn prefix(&self, key: &Key) -> bool {
        let character = key.key.chars().count() == 1;
        key.ctrl
            && self.bindings.iter().any(|(c, seq, a)| {
                *c == Context::Global
                    && *a != Action::None
                    && (seq.len() > 1 || !character)
                    && seq[0].matches(key)
            })
    }

    fn resolve_exactly(&self, contexts: &[Context], pressed: &[Key]) -> Step {
        let mut longer = false;
        // A Command key and the window commands' prefix are the window's
        // wherever the keys are, Neovim's insert mode included, so they go
        // to the global bindings too.
        let first = &pressed[0];
        let last = &pressed[pressed.len() - 1];
        let app = |k: &Key| k.meta || Chord::held_mod(k);
        let global = ((app(first) || app(last) || self.prefix(first))
            && !contexts.contains(&Context::Global))
        .then_some(Context::Global);
        for ctx in contexts.iter().chain(global.as_ref()) {
            for (c, seq, action) in &self.bindings {
                if c != ctx || seq.len() < pressed.len() {
                    continue;
                }
                if !seq.iter().zip(pressed).all(|(chord, k)| chord.matches(k)) {
                    continue;
                }
                if seq.len() == pressed.len() {
                    return Step::Run(action.clone());
                }
                longer = true;
            }
        }
        if longer { Step::Wait } else { Step::Nothing }
    }

    /// The first keys bound to `action` in `ctx`, written as in the file.
    pub fn keys_for(&self, ctx: Context, action: &Action) -> Option<String> {
        self.first_bound(ctx, action).map(|seq| show_sequence(seq))
    }

    /// The first sequence bound to `action` in `ctx`.
    fn first_bound(&self, ctx: Context, action: &Action) -> Option<&Vec<Chord>> {
        self.bindings
            .iter()
            .find(|(c, _, a)| *c == ctx && a == action)
            .map(|(_, seq, _)| seq)
    }

    /// Every sequence bound to `action` in `ctx`, in the order of the file.
    pub fn sequences_for(&self, ctx: Context, action: &Action) -> Vec<&[Chord]> {
        self.bindings
            .iter()
            .filter(|(c, _, a)| *c == ctx && a == action)
            .map(|(_, seq, _)| seq.as_slice())
            .collect()
    }

    /// The keys of `action` in the first of `contexts` that binds it: what
    /// works in an area that reads those contexts.
    pub fn keys_in(&self, contexts: &[Context], action: &Action) -> Option<String> {
        contexts.iter().find_map(|&c| self.keys_for(c, action))
    }

    /// The keys of several actions in one: the shared first key once, then
    /// the key each adds, as in `Ctrl+q h j k l`; each in full otherwise.
    pub fn keys_for_all(&self, ctx: Context, actions: &[Action]) -> Option<String> {
        let seqs: Vec<&Vec<Chord>> = actions
            .iter()
            .map(|a| self.first_bound(ctx, a))
            .collect::<Option<_>>()?;
        let first = &seqs.first()?[0];
        if seqs.iter().all(|s| s.len() == 2 && s[0] == *first) {
            let rest: Vec<String> = seqs.iter().map(|s| s[1].to_string()).collect();
            return Some(format!("{first} {}", rest.join(" ")));
        }
        Some(
            seqs.iter()
                .map(|s| show_sequence(s))
                .collect::<Vec<_>>()
                .join(" "),
        )
    }

    /// What binding `keys` in `ctx` would collide with, told in words: the
    /// same keys bound there, keys that start one or that one starts, and
    /// a global binding it would win over. `except` is the keys of the
    /// binding being changed, which is no clash with itself.
    pub fn clashes(&self, ctx: Context, keys: &str, except: Option<&str>) -> Vec<String> {
        let Ok(seq) = parse_sequence(keys) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (c, s, a) in &self.bindings {
            if *a == Action::None || except == Some(show_sequence(s).as_str()) {
                continue;
            }
            let shared = s.iter().zip(&seq).take_while(|(x, y)| x == y).count();
            if shared < s.len().min(seq.len()) {
                continue;
            }
            let shown = show_sequence(s);
            if *c == ctx {
                out.push(format!("{shown} is {a}"));
            } else if *c == Context::Global && ctx != Context::Global && s.len() == seq.len() {
                out.push(format!(
                    "{shown} is {a} everywhere; this one wins in {}",
                    ctx.name()
                ));
            }
        }
        out
    }

    /// Every binding that does something, area by area in the order the
    /// file names them: the area, the keys and the action, as written there.
    pub fn listed(&self) -> Vec<(&'static str, String, String)> {
        Context::ALL
            .into_iter()
            .flat_map(|ctx| {
                self.bindings
                    .iter()
                    .filter(move |(c, _, a)| *c == ctx && *a != Action::None)
                    .map(move |(_, seq, a)| (ctx.name(), show_sequence(seq), a.to_string()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
