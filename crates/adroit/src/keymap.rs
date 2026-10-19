//! Which action a key runs, from the bindings adroit ships and the user's own.

use std::fmt;
use std::str::FromStr;

use adroit_term::Key;

/// The bindings adroit ships, in the format of the user's file.
pub const DEFAULTS: &str = r#"
[global]
"Ctrl+w h" = "window.left"
"Ctrl+w ArrowLeft" = "window.left"
"Ctrl+w j" = "window.down"
"Ctrl+w ArrowDown" = "window.down"
"Ctrl+w k" = "window.up"
"Ctrl+w ArrowUp" = "window.up"
"Ctrl+w l" = "window.right"
"Ctrl+w ArrowRight" = "window.right"
"Ctrl+w n" = "tab.next"
"Ctrl+w p" = "tab.prev"
"Cmd+]" = "tab.next"
"Cmd+[" = "tab.prev"
"Cmd+1" = "details.overview"
"Cmd+2" = "details.changes"
"Cmd+3" = "details.log"
"Ctrl+w ]" = "waiting.next"
"Ctrl+w [" = "waiting.prev"
"Ctrl+w N" = "session.next"
"Ctrl+w P" = "session.prev"
"Cmd+}" = "session.next"
"Cmd+{" = "session.prev"
"Cmd+p" = "session.pick"
"Ctrl+w ." = "console.send Ctrl+w"
"Cmd+n" = "session.new"
"Cmd+N" = "session.new_reader"
"Cmd+s" = "shell.open"
"Cmd+t" = "session.take_over"
"Cmd+Backspace" = "session.stop"
"Cmd+f" = "filter"
"Cmd+e" = "worktrees.toggle"
"Cmd+r" = "refresh"
"Cmd+," = "settings.open"
"Cmd+=" = "zoom.in"
"Cmd+Plus" = "zoom.in"
"Cmd+-" = "zoom.out"
"Cmd+0" = "zoom.reset"
"Cmd+b" = "sidebar.toggle"
"Cmd+Alt+b" = "details.toggle"

[sidebar]
"j" = "cursor.down"
"ArrowDown" = "cursor.down"
"k" = "cursor.up"
"ArrowUp" = "cursor.up"
"g g" = "cursor.first"
"G" = "cursor.last"
"Enter" = "cursor.open"
"l" = "cursor.open"
"o" = "cursor.open"
"L" = "cursor.show"
"Shift+Enter" = "cursor.show"
"Ctrl+Enter" = "cursor.show"
"h" = "cursor.fold"
"] w" = "waiting.next"
"[ w" = "waiting.prev"
"n" = "session.new"
"N" = "session.new_reader"
"s" = "shell.open"
"x" = "tab.close"
"X" = "session.stop"
"R" = "session.rename"
"t" = "session.take_over"
"i" = "prompt.write"
"a" = "prompt.write"
"r" = "refresh"
"/" = "filter"
"w" = "session.pick"
"!" = "worktrees.toggle"

[details]
"j" = "cursor.down"
"ArrowDown" = "cursor.down"
"k" = "cursor.up"
"ArrowUp" = "cursor.up"
"g g" = "cursor.first"
"G" = "cursor.last"
"Escape" = "cursor.fold"
"Backspace" = "cursor.fold"
"Ctrl+d" = "scroll.half_down"
"Ctrl+u" = "scroll.half_up"

[console]
"Enter" = "console.send"
"ArrowUp" = "prompt.history_prev"
"ArrowDown" = "prompt.history_next"
"ArrowLeft" = "console.send"
"ArrowRight" = "console.send"
"Tab" = "console.send"
"Shift+Tab" = "console.send"
"F2" = "console.send"
"Ctrl+l" = "console.send"
"Ctrl+p" = "prompt.history_prev"
"Ctrl+n" = "prompt.history_next"
"Ctrl+o" = "console.send"
"Ctrl+t" = "console.send"
"Ctrl+_" = "console.send"
"Ctrl+-" = "console.send Ctrl+_"
"Ctrl+c" = "console.send Escape"
"g Ctrl+c" = "console.send Ctrl+c"
"Ctrl+g Ctrl+o" = "console.send_next"
"Ctrl+g i" = "console.pass_every_key"
"Ctrl+g Ctrl+i" = "console.pass_every_key"
"i" = "prompt.write"
"a" = "prompt.write"
"I" = "prompt.write"
"A" = "prompt.write"
"o" = "prompt.write"
"O" = "prompt.write"
"t" = "session.take_over"
"X" = "session.stop"

[prompt]
"Ctrl+Enter" = "prompt.send"
"Ctrl+p" = "prompt.history_prev"
"Ctrl+n" = "prompt.history_next"
"#;

/// A mistake in a file of bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// The keys that are not one character, by the names the window reports them.
const NAMED: &[&str] = &[
    "Enter",
    "Escape",
    "Tab",
    "Backspace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
];

/// Keys that stay with the menu with Command held: copy, paste and the rest
/// of editing, quitting, hiding, minimizing and closing.
pub const MENU_KEYS: &[&str] = &["c", "v", "x", "a", "z", "q", "w", "h", "m"];

fn menu_key(key: &str) -> bool {
    MENU_KEYS.contains(&key.to_lowercase().as_str())
}

/// One key with the modifiers held for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    key: String,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
}

impl Chord {
    pub fn parse(text: &str) -> Result<Self, Error> {
        let bad = || Error(format!("{text:?} is not a key"));
        let (mods, name) = match text.rsplit_once('+') {
            Some((mods, name)) => (mods, name),
            None => ("", text),
        };
        let mut c = Chord {
            key: String::new(),
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
        };
        for m in mods.split('+').filter(|_| !mods.is_empty()) {
            match m {
                "Ctrl" => c.ctrl = true,
                "Alt" => c.alt = true,
                "Shift" => c.shift = true,
                "Cmd" => c.meta = true,
                _ => return Err(bad()),
            }
        }
        c.key = match name {
            "Space" => " ".into(),
            "Plus" => "+".into(),
            n if n.chars().count() == 1 => n.into(),
            n if NAMED.contains(&n) => n.into(),
            n if n
                .strip_prefix('F')
                .is_some_and(|d| d.parse::<u8>().is_ok_and(|d| (1..=12).contains(&d))) =>
            {
                n.into()
            }
            _ => return Err(bad()),
        };
        // A character carries its own case; Shift with a letter is its
        // capital.
        if c.printable() {
            if c.shift {
                // Shift with another character types a third one, written as
                // itself: Shift and 1 is !.
                if !c.key.chars().all(char::is_alphabetic) {
                    return Err(bad());
                }
                c.key = c.key.to_uppercase();
            }
            c.shift = false;
        }
        if c.meta && menu_key(&c.key) {
            return Err(Error(format!("{text:?} stays with the menu")));
        }
        Ok(c)
    }

    /// A key pressed in the window, by the name it reports and its
    /// modifiers, written as the file writes it; `None` for a modifier on
    /// its own and for Command with a key the menu keeps.
    pub fn pressed(key: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> Option<String> {
        if (meta && menu_key(key))
            || matches!(
                key,
                "Control" | "Alt" | "Shift" | "Meta" | "CapsLock" | "Dead" | "Process"
            )
        {
            return None;
        }
        let name = match key {
            " " => "Space".to_string(),
            "+" => "Plus".to_string(),
            k => k.to_string(),
        };
        // A character carries its own case, so Shift is already in it.
        let character = key.chars().count() == 1 && key != " ";
        let mut text = String::new();
        for (on, m) in [
            (meta, "Cmd+"),
            (ctrl, "Ctrl+"),
            (alt, "Alt+"),
            (shift && !character, "Shift+"),
        ] {
            if on {
                text.push_str(m);
            }
        }
        text.push_str(&name);
        Chord::parse(&text).ok().map(|c| c.to_string())
    }

    fn printable(&self) -> bool {
        self.key.chars().count() == 1 && self.key != " "
    }

    pub fn matches(&self, key: &Key) -> bool {
        key.key == self.key
            && key.ctrl == self.ctrl
            && key.alt == self.alt
            && key.meta == self.meta
            && (self.printable() || key.shift == self.shift)
    }

    /// The key this chord stands for, as the window would report it.
    pub fn key(&self) -> Key {
        let shifted = self.printable() && self.key.chars().any(char::is_uppercase);
        Key {
            key: self.key.clone(),
            ctrl: self.ctrl,
            alt: self.alt,
            shift: self.shift || shifted,
            meta: self.meta,
        }
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.meta, "Cmd+"),
            (self.ctrl, "Ctrl+"),
            (self.alt, "Alt+"),
            (self.shift, "Shift+"),
        ] {
            if on {
                f.write_str(name)?;
            }
        }
        match self.key.as_str() {
            " " => f.write_str("Space"),
            "+" => f.write_str("Plus"),
            k => f.write_str(k),
        }
    }
}

fn parse_sequence(text: &str) -> Result<Vec<Chord>, Error> {
    let chords: Vec<Chord> = text
        .split(' ')
        .filter(|t| !t.is_empty())
        .map(Chord::parse)
        .collect::<Result<_, _>>()?;
    if chords.is_empty() {
        return Err(Error(format!("{text:?} is not a key")));
    }
    Ok(chords)
}

fn show_sequence(chords: &[Chord]) -> String {
    chords
        .iter()
        .map(Chord::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The areas of the window that bindings belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// Every area, after its own bindings.
    Global,
    Sidebar,
    Details,
    /// The console while it shows a session.
    Console,
    Prompt,
}

impl Context {
    pub const ALL: [Context; 5] = [
        Context::Global,
        Context::Sidebar,
        Context::Details,
        Context::Console,
        Context::Prompt,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Context::Global => "global",
            Context::Sidebar => "sidebar",
            Context::Details => "details",
            Context::Console => "console",
            Context::Prompt => "prompt",
        }
    }
}

/// Actions earlier versions had: a binding to one is dropped as it is read,
/// so a file that has one still reads.
const RETIRED: &[&str] = &[
    "coordinator.open",
    "project.new",
    "project.edit",
    "workspace.pick",
    "sheet.prev",
    "sheet.next",
    "details.files",
    "details.diff",
];

macro_rules! actions {
    ($($variant:ident => $name:literal,)*) => {
        /// What a key does.
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum Action {
            /// Nothing, in place of a binding taken away.
            None,
            $($variant,)*
            /// Send a key to the session: the one named, or the one pressed.
            ConsoleSend(Option<Chord>),
        }

        impl Action {
            /// Every action's name, as the file names them.
            pub const NAMES: &[&str] = &[$($name,)* "console.send"];
        }

        impl FromStr for Action {
            type Err = Error;

            fn from_str(text: &str) -> Result<Self, Error> {
                let (name, arg) = match text.split_once(' ') {
                    Some((n, a)) => (n, Some(a.trim())),
                    None => (text, None),
                };
                Ok(match (name, arg) {
                    ("none", None) => Action::None,
                    ("console.send", None) => Action::ConsoleSend(None),
                    ("console.send", Some(k)) => Action::ConsoleSend(Some(Chord::parse(k)?)),
                    $(($name, None) => Action::$variant,)*
                    _ => return Err(Error(format!("{text:?} is not an action"))),
                })
            }
        }

        impl fmt::Display for Action {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Action::None => f.write_str("none"),
                    Action::ConsoleSend(None) => f.write_str("console.send"),
                    Action::ConsoleSend(Some(c)) => write!(f, "console.send {c}"),
                    $(Action::$variant => f.write_str($name),)*
                }
            }
        }
    };
}

actions! {
    WindowLeft => "window.left",
    WindowDown => "window.down",
    WindowUp => "window.up",
    WindowRight => "window.right",
    TabNext => "tab.next",
    TabPrev => "tab.prev",
    CursorDown => "cursor.down",
    CursorUp => "cursor.up",
    CursorFirst => "cursor.first",
    CursorLast => "cursor.last",
    CursorOpen => "cursor.open",
    CursorShow => "cursor.show",
    CursorFold => "cursor.fold",
    WaitingNext => "waiting.next",
    WaitingPrev => "waiting.prev",
    SessionNext => "session.next",
    SessionPrev => "session.prev",
    SessionPick => "session.pick",
    SessionNew => "session.new",
    SessionNewReader => "session.new_reader",
    SessionTakeOver => "session.take_over",
    SessionStop => "session.stop",
    SessionRename => "session.rename",
    ShellOpen => "shell.open",
    ShellClose => "shell.close",
    TabClose => "tab.close",
    Refresh => "refresh",
    Filter => "filter",
    DetailsOverview => "details.overview",
    DetailsChanges => "details.changes",
    DetailsLog => "details.log",
    WorktreesToggle => "worktrees.toggle",
    ScrollDown => "scroll.down",
    ScrollUp => "scroll.up",
    ScrollHalfDown => "scroll.half_down",
    ScrollHalfUp => "scroll.half_up",
    ScrollTop => "scroll.top",
    ScrollBottom => "scroll.bottom",
    ConsoleSendNext => "console.send_next",
    ConsolePassEveryKey => "console.pass_every_key",
    PromptFocus => "prompt.focus",
    PromptWrite => "prompt.write",
    PromptSend => "prompt.send",
    HistoryPrev => "prompt.history_prev",
    HistoryNext => "prompt.history_next",
    SettingsOpen => "settings.open",
    ZoomIn => "zoom.in",
    ZoomOut => "zoom.out",
    ZoomReset => "zoom.reset",
    SidebarToggle => "sidebar.toggle",
    DetailsToggle => "details.toggle",
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
        for (area, entries) in table {
            let ctx = Context::ALL
                .into_iter()
                .find(|c| c.name() == area)
                .ok_or_else(|| Error(format!("{area:?} is not an area of the window")))?;
            let entries = entries
                .as_table()
                .ok_or_else(|| Error(format!("[{area}] is not a table of keys")))?;
            for (keys, action) in entries {
                let seq = parse_sequence(keys)?;
                let name = action
                    .as_str()
                    .ok_or_else(|| Error(format!("{keys:?} in [{area}] is not bound to a name")))?;
                if RETIRED.contains(&name) {
                    continue;
                }
                let action: Action = name.parse()?;
                self.bindings.retain(|(c, s, _)| !(*c == ctx && *s == seq));
                self.bindings.push((ctx, seq, action));
            }
        }
        Ok(())
    }

    /// What `key` does after `pending`, looking at `contexts` in order. A
    /// binding that ends here runs at once, before one that only starts
    /// here, and an earlier area's before a later one's.
    pub fn resolve(&self, contexts: &[Context], pending: &[Key], key: &Key) -> Step {
        let pressed: Vec<&Key> = pending.iter().chain(Some(key)).collect();
        let mut longer = false;
        // A Command key is the window's wherever the keys are: no program
        // or editor takes it, so it goes to the global bindings too.
        let global = (key.meta && !contexts.contains(&Context::Global)).then_some(Context::Global);
        for ctx in contexts.iter().chain(global.as_ref()) {
            for (c, seq, action) in &self.bindings {
                if c != ctx || seq.len() < pressed.len() {
                    continue;
                }
                if !seq.iter().zip(&pressed).all(|(chord, k)| chord.matches(k)) {
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
        self.bindings
            .iter()
            .find(|(c, _, a)| *c == ctx && a == action)
            .map(|(_, seq, _)| show_sequence(seq))
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
    /// the key each adds, as in `Ctrl+w h j k l`; each in full otherwise.
    pub fn keys_for_all(&self, ctx: Context, actions: &[Action]) -> Option<String> {
        let seqs: Vec<&Vec<Chord>> = actions
            .iter()
            .map(|a| {
                self.bindings
                    .iter()
                    .find(|(c, _, b)| *c == ctx && b == a)
                    .map(|(_, s, _)| s)
            })
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
mod tests {
    use super::*;

    fn press(chord: &str) -> Key {
        Chord::parse(chord).unwrap().key()
    }

    fn step(map: &Keymap, contexts: &[Context], pending: &[&str], key: &str) -> Step {
        let pending: Vec<Key> = pending.iter().map(|k| press(k)).collect();
        map.resolve(contexts, &pending, &press(key))
    }

    #[test]
    fn a_chord_names_its_modifiers_before_its_key() {
        let c = Chord::parse("Ctrl+Shift+Tab").unwrap();
        assert_eq!(c.key(), Key::new("Tab").ctrl().shift());
        assert_eq!(
            Chord::parse("Ctrl+Alt+x").unwrap().key(),
            Key::new("x").ctrl().alt()
        );
    }

    #[test]
    fn the_bindings_are_listed_by_area_with_the_user_s_in_place_of_the_defaults() {
        let map =
            Keymap::with_user("[sidebar]\n\"j\" = \"none\"\n\"J\" = \"cursor.down\"\n").unwrap();
        let listed = map.listed();
        assert!(listed.contains(&("sidebar", "J".into(), "cursor.down".into())));
        assert!(!listed.iter().any(|(a, k, _)| *a == "sidebar" && k == "j"));
        let areas: Vec<&str> = listed.iter().map(|(a, _, _)| *a).collect();
        let first_sidebar = areas.iter().position(|a| *a == "sidebar").unwrap();
        assert!(areas[..first_sidebar].iter().all(|a| *a == "global"));
    }

    #[test]
    fn a_pressed_key_is_written_as_the_file_writes_it() {
        let c = |k: &str, ctrl, alt, shift, meta| Chord::pressed(k, ctrl, alt, shift, meta);
        assert_eq!(c("w", true, false, false, false), Some("Ctrl+w".into()));
        // A character carries its case, and Shift is in it.
        assert_eq!(c("G", false, false, true, false), Some("G".into()));
        assert_eq!(
            c("Tab", false, false, true, false),
            Some("Shift+Tab".into())
        );
        assert_eq!(c(" ", false, false, false, false), Some("Space".into()));
        assert_eq!(c("+", false, false, true, false), Some("Plus".into()));
        // A modifier alone is no key yet; Command with what the menu uses
        // stays with the menu.
        assert_eq!(c("Control", true, false, false, false), None);
        assert_eq!(c("j", false, false, false, true), Some("Cmd+j".into()));
        assert_eq!(c("c", false, false, false, true), None);
    }

    #[test]
    fn every_action_is_offered_by_its_name() {
        assert!(Action::NAMES.contains(&"cursor.down"));
        assert!(Action::NAMES.contains(&"console.send"));
        for n in Action::NAMES {
            assert!(n.parse::<Action>().is_ok(), "{n}");
        }
    }

    #[test]
    fn keys_already_bound_in_the_area_or_starting_a_longer_binding_are_told() {
        let map = Keymap::defaults();
        // `j` is cursor.down in the sidebar.
        assert_eq!(
            map.clashes(Context::Sidebar, "j", None),
            vec!["j is cursor.down".to_string()]
        );
        // Editing that binding itself is no clash.
        assert!(map.clashes(Context::Sidebar, "j", Some("j")).is_empty());
        // `g` alone would cut `g g` and `g t` short.
        let g = map.clashes(Context::Sidebar, "g", None);
        assert!(g.iter().any(|c| c == "g g is cursor.first"), "{g:?}");
        // Over a global binding, the area's wins there.
        let w = map.clashes(Context::Sidebar, "Ctrl+w h", None);
        assert_eq!(
            w,
            vec!["Ctrl+w h is window.left everywhere; this one wins in sidebar".to_string()]
        );
        assert!(map.clashes(Context::Sidebar, "Ctrl+q", None).is_empty());
    }

    #[test]
    fn command_keys_the_menu_uses_cannot_be_bound() {
        for taken in [
            "Cmd+c", "Cmd+v", "Cmd+q", "Cmd+w", "Cmd+Z", "Cmd+h", "Cmd+m",
        ] {
            assert!(Chord::parse(taken).is_err(), "{taken}");
        }
        for free in ["Cmd+b", "Cmd+0", "Cmd+,", "Cmd+Plus", "Cmd+-"] {
            assert!(Chord::parse(free).is_ok(), "{free}");
        }
        let chord = Chord::parse("Cmd+N").unwrap();
        assert!(chord.matches(&Key::new("N").shift().meta()));
        assert!(!chord.matches(&Key::new("N").shift()));
        assert_eq!(chord.to_string(), "Cmd+N");
    }

    #[test]
    fn the_window_s_own_command_keys_are_global_bindings() {
        let map = Keymap::defaults();
        let run = |key: Key| map.resolve(&[Context::Sidebar], &[], &key);
        assert_eq!(run(Key::new(",").meta()), Step::Run(Action::SettingsOpen));
        assert_eq!(run(Key::new("=").meta()), Step::Run(Action::ZoomIn));
        assert_eq!(run(Key::new("+").meta().shift()), Step::Run(Action::ZoomIn));
        assert_eq!(run(Key::new("-").meta()), Step::Run(Action::ZoomOut));
        assert_eq!(run(Key::new("0").meta()), Step::Run(Action::ZoomReset));
        assert_eq!(run(Key::new("b").meta()), Step::Run(Action::SidebarToggle));
        assert_eq!(
            run(Key::new("b").meta().alt()),
            Step::Run(Action::DetailsToggle)
        );
    }

    #[test]
    fn a_command_key_resolves_among_the_global_bindings_wherever_the_keys_are() {
        let map = Keymap::defaults();
        let key = Key::new("s").meta();
        assert_eq!(
            map.resolve(&[Context::Prompt], &[], &key),
            Step::Run(Action::ShellOpen)
        );
    }

    #[test]
    fn shift_with_a_character_other_than_a_letter_is_refused() {
        // Shift and 1 types !, which is written as itself.
        assert!(Chord::parse("Shift+1").is_err());
        assert_eq!(Chord::parse("Shift+g").unwrap(), Chord::parse("G").unwrap());
    }

    #[test]
    fn a_letter_s_case_is_part_of_it() {
        assert!(Chord::parse("G").unwrap().matches(&Key::new("G").shift()));
        assert!(!Chord::parse("G").unwrap().matches(&Key::new("g")));
        assert!(!Chord::parse("g").unwrap().matches(&Key::new("G").shift()));
    }

    #[test]
    fn a_named_key_needs_its_modifiers_exactly() {
        assert!(
            Chord::parse("Shift+Tab")
                .unwrap()
                .matches(&Key::new("Tab").shift())
        );
        assert!(
            !Chord::parse("Tab")
                .unwrap()
                .matches(&Key::new("Tab").shift())
        );
        assert!(!Chord::parse("Ctrl+w").unwrap().matches(&Key::new("w")));
    }

    #[test]
    fn space_and_plus_have_names_of_their_own() {
        assert_eq!(Chord::parse("Space").unwrap().key(), Key::new(" "));
        assert_eq!(
            Chord::parse("Ctrl+Plus").unwrap().key(),
            Key::new("+").ctrl()
        );
    }

    #[test]
    fn a_chord_that_names_nothing_is_refused() {
        for bad in ["", "Ctrl+", "Hyper+x", "Ctrl++"] {
            assert!(Chord::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_chord_reads_back_as_written() {
        for s in ["Ctrl+w", "G", "Shift+Tab", "Space", "Ctrl+Alt+x"] {
            assert_eq!(Chord::parse(s).unwrap().to_string(), s);
        }
    }

    #[test]
    fn an_action_reads_from_its_name_and_back() {
        let a: Action = "cursor.down".parse().unwrap();
        assert_eq!(a, Action::CursorDown);
        assert_eq!(a.to_string(), "cursor.down");
        assert!("cursor.sideways".parse::<Action>().is_err());
    }

    #[test]
    fn sending_a_key_names_the_key_to_send() {
        let a: Action = "console.send Escape".parse().unwrap();
        assert_eq!(
            a,
            Action::ConsoleSend(Some(Chord::parse("Escape").unwrap()))
        );
        assert_eq!(a.to_string(), "console.send Escape");
        assert_eq!(
            "console.send".parse::<Action>().unwrap(),
            Action::ConsoleSend(None)
        );
    }

    #[test]
    fn a_bound_key_runs_its_action() {
        let map = Keymap::defaults();
        assert_eq!(
            step(&map, &[Context::Sidebar], &[], "j"),
            Step::Run(Action::CursorDown)
        );
    }

    #[test]
    fn the_start_of_a_sequence_waits_for_the_rest() {
        let map = Keymap::defaults();
        assert_eq!(step(&map, &[Context::Sidebar], &[], "g"), Step::Wait);
        assert_eq!(
            step(&map, &[Context::Sidebar], &["g"], "g"),
            Step::Run(Action::CursorFirst)
        );
        assert_eq!(step(&map, &[Context::Sidebar], &["g"], "z"), Step::Nothing);
    }

    #[test]
    fn a_key_bound_nowhere_in_the_areas_does_nothing() {
        let map = Keymap::defaults();
        assert_eq!(step(&map, &[Context::Sidebar], &[], "y"), Step::Nothing);
    }

    #[test]
    fn an_earlier_area_wins_over_a_later_one() {
        let map = Keymap::with_user("[details]\n\"n\" = \"refresh\"\n").unwrap();
        let both = [Context::Details, Context::Sidebar];
        assert_eq!(step(&map, &both, &[], "n"), Step::Run(Action::Refresh));
        assert_eq!(
            step(&map, &[Context::Sidebar], &[], "n"),
            Step::Run(Action::SessionNew)
        );
        // A key the details panel leaves alone falls through to the sidebar's.
        assert_eq!(
            step(&map, &both, &[], "N"),
            Step::Run(Action::SessionNewReader)
        );
    }

    #[test]
    fn the_details_panel_moves_opens_and_goes_back_with_the_cursor_and_scrolls_by_half() {
        let map = Keymap::defaults();
        let areas = [Context::Details, Context::Sidebar];
        for (key, action) in [
            ("j", Action::CursorDown),
            ("ArrowDown", Action::CursorDown),
            ("k", Action::CursorUp),
            ("ArrowUp", Action::CursorUp),
            ("G", Action::CursorLast),
            ("Enter", Action::CursorOpen),
            ("l", Action::CursorOpen),
            ("Shift+Enter", Action::CursorShow),
            ("h", Action::CursorFold),
            ("Escape", Action::CursorFold),
            ("Backspace", Action::CursorFold),
            ("Ctrl+d", Action::ScrollHalfDown),
            ("Ctrl+u", Action::ScrollHalfUp),
        ] {
            assert_eq!(step(&map, &areas, &[], key), Step::Run(action), "{key}");
        }
        assert_eq!(
            step(&map, &areas, &["g"], "g"),
            Step::Run(Action::CursorFirst)
        );
    }

    #[test]
    fn the_tabs_are_switched_by_the_same_keys_in_every_area() {
        let map = Keymap::defaults();
        for ctx in [
            Context::Sidebar,
            Context::Details,
            Context::Console,
            Context::Prompt,
        ] {
            let resolve = |k: Key| map.resolve(&[ctx, Context::Global], &[], &k);
            assert_eq!(resolve(Key::new("]").meta()), Step::Run(Action::TabNext));
            assert_eq!(resolve(Key::new("[").meta()), Step::Run(Action::TabPrev));
        }
    }

    #[test]
    fn any_session_is_a_key_away_from_every_area() {
        let map = Keymap::defaults();
        for ctx in [
            Context::Sidebar,
            Context::Details,
            Context::Console,
            Context::Prompt,
        ] {
            let areas = [ctx, Context::Global];
            // Command with Shift and a bracket arrives as the brace it types.
            let resolve = |k: Key| map.resolve(&areas, &[], &k);
            let brace = |b: &str| Key::new(b).meta().shift();
            assert_eq!(resolve(brace("}")), Step::Run(Action::SessionNext));
            assert_eq!(resolve(brace("{")), Step::Run(Action::SessionPrev));
            assert_eq!(
                resolve(Key::new("p").meta()),
                Step::Run(Action::SessionPick)
            );
            assert_eq!(
                step(&map, &areas, &["Ctrl+w"], "N"),
                Step::Run(Action::SessionNext)
            );
            assert_eq!(
                step(&map, &areas, &["Ctrl+w"], "P"),
                Step::Run(Action::SessionPrev)
            );
        }
        assert_eq!(
            Chord::pressed("}", false, false, true, true),
            Some("Cmd+}".into())
        );
    }

    #[test]
    fn each_sheet_of_the_details_panel_has_a_key_of_its_own_everywhere() {
        let map = Keymap::defaults();
        for (digit, action) in [
            ("1", Action::DetailsOverview),
            ("2", Action::DetailsChanges),
            ("3", Action::DetailsLog),
        ] {
            let key = Key::new(digit).meta();
            for ctx in [Context::Sidebar, Context::Console, Context::Prompt] {
                assert_eq!(
                    map.resolve(&[ctx], &[], &key),
                    Step::Run(action.clone()),
                    "{digit}"
                );
            }
        }
    }

    #[test]
    fn a_binding_to_the_sheets_actions_earlier_versions_had_is_dropped() {
        let map = Keymap::with_user(
            "[details]\n\"J\" = \"sheet.next\"\n\"K\" = \"scroll.up\"\n\
             [global]\n\"Cmd+4\" = \"details.files\"\n\"Cmd+5\" = \"details.diff\"\n",
        )
        .unwrap();
        for digit in ["4", "5"] {
            assert_eq!(
                map.resolve(&[Context::Global], &[], &Key::new(digit).meta()),
                Step::Nothing,
                "{digit}"
            );
        }
        assert_eq!(
            map.resolve(&[Context::Details], &[], &Key::new("K").shift()),
            Step::Run(Action::ScrollUp)
        );
    }

    #[test]
    fn window_motions_reach_every_area() {
        let map = Keymap::defaults();
        for ctx in [Context::Sidebar, Context::Details, Context::Console] {
            assert_eq!(
                step(&map, &[ctx, Context::Global], &["Ctrl+w"], "l"),
                Step::Run(Action::WindowRight)
            );
        }
    }

    #[test]
    fn the_user_s_binding_replaces_the_default_for_the_same_keys() {
        let map = Keymap::with_user("[sidebar]\n\"j\" = \"cursor.up\"\n").unwrap();
        assert_eq!(
            step(&map, &[Context::Sidebar], &[], "j"),
            Step::Run(Action::CursorUp)
        );
        assert_eq!(
            step(&map, &[Context::Sidebar], &[], "k"),
            Step::Run(Action::CursorUp)
        );
    }

    #[test]
    fn the_user_can_add_keys_to_an_action() {
        let map = Keymap::with_user("[sidebar]\n\"Ctrl+n\" = \"cursor.down\"\n").unwrap();
        assert_eq!(
            step(&map, &[Context::Sidebar], &[], "Ctrl+n"),
            Step::Run(Action::CursorDown)
        );
        assert_eq!(
            step(&map, &[Context::Sidebar], &[], "j"),
            Step::Run(Action::CursorDown)
        );
    }

    #[test]
    fn none_takes_a_key_away_even_from_later_areas() {
        let map = Keymap::with_user("[console]\n\"Ctrl+w\" = \"none\"\n").unwrap();
        assert_eq!(
            step(&map, &[Context::Console, Context::Global], &[], "Ctrl+w"),
            Step::Run(Action::None)
        );
    }

    #[test]
    fn a_binding_to_an_action_earlier_versions_had_is_read_and_dropped() {
        let map = Keymap::with_user(
            "[sidebar]\n\"P\" = \"project.new\"\n\"c\" = \"coordinator.open\"\n\"J\" = \"cursor.down\"\n",
        )
        .unwrap();
        assert_eq!(
            step(&map, &[Context::Sidebar], &[], "J"),
            Step::Run(Action::CursorDown)
        );
        assert_eq!(step(&map, &[Context::Sidebar], &[], "P"), Step::Nothing);
    }

    #[test]
    fn a_binding_to_the_workspace_picker_earlier_versions_had_is_dropped() {
        let map = Keymap::with_user("[global]\n\"Cmd+y\" = \"workspace.pick\"\n").unwrap();
        let key = Key::new("y").meta();
        assert_eq!(map.resolve(&[Context::Global], &[], &key), Step::Nothing);
        assert!(!Action::NAMES.contains(&"workspace.pick"));
    }

    #[test]
    fn w_in_the_sidebar_opens_the_one_picker_and_cmd_o_is_free() {
        let map = Keymap::defaults();
        assert_eq!(
            step(&map, &[Context::Sidebar, Context::Global], &[], "w"),
            Step::Run(Action::SessionPick)
        );
        let o = Key::new("o").meta();
        assert_eq!(map.resolve(&[Context::Global], &[], &o), Step::Nothing);
    }

    #[test]
    fn an_unknown_area_key_or_action_in_the_user_s_file_is_reported() {
        for bad in [
            "[side]\n\"j\" = \"cursor.down\"\n",
            "[sidebar]\n\"Hyper+j\" = \"cursor.down\"\n",
            "[sidebar]\n\"j\" = \"cursor.sideways\"\n",
            "[sidebar]\n\"j\" = 1\n",
        ] {
            assert!(Keymap::with_user(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_keys_of_an_action_read_as_written_for_the_window_to_show() {
        let map = Keymap::defaults();
        assert_eq!(
            map.keys_for(Context::Sidebar, &Action::WaitingNext),
            Some("] w".into())
        );
        assert_eq!(
            map.keys_for(Context::Sidebar, &Action::SessionTakeOver),
            Some("t".into())
        );
        let map = Keymap::with_user("[sidebar]\n\"t\" = \"none\"\n\"T\" = \"session.take_over\"\n")
            .unwrap();
        assert_eq!(
            map.keys_for(Context::Sidebar, &Action::SessionTakeOver),
            Some("T".into())
        );
    }

    #[test]
    fn an_action_s_keys_are_found_only_in_the_areas_they_work_in() {
        let map = Keymap::defaults();
        let sidebar = [Context::Sidebar, Context::Global];
        let console = [Context::Console, Context::Global];
        assert_eq!(
            map.keys_in(&sidebar, &Action::WaitingNext),
            Some("] w".into())
        );
        assert_eq!(
            map.keys_in(&console, &Action::WaitingNext),
            Some("Ctrl+w ]".into())
        );
        assert_eq!(map.keys_in(&console, &Action::CursorDown), None);
        let map = Keymap::with_user("[global]\n\"Ctrl+w j\" = \"cursor.down\"\n").unwrap();
        assert_eq!(
            map.keys_in(&console, &Action::CursorDown),
            Some("Ctrl+w j".into())
        );
    }

    fn console(pending: &[&str], key: &str) -> Step {
        step(
            &Keymap::defaults(),
            &[Context::Console, Context::Global],
            pending,
            key,
        )
    }

    #[test]
    fn in_the_console_keys_that_start_writing_go_to_the_prompt() {
        for k in ["i", "a", "I", "A", "o", "O"] {
            assert_eq!(console(&[], k), Step::Run(Action::PromptWrite), "{k}");
        }
    }

    #[test]
    fn a_slash_in_the_sidebar_starts_a_filter() {
        let sidebar = [Context::Sidebar, Context::Global];
        assert_eq!(
            step(&Keymap::defaults(), &sidebar, &[], "/"),
            Step::Run(Action::Filter)
        );
    }

    #[test]
    fn in_the_sidebar_capital_l_and_enter_with_shift_or_ctrl_show_the_row_and_keep_the_keys() {
        let map = Keymap::defaults();
        let sidebar = [Context::Sidebar, Context::Global];
        for k in ["L", "Shift+Enter", "Ctrl+Enter"] {
            assert_eq!(
                step(&map, &sidebar, &[], k),
                Step::Run(Action::CursorShow),
                "{k}"
            );
        }
        // As the window reports them.
        assert_eq!(
            Chord::pressed("Enter", false, false, true, false),
            Some("Shift+Enter".into())
        );
        assert_eq!(
            Chord::pressed("L", false, false, true, false),
            Some("L".into())
        );
        assert_eq!(
            step(&map, &sidebar, &[], "Enter"),
            Step::Run(Action::CursorOpen)
        );
    }

    #[test]
    fn x_closes_the_tab_and_capital_x_stops_a_session() {
        let map = Keymap::defaults();
        let sidebar = [Context::Sidebar, Context::Global];
        let console = [Context::Console, Context::Global];
        assert_eq!(step(&map, &sidebar, &[], "x"), Step::Run(Action::TabClose));
        assert_eq!(
            step(&map, &sidebar, &[], "X"),
            Step::Run(Action::SessionStop)
        );
        assert_eq!(
            step(&map, &console, &[], "X"),
            Step::Run(Action::SessionStop)
        );
    }

    #[test]
    fn the_next_waiting_session_is_a_key_away_while_answering_one() {
        // Answering, only global bindings apply.
        let answering = [Context::Global];
        let map = Keymap::defaults();
        assert_eq!(
            step(&map, &answering, &["Ctrl+w"], "]"),
            Step::Run(Action::WaitingNext)
        );
        assert_eq!(
            step(&map, &answering, &["Ctrl+w"], "["),
            Step::Run(Action::WaitingPrev)
        );
    }

    #[test]
    fn in_the_sidebar_i_and_a_start_writing_as_in_the_console() {
        for k in ["i", "a"] {
            let got = step(
                &Keymap::defaults(),
                &[Context::Sidebar, Context::Global],
                &[],
                k,
            );
            assert_eq!(got, Step::Run(Action::PromptWrite), "{k}");
        }
    }

    #[test]
    fn in_the_console_other_text_is_not_typed_into_the_session() {
        for k in ["x", "y", "1", "Space", "/"] {
            assert_eq!(console(&[], k), Step::Nothing, "{k}");
        }
    }

    #[test]
    fn in_the_prompt_ctrl_p_ctrl_n_recall_the_prompts_sent_and_the_arrows_stay_neovim_s() {
        let prompt = |k| step(&Keymap::defaults(), &[Context::Prompt], &[], k);
        assert_eq!(prompt("Ctrl+p"), Step::Run(Action::HistoryPrev));
        assert_eq!(prompt("Ctrl+n"), Step::Run(Action::HistoryNext));
        assert_eq!(prompt("ArrowUp"), Step::Nothing);
    }

    #[test]
    fn in_the_console_the_arrows_and_ctrl_p_ctrl_n_recall_the_prompts_sent_too() {
        for (k, a) in [
            ("ArrowUp", Action::HistoryPrev),
            ("Ctrl+p", Action::HistoryPrev),
            ("ArrowDown", Action::HistoryNext),
            ("Ctrl+n", Action::HistoryNext),
        ] {
            assert_eq!(console(&[], k), Step::Run(a), "{k}");
        }
    }

    #[test]
    fn in_the_console_the_keys_claude_code_s_controls_use_are_sent() {
        for k in [
            "Enter",
            "ArrowLeft",
            "ArrowRight",
            "Tab",
            "Shift+Tab",
            "F2",
            "Ctrl+l",
            "Ctrl+o",
            "Ctrl+t",
            "Ctrl+_",
        ] {
            assert_eq!(console(&[], k), Step::Run(Action::ConsoleSend(None)), "{k}");
        }
        assert_eq!(
            console(&[], "Ctrl+-"),
            Step::Run(Action::ConsoleSend(Some(Chord::parse("Ctrl+_").unwrap())))
        );
    }

    #[test]
    fn in_the_console_escape_is_kept_and_ctrl_c_sends_it() {
        assert_eq!(console(&[], "Escape"), Step::Nothing);
        assert_eq!(
            console(&[], "Ctrl+c"),
            Step::Run(Action::ConsoleSend(Some(Chord::parse("Escape").unwrap())))
        );
        assert_eq!(
            console(&["g"], "Ctrl+c"),
            Step::Run(Action::ConsoleSend(Some(Chord::parse("Ctrl+c").unwrap())))
        );
    }

    #[test]
    fn in_the_console_ctrl_g_asks_for_the_next_key_or_every_key() {
        assert_eq!(console(&[], "Ctrl+g"), Step::Wait);
        assert_eq!(
            console(&["Ctrl+g"], "Ctrl+o"),
            Step::Run(Action::ConsoleSendNext)
        );
        assert_eq!(
            console(&["Ctrl+g"], "i"),
            Step::Run(Action::ConsolePassEveryKey)
        );
        assert_eq!(
            console(&["Ctrl+g"], "Ctrl+i"),
            Step::Run(Action::ConsolePassEveryKey)
        );
    }

    #[test]
    fn keys_that_share_their_first_key_are_shown_together() {
        let moves = [
            Action::WindowLeft,
            Action::WindowDown,
            Action::WindowUp,
            Action::WindowRight,
        ];
        let map = Keymap::defaults();
        assert_eq!(
            map.keys_for_all(Context::Global, &moves),
            Some("Ctrl+w h j k l".into())
        );
        let map = Keymap::with_user(
            "[global]\n\"Ctrl+w h\" = \"none\"\n\"Ctrl+w ArrowLeft\" = \"none\"\n\"Alt+h\" = \"window.left\"\n",
        )
        .unwrap();
        assert_eq!(
            map.keys_for_all(Context::Global, &moves),
            Some("Alt+h Ctrl+w j Ctrl+w k Ctrl+w l".into())
        );
        let map = Keymap::with_user(
            "[global]\n\"Ctrl+w l\" = \"none\"\n\"Ctrl+w ArrowRight\" = \"none\"\n",
        )
        .unwrap();
        assert_eq!(map.keys_for_all(Context::Global, &moves), None);
    }

    #[test]
    fn the_specification_lists_the_bindings_adroit_ships() {
        let spec = include_str!("../../../docs/spec/keys.md");
        assert!(spec.contains(DEFAULTS.trim()));
    }

    #[test]
    fn every_default_binding_reads() {
        // The defaults are text, so a mistake in them shows here rather than
        // as a key that silently does nothing.
        Keymap::try_defaults().unwrap();
    }
}
