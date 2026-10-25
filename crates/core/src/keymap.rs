//! Which action a key runs, from the bindings adroit ships and the user's own.

use std::fmt;

use adroit_model::Key;

use crate::actions::Action;

mod chord;

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
    /// Files and directories: the tree's, with a file or a directory
    /// selected, before its own bindings, and the picker's.
    Filer,
}

impl Context {
    pub const ALL: [Context; 8] = [
        Context::Global,
        Context::Tree,
        Context::Filer,
        Context::Details,
        Context::Console,
        Context::Prompt,
        Context::Draft,
        Context::Viewer,
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
            Context::Filer => "filer",
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
            Context::Filer => "Files",
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
            Context::Filer => {
                "The tree's files and directories, before the tree's bindings, and the picker's directories."
            }
        }
    }
}

/// Names earlier versions gave an area or an action, read as the names they
/// go by now.
const RENAMED: &[(&str, &str)] = &[("sidebar", "tree"), ("sidebar.toggle", "tree.toggle")];

fn renamed(name: &str) -> &str {
    RENAMED
        .iter()
        .find(|(old, _)| *old == name)
        .map_or(name, |(_, now)| now)
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
    "session.take_over",
    "session.agent_next",
    "session.agent_prev",
];

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
                .find(|c| c.name() == renamed(&area))
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
                let action: Action = renamed(name).parse()?;
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
    fn the_bindings_are_listed_by_area_with_the_user_s_in_place_of_the_defaults() {
        let map = Keymap::with_user("[tree]\n\"j\" = \"none\"\n\"J\" = \"cursor.down\"\n").unwrap();
        let listed = map.listed();
        assert!(listed.contains(&("tree", "J".into(), "cursor.down".into())));
        assert!(!listed.iter().any(|(a, k, _)| *a == "tree" && k == "j"));
        let areas: Vec<&str> = listed.iter().map(|(a, _, _)| *a).collect();
        let first_sidebar = areas.iter().position(|a| *a == "tree").unwrap();
        assert!(areas[..first_sidebar].iter().all(|a| *a == "global"));
    }

    #[test]
    fn keys_already_bound_in_the_area_or_starting_a_longer_binding_are_told() {
        let map = Keymap::defaults();
        // `j` is cursor.down in the sidebar.
        assert_eq!(
            map.clashes(Context::Tree, "j", None),
            vec!["j is cursor.down".to_string()]
        );
        // Editing that binding itself is no clash.
        assert!(map.clashes(Context::Tree, "j", Some("j")).is_empty());
        // `g` alone would cut `g g` and `g t` short.
        let g = map.clashes(Context::Tree, "g", None);
        assert!(g.iter().any(|c| c == "g g is cursor.first"), "{g:?}");
        // Over a global binding, the area's wins there.
        let w = map.clashes(Context::Tree, "Ctrl+q h", None);
        assert_eq!(
            w,
            vec!["Ctrl+q h is window.left everywhere; this one wins in tree".to_string()]
        );
        assert!(map.clashes(Context::Tree, "Ctrl+w", None).is_empty());
    }

    #[test]
    fn the_window_s_own_mod_keys_are_global_bindings() {
        let map = Keymap::defaults();
        for (key, action) in [
            ("Mod+w", Action::TabClose),
            ("Mod+p", Action::SessionPick),
            ("Mod+t", Action::SessionNew),
            ("Mod+n", Action::WindowOpen),
            ("Mod+o", Action::SessionResume),
            ("Mod+j", Action::WaitingNext),
            ("Mod+k", Action::WaitingPrev),
            ("Mod+s", Action::ShellOpen),
            ("Mod+e", Action::NeovimOpen),
            ("Mod+Backspace", Action::SessionStop),
            ("Mod+f", Action::Filter),
            ("Mod+r", Action::Refresh),
            ("Mod+,", Action::SettingsOpen),
            ("Mod+=", Action::ZoomIn),
            ("Mod+Plus", Action::ZoomIn),
            ("Mod+-", Action::ZoomOut),
            ("Mod+0", Action::ZoomReset),
            ("Mod+b", Action::TreeToggle),
            ("Mod+d", Action::DetailsToggle),
        ] {
            assert_eq!(
                step(&map, &[Context::Tree], &[], key),
                Step::Run(action),
                "{key}"
            );
        }
        assert_eq!(
            step(&map, &[Context::Tree], &[], "Cmd+W"),
            Step::Run(Action::WindowClose)
        );
    }

    #[test]
    fn a_mod_key_resolves_among_the_global_bindings_wherever_the_keys_are() {
        let map = Keymap::defaults();
        // In the prompt's insert mode the window reads the prompt's alone.
        assert_eq!(
            step(&map, &[Context::Prompt], &[], "Mod+s"),
            Step::Run(Action::ShellOpen)
        );
    }

    #[test]
    fn the_keys_the_menu_alone_offers_now_have_no_default() {
        let map = Keymap::defaults();
        for areas in [&[Context::Tree, Context::Global][..], &[Context::Global]] {
            for key in [
                "Cmd+Alt+n",
                "Cmd+N",
                "Mod+]",
                "Mod+[",
                "Cmd+Alt+e",
                "Cmd+Alt+b",
            ] {
                assert_eq!(step(&map, areas, &[], key), Step::Nothing, "{key}");
            }
        }
        for action in [Action::SessionScratch, Action::WorktreesToggle] {
            assert_eq!(map.keys_for(Context::Global, &action), None, "{action}");
        }
        // The sidebar keeps its own key for every worktree.
        assert_eq!(
            map.keys_for(Context::Tree, &Action::WorktreesToggle),
            Some("!".into())
        );
    }

    #[test]
    fn a_bound_key_runs_its_action() {
        let map = Keymap::defaults();
        assert_eq!(
            step(&map, &[Context::Tree], &[], "j"),
            Step::Run(Action::CursorDown)
        );
    }

    #[test]
    fn the_start_of_a_sequence_waits_for_the_rest() {
        let map = Keymap::defaults();
        assert_eq!(step(&map, &[Context::Tree], &[], "g"), Step::Wait);
        assert_eq!(
            step(&map, &[Context::Tree], &["g"], "g"),
            Step::Run(Action::CursorFirst)
        );
        assert_eq!(step(&map, &[Context::Tree], &["g"], "z"), Step::Nothing);
    }

    #[test]
    fn a_key_bound_nowhere_in_the_areas_does_nothing() {
        let map = Keymap::defaults();
        assert_eq!(step(&map, &[Context::Tree], &[], "z"), Step::Nothing);
    }

    #[test]
    fn an_earlier_area_wins_over_a_later_one() {
        let map = Keymap::with_user("[details]\n\"n\" = \"refresh\"\n").unwrap();
        let both = [Context::Details, Context::Tree];
        assert_eq!(step(&map, &both, &[], "n"), Step::Run(Action::Refresh));
        assert_eq!(
            step(&map, &[Context::Tree], &[], "n"),
            Step::Run(Action::SessionNew)
        );
        // A key the details panel leaves alone falls through to the sidebar's.
        assert_eq!(step(&map, &both, &[], "N"), Step::Nothing);
    }

    #[test]
    fn the_details_panel_moves_opens_and_goes_back_with_the_cursor_and_scrolls_by_half() {
        let map = Keymap::defaults();
        let areas = [Context::Details, Context::Tree];
        for (key, action) in [
            ("j", Action::CursorDown),
            ("ArrowDown", Action::CursorDown),
            ("k", Action::CursorUp),
            ("ArrowUp", Action::CursorUp),
            ("G", Action::CursorLast),
            ("Enter", Action::CursorGo),
            ("l", Action::CursorShow),
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
    fn the_tabs_are_switched_by_the_keys_each_platform_and_vim_switch_tabs_with() {
        let map = Keymap::defaults();
        for ctx in [
            Context::Tree,
            Context::Details,
            Context::Console,
            Context::Prompt,
        ] {
            let areas = [ctx, Context::Global];
            for (pending, key, action) in [
                (&[][..], "Ctrl+Tab", Action::TabNext),
                (&[], "Ctrl+Shift+Tab", Action::TabPrev),
                (&[], "Ctrl+PageDown", Action::TabNext),
                (&[], "Ctrl+PageUp", Action::TabPrev),
                (&[], "Cmd+}", Action::TabNext),
                (&[], "Cmd+{", Action::TabPrev),
                (&["Ctrl+q", "g"], "t", Action::TabNext),
                (&["Ctrl+q", "g"], "T", Action::TabPrev),
            ] {
                assert_eq!(
                    step(&map, &areas, pending, key),
                    Step::Run(action),
                    "{} {pending:?} {key}",
                    ctx.name()
                );
            }
        }
    }

    #[test]
    fn a_ctrl_key_bound_alone_everywhere_reaches_the_window_from_the_prompt_s_insert_mode() {
        let map = Keymap::defaults();
        let inserting = [Context::Prompt];
        assert_eq!(
            step(&map, &inserting, &[], "Ctrl+Tab"),
            Step::Run(Action::TabNext)
        );
        assert_eq!(
            step(&map, &inserting, &[], "Ctrl+PageUp"),
            Step::Run(Action::TabPrev)
        );
        // Other Ctrl keys stay Neovim's.
        assert_eq!(step(&map, &inserting, &[], "Ctrl+w"), Step::Nothing);
    }

    #[test]
    fn any_session_is_a_key_away_from_every_area() {
        let map = Keymap::defaults();
        for ctx in [
            Context::Tree,
            Context::Details,
            Context::Console,
            Context::Prompt,
        ] {
            let areas = [ctx, Context::Global];
            assert_eq!(
                step(&map, &areas, &[], "Mod+p"),
                Step::Run(Action::SessionPick)
            );
        }
        // Stepping through every session one by one is left to be bound.
        for action in [Action::SessionNext, Action::SessionPrev] {
            assert!(
                Context::ALL
                    .iter()
                    .all(|&c| map.keys_for(c, &action).is_none()),
                "{action}"
            );
        }
        // Command with Shift and a bracket arrives as the brace it types.
        assert_eq!(
            Chord::pressed("}", false, false, true, true),
            Some("Cmd+}".into())
        );
        // Option changes the letter typed; the window reports the key's own.
        assert_eq!(
            Chord::pressed("n", false, true, false, true),
            Some("Cmd+Alt+n".into())
        );
    }

    #[test]
    fn another_workspace_is_a_key_away_from_every_area() {
        let map = Keymap::defaults();
        for ctx in [
            Context::Tree,
            Context::Details,
            Context::Console,
            Context::Prompt,
        ] {
            let areas = [ctx, Context::Global];
            assert_eq!(
                step(&map, &areas, &["Ctrl+q"], "}"),
                Step::Run(Action::WorkspaceNext)
            );
            assert_eq!(
                step(&map, &areas, &["Ctrl+q"], "{"),
                Step::Run(Action::WorkspacePrev)
            );
        }
        let tree = [Context::Tree, Context::Global];
        assert_eq!(
            step(&map, &tree, &[], "}"),
            Step::Run(Action::WorkspaceNext)
        );
        assert_eq!(
            step(&map, &tree, &[], "{"),
            Step::Run(Action::WorkspacePrev)
        );
    }

    #[test]
    fn each_sheet_of_the_details_panel_has_a_digit_of_its_own_there() {
        let map = Keymap::defaults();
        let details = [Context::Details, Context::Tree, Context::Global];
        for (digit, action) in [
            ("1", Action::DetailsOverview),
            ("2", Action::DetailsChanges),
            ("3", Action::DetailsLog),
            ("4", Action::DetailsAgents),
            ("5", Action::DetailsResources),
        ] {
            assert_eq!(
                step(&map, &details, &[], digit),
                Step::Run(action),
                "{digit}"
            );
        }
    }

    #[test]
    fn mod_and_a_digit_shows_the_tab_of_that_number_as_browsers_do() {
        let map = Keymap::defaults();
        for ctx in [
            Context::Tree,
            Context::Details,
            Context::Console,
            Context::Prompt,
        ] {
            for n in 1..=9 {
                assert_eq!(
                    map.resolve(&[ctx], &[], &press(&format!("Mod+{n}"))),
                    Step::Run(Action::TabGo(n)),
                    "{} {n}",
                    ctx.name()
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
        for (digit, n) in [("4", 4), ("5", 5)] {
            assert_eq!(
                map.resolve(&[Context::Global], &[], &Key::new(digit).meta()),
                Step::Run(Action::TabGo(n)),
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
        for ctx in [Context::Tree, Context::Details, Context::Console] {
            assert_eq!(
                step(&map, &[ctx, Context::Global], &["Ctrl+q"], "l"),
                Step::Run(Action::WindowRight)
            );
        }
    }

    #[test]
    fn the_user_s_binding_replaces_the_default_for_the_same_keys() {
        let map = Keymap::with_user("[tree]\n\"j\" = \"cursor.up\"\n").unwrap();
        assert_eq!(
            step(&map, &[Context::Tree], &[], "j"),
            Step::Run(Action::CursorUp)
        );
        assert_eq!(
            step(&map, &[Context::Tree], &[], "k"),
            Step::Run(Action::CursorUp)
        );
    }

    #[test]
    fn the_user_can_add_keys_to_an_action() {
        let map = Keymap::with_user("[tree]\n\"Ctrl+n\" = \"cursor.down\"\n").unwrap();
        assert_eq!(
            step(&map, &[Context::Tree], &[], "Ctrl+n"),
            Step::Run(Action::CursorDown)
        );
        assert_eq!(
            step(&map, &[Context::Tree], &[], "j"),
            Step::Run(Action::CursorDown)
        );
    }

    #[test]
    fn none_takes_a_key_away_even_from_later_areas() {
        let map = Keymap::with_user("[console]\n\"Ctrl+q\" = \"none\"\n").unwrap();
        assert_eq!(
            step(&map, &[Context::Console, Context::Global], &[], "Ctrl+q"),
            Step::Run(Action::None)
        );
    }

    #[test]
    fn a_binding_to_an_action_earlier_versions_had_is_read_and_dropped() {
        let map = Keymap::with_user(
            "[tree]\n\"P\" = \"project.new\"\n\"c\" = \"coordinator.open\"\n\"T\" = \"session.take_over\"\n\"J\" = \"cursor.down\"\n",
        )
        .unwrap();
        assert_eq!(
            step(&map, &[Context::Tree], &[], "J"),
            Step::Run(Action::CursorDown)
        );
        assert_eq!(step(&map, &[Context::Tree], &[], "P"), Step::Nothing);
        assert_eq!(step(&map, &[Context::Tree], &[], "T"), Step::Nothing);
    }

    #[test]
    fn a_viewer_scrolls_zooms_turns_pages_plays_and_closes_by_its_own_keys() {
        let map = Keymap::defaults();
        let viewer = [Context::Viewer, Context::Global];
        for (keys, action) in [
            ("j", Action::ScrollDown),
            ("Ctrl+u", Action::ScrollHalfUp),
            ("G", Action::ScrollBottom),
            ("Plus", Action::ViewerZoomIn),
            ("=", Action::ViewerZoomIn),
            ("-", Action::ViewerZoomOut),
            ("0", Action::ViewerZoomFit),
            ("J", Action::ViewerPageNext),
            ("K", Action::ViewerPagePrev),
            ("Space", Action::ViewerPlay),
            ("h", Action::ViewerBack),
            ("l", Action::ViewerForward),
            ("y", Action::ViewerCopyPath),
            ("q", Action::WindowQuit),
        ] {
            assert_eq!(step(&map, &viewer, &[], keys), Step::Run(action), "{keys}");
        }
        assert_eq!(
            step(&map, &viewer, &["g"], "g"),
            Step::Run(Action::ScrollTop)
        );
    }

    #[test]
    fn the_tree_s_bindings_are_read_from_the_area_earlier_versions_called_sidebar() {
        let map = Keymap::with_user("[sidebar]\n\"J\" = \"cursor.down\"\n").unwrap();
        assert_eq!(
            step(&map, &[Context::Tree], &[], "J"),
            Step::Run(Action::CursorDown)
        );
        assert!(map.listed().iter().all(|(area, _, _)| *area != "sidebar"));
    }

    #[test]
    fn a_binding_to_the_sidebar_toggle_earlier_versions_had_toggles_the_tree() {
        let map = Keymap::with_user("[global]\n\"Mod+y\" = \"sidebar.toggle\"\n").unwrap();
        assert_eq!(
            step(&map, &[Context::Global], &[], "Mod+y"),
            Step::Run(Action::TreeToggle)
        );
        assert!(!Action::names().any(|n| n == "sidebar.toggle"));
    }

    #[test]
    fn a_binding_to_the_workspace_picker_earlier_versions_had_is_dropped() {
        let map = Keymap::with_user("[global]\n\"Cmd+y\" = \"workspace.pick\"\n").unwrap();
        let key = Key::new("y").meta();
        assert_eq!(map.resolve(&[Context::Global], &[], &key), Step::Nothing);
        assert!(!Action::names().any(|n| n == "workspace.pick"));
    }

    #[test]
    fn w_in_the_sidebar_opens_the_one_picker_and_cmd_o_opens_a_past_conversation() {
        let map = Keymap::defaults();
        assert_eq!(
            step(&map, &[Context::Tree, Context::Global], &[], "w"),
            Step::Run(Action::SessionPick)
        );
        let o = Key::new("o").meta();
        assert_eq!(
            map.resolve(&[Context::Global], &[], &o),
            Step::Run(Action::SessionResume)
        );
    }

    #[test]
    fn an_unknown_area_key_or_action_in_the_user_s_file_is_reported() {
        for bad in [
            "[side]\n\"j\" = \"cursor.down\"\n",
            "[tree]\n\"Hyper+j\" = \"cursor.down\"\n",
            "[tree]\n\"j\" = \"cursor.sideways\"\n",
            "[tree]\n\"j\" = 1\n",
        ] {
            assert!(Keymap::with_user(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_keys_of_an_action_read_as_written_for_the_window_to_show() {
        let map = Keymap::defaults();
        assert_eq!(
            map.keys_for(Context::Tree, &Action::CursorFirst),
            Some("g g".into())
        );
        assert_eq!(
            map.keys_for(Context::Tree, &Action::SessionStop),
            Some("X".into())
        );
        let map =
            Keymap::with_user("[tree]\n\"X\" = \"none\"\n\"Ctrl+x\" = \"session.stop\"\n").unwrap();
        assert_eq!(
            map.keys_for(Context::Tree, &Action::SessionStop),
            Some("Ctrl+x".into())
        );
    }

    #[test]
    fn an_action_s_keys_are_found_only_in_the_areas_they_work_in() {
        let map = Keymap::defaults();
        let sidebar = [Context::Tree, Context::Global];
        let console = [Context::Console, Context::Global];
        assert_eq!(
            map.keys_in(&sidebar, &Action::WorkspaceNext),
            Some("}".into())
        );
        assert_eq!(
            map.keys_in(&console, &Action::WorkspaceNext),
            Some("Ctrl+q }".into())
        );
        assert_eq!(map.keys_in(&console, &Action::CursorDown), None);
        let map = Keymap::with_user("[global]\n\"Ctrl+q j\" = \"cursor.down\"\n").unwrap();
        assert_eq!(
            map.keys_in(&console, &Action::CursorDown),
            Some("Ctrl+q j".into())
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
    fn prompt_attachments_have_a_named_selection_action() {
        assert_eq!(
            step(&Keymap::defaults(), &[Context::Prompt], &[], "Mod+i"),
            Step::Run(Action::PromptImages)
        );
    }

    #[test]
    fn ctrl_c_in_the_prompt_sends_escape_to_the_agent() {
        assert_eq!(
            step(&Keymap::defaults(), &[Context::Prompt], &[], "Ctrl+c"),
            Step::Run(Action::ConsoleSend(Some(Chord::parse("Escape").unwrap())))
        );
    }

    #[test]
    fn in_the_console_keys_that_start_writing_go_to_the_prompt() {
        for k in ["i", "a", "I", "A", "o", "O"] {
            assert_eq!(console(&[], k), Step::Run(Action::PromptWrite), "{k}");
        }
    }

    #[test]
    fn a_slash_in_the_sidebar_starts_a_filter() {
        let sidebar = [Context::Tree, Context::Global];
        assert_eq!(
            step(&Keymap::defaults(), &sidebar, &[], "/"),
            Step::Run(Action::Filter)
        );
    }

    #[test]
    fn in_the_sidebar_l_and_enter_with_shift_or_ctrl_look_at_the_row_and_keep_the_keys() {
        let map = Keymap::defaults();
        let sidebar = [Context::Tree, Context::Global];
        for k in ["l", "L", "Shift+Enter", "Ctrl+Enter"] {
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
    }

    #[test]
    fn the_filer_moves_as_fern_does() {
        let map = Keymap::defaults();
        let files = [Context::Filer, Context::Tree, Context::Global];
        for (key, action) in [
            ("j", Action::FilerDown),
            ("ArrowDown", Action::FilerDown),
            ("k", Action::FilerUp),
            ("ArrowUp", Action::FilerUp),
            ("l", Action::FilerInto),
            ("ArrowRight", Action::FilerInto),
            ("h", Action::FilerOut),
            ("ArrowLeft", Action::FilerOut),
            ("Enter", Action::FilerOpen),
            ("!", Action::FilerHidden),
            // What the filer leaves unbound is the tree's.
            ("s", Action::CursorSplit),
        ] {
            assert_eq!(step(&map, &files, &[], key), Step::Run(action), "{key}");
        }
    }

    #[test]
    fn in_the_tree_the_key_says_where_the_row_opens() {
        let map = Keymap::defaults();
        let tree = [Context::Tree, Context::Global];
        for (key, action) in [
            ("Enter", Action::CursorGo),
            ("o", Action::CursorGo),
            ("l", Action::CursorShow),
            ("s", Action::CursorSplit),
            ("v", Action::CursorVsplit),
            ("t", Action::CursorTab),
        ] {
            assert_eq!(step(&map, &tree, &[], key), Step::Run(action), "{key}");
        }
    }

    #[test]
    fn in_the_tree_what_starts_beside_a_row_moved_off_the_keys_that_open_it() {
        let map = Keymap::defaults();
        let tree = [Context::Tree, Context::Global];
        for (key, action) in [
            ("$", Action::ShellOpen),
            ("e", Action::NeovimOpen),
            ("O", Action::SessionResume),
            ("n", Action::SessionNew),
        ] {
            assert_eq!(step(&map, &tree, &[], key), Step::Run(action), "{key}");
        }
    }

    #[test]
    fn in_the_tree_x_closes_the_row_s_windows_capital_x_stops_and_y_copies() {
        let map = Keymap::defaults();
        let sidebar = [Context::Tree, Context::Global];
        let console = [Context::Console, Context::Global];
        assert_eq!(
            step(&map, &sidebar, &[], "x"),
            Step::Run(Action::CursorClose)
        );
        assert_eq!(
            step(&map, &sidebar, &[], "y"),
            Step::Run(Action::CursorCopy)
        );
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
            step(&map, &answering, &[], "Mod+j"),
            Step::Run(Action::WaitingNext)
        );
        assert_eq!(
            step(&map, &answering, &[], "Mod+k"),
            Step::Run(Action::WaitingPrev)
        );
        // From the prompt's insert mode too.
        assert_eq!(
            step(&map, &[Context::Prompt], &[], "Mod+j"),
            Step::Run(Action::WaitingNext)
        );
    }

    #[test]
    fn in_the_sidebar_i_and_a_start_writing_as_in_the_console() {
        for k in ["i", "a"] {
            let got = step(
                &Keymap::defaults(),
                &[Context::Tree, Context::Global],
                &[],
                k,
            );
            assert_eq!(got, Step::Run(Action::PromptWrite), "{k}");
        }
    }

    #[test]
    fn a_binding_to_switching_a_draft_s_agent_is_read_and_dropped_now_its_profile_names_it() {
        let map = Keymap::with_user(
            "[draft]\n\"Ctrl+g a\" = \"session.agent_next\"\n\"Ctrl+g A\" = \"session.agent_prev\"\n\"Ctrl+g p\" = \"session.profile_next\"\n",
        )
        .unwrap();
        let draft = [Context::Draft, Context::Prompt, Context::Global];
        for k in ["a", "A"] {
            assert_eq!(step(&map, &draft, &["Ctrl+g"], k), Step::Nothing, "{k}");
        }
        assert_eq!(
            step(&map, &draft, &["Ctrl+g"], "p"),
            Step::Run(Action::SessionProfileNext)
        );
        for k in ["a", "A"] {
            assert_eq!(
                step(&Keymap::defaults(), &draft, &["Ctrl+g"], k),
                Step::Nothing,
                "{k}"
            );
        }
    }

    #[test]
    fn tab_in_the_prompt_takes_a_suggestion_and_shift_tab_goes_to_the_agent_even_while_writing() {
        for areas in [
            &[Context::Prompt][..],
            &[Context::Prompt, Context::Global],
            &[Context::Draft, Context::Prompt],
        ] {
            let prompt = |key| step(&Keymap::defaults(), areas, &[], key);
            assert_eq!(
                prompt("Tab"),
                Step::Run(Action::PromptTakeSuggestion),
                "{areas:?}"
            );
            assert_eq!(
                prompt("Shift+Tab"),
                Step::Run(Action::ConsoleSend(None)),
                "{areas:?}"
            );
        }
        assert_eq!(
            "prompt.take_suggestion".parse(),
            Ok(Action::PromptTakeSuggestion)
        );
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
            Some("Ctrl+q h j k l".into())
        );
        let map = Keymap::with_user(
            "[global]\n\"Ctrl+q h\" = \"none\"\n\"Ctrl+q ArrowLeft\" = \"none\"\n\"Alt+h\" = \"window.left\"\n",
        )
        .unwrap();
        assert_eq!(
            map.keys_for_all(Context::Global, &moves),
            Some("Alt+h Ctrl+q j Ctrl+q k Ctrl+q l".into())
        );
        let map = Keymap::with_user(
            "[global]\n\"Ctrl+q l\" = \"none\"\n\"Ctrl+q ArrowRight\" = \"none\"\n",
        )
        .unwrap();
        assert_eq!(map.keys_for_all(Context::Global, &moves), None);
    }

    #[test]
    fn window_and_tab_commands_begin_with_ctrl_q_in_every_area() {
        let map = Keymap::defaults();
        for ctx in [
            Context::Tree,
            Context::Details,
            Context::Console,
            Context::Prompt,
        ] {
            let areas = [ctx, Context::Global];
            for (key, action) in [
                ("h", Action::WindowLeft),
                ("ArrowLeft", Action::WindowLeft),
                ("j", Action::WindowDown),
                ("ArrowDown", Action::WindowDown),
                ("k", Action::WindowUp),
                ("ArrowUp", Action::WindowUp),
                ("l", Action::WindowRight),
                ("ArrowRight", Action::WindowRight),
                ("q", Action::WindowNext),
                ("Ctrl+q", Action::WindowNext),
                ("Q", Action::WindowPrev),
                ("t", Action::WindowTop),
                ("b", Action::WindowBottom),
                ("p", Action::WindowBack),
            ] {
                assert_eq!(
                    step(&map, &areas, &["Ctrl+q"], key),
                    Step::Run(action),
                    "{} {key}",
                    ctx.name()
                );
            }
            // Keys left for the user's own: `w`, as `q` goes to the next
            // window with Ctrl held or not, and those Vim's Ctrl-w leaves
            // alone.
            for key in ["w", "W", "Tab", "Shift+Tab", "]", "[", "N", "P"] {
                assert_eq!(
                    step(&map, &areas, &["Ctrl+q"], key),
                    Step::Nothing,
                    "{} {key}",
                    ctx.name()
                );
            }
        }
    }

    #[test]
    fn ctrl_q_splits_closes_moves_and_resizes_the_content_s_windows_in_every_area() {
        let map = Keymap::defaults();
        for ctx in [
            Context::Tree,
            Context::Details,
            Context::Console,
            Context::Prompt,
        ] {
            let areas = [ctx, Context::Global];
            for (key, action) in [
                ("v", Action::WindowVsplit),
                ("s", Action::WindowSplit),
                ("n", Action::WindowNew),
                ("c", Action::WindowQuit),
                ("o", Action::WindowOnly),
                ("H", Action::WindowMoveLeft),
                ("J", Action::WindowMoveDown),
                ("K", Action::WindowMoveUp),
                ("L", Action::WindowMoveRight),
                ("x", Action::WindowExchange),
                ("r", Action::WindowRotate),
                ("=", Action::WindowEqualize),
                ("<", Action::WindowNarrower),
                (">", Action::WindowWider),
                ("Plus", Action::WindowTaller),
                ("-", Action::WindowShorter),
                ("_", Action::WindowMaximizeHeight),
                ("|", Action::WindowMaximizeWidth),
                ("T", Action::WindowToTab),
                ("1", Action::TabGo(1)),
                ("9", Action::TabGo(9)),
            ] {
                assert_eq!(
                    step(&map, &areas, &["Ctrl+q"], key),
                    Step::Run(action),
                    "{} {key}",
                    ctx.name()
                );
            }
            assert_eq!(
                step(&map, &areas, &["Ctrl+q", "g"], "Tab"),
                Step::Run(Action::TabLast),
                "{}",
                ctx.name()
            );
        }
    }

    #[test]
    fn a_tab_is_gone_to_by_its_number_from_one_to_nine() {
        assert_eq!("tab.go 3".parse::<Action>(), Ok(Action::TabGo(3)));
        assert_eq!(Action::TabGo(3).to_string(), "tab.go 3");
        for bad in ["tab.go", "tab.go 0", "tab.go 10", "tab.go x"] {
            assert!(bad.parse::<Action>().is_err(), "{bad}");
        }
    }

    #[test]
    fn the_key_after_the_prefix_means_the_same_with_ctrl_held_but_tab() {
        let map = Keymap::defaults();
        let areas = [Context::Console, Context::Global];
        for (key, action) in [
            ("Ctrl+h", Action::WindowLeft),
            ("Ctrl+l", Action::WindowRight),
            ("Ctrl+}", Action::WorkspaceNext),
            ("Ctrl+c", Action::WindowQuit),
            ("Ctrl+Q", Action::WindowPrev),
        ] {
            assert_eq!(
                step(&map, &areas, &["Ctrl+q"], key),
                Step::Run(action),
                "{key}"
            );
        }
        assert_eq!(step(&map, &areas, &["Ctrl+q"], "Ctrl+Tab"), Step::Nothing);
        // A key bound with Ctrl held is its own binding first.
        let map = Keymap::with_user("[global]\n\"Ctrl+q Ctrl+h\" = \"refresh\"\n").unwrap();
        assert_eq!(
            step(&map, &areas, &["Ctrl+q"], "Ctrl+h"),
            Step::Run(Action::Refresh)
        );
    }

    #[test]
    fn ctrl_q_dot_sends_ctrl_q_and_ctrl_w_is_left_to_the_program() {
        let map = Keymap::defaults();
        assert_eq!(
            step(&map, &[Context::Console, Context::Global], &["Ctrl+q"], "."),
            Step::Run(Action::ConsoleSend(Some(Chord::parse("Ctrl+q").unwrap())))
        );
        for areas in [
            &[Context::Tree, Context::Global][..],
            &[Context::Console, Context::Global],
            &[Context::Prompt, Context::Global],
            &[Context::Prompt],
            &[Context::Global],
        ] {
            assert_eq!(step(&map, areas, &[], "Ctrl+w"), Step::Nothing, "{areas:?}");
        }
    }

    #[test]
    fn the_prefix_works_in_the_prompt_s_insert_mode() {
        // In insert mode the window reads the prompt's bindings alone.
        let insert = [Context::Prompt];
        let map = Keymap::defaults();
        assert_eq!(step(&map, &insert, &[], "Ctrl+q"), Step::Wait);
        assert_eq!(
            step(&map, &insert, &["Ctrl+q"], "k"),
            Step::Run(Action::WindowUp)
        );
        // Other global bindings stay out of Neovim's way.
        let map = Keymap::with_user("[global]\n\"Ctrl+x\" = \"refresh\"\n\"g t\" = \"refresh\"\n")
            .unwrap();
        assert_eq!(step(&map, &insert, &[], "Ctrl+x"), Step::Nothing);
        assert_eq!(step(&map, &insert, &[], "g"), Step::Nothing);
    }

    #[test]
    fn a_prefix_of_the_user_s_own_works_as_ctrl_q_does() {
        let map = Keymap::with_user("[global]\n\"Ctrl+x k\" = \"window.up\"\n").unwrap();
        assert_eq!(
            step(&map, &[Context::Prompt], &["Ctrl+x"], "Ctrl+k"),
            Step::Run(Action::WindowUp)
        );
    }

    #[test]
    fn every_default_binding_reads() {
        // The defaults are text, so a mistake in them shows here rather than
        // as a key that silently does nothing.
        Keymap::try_defaults().unwrap();
    }
}
