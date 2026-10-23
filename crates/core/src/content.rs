//! The content as the window holds it: its tabs and the windows they split
//! into, what each window shows, the worktree in the middle, which area has
//! the keys, and the options a draft starts with.

pub mod draft;
mod focus;
mod layout;
mod listing;
mod naming;
mod tab;
mod windows;

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use adroit_model::{Agent, TerminalProgram, Tree};
use adroit_protocol::{Seq, WorkSnapshot};

use crate::effect::Effect;
use crate::layout::Layout;
use crate::tree::nav::Place;

pub use focus::*;
pub use layout::*;
pub use listing::newly_waiting;
pub(crate) use listing::{listed, tell_live};
pub use naming::*;
pub use tab::*;
pub(crate) use windows::*;

#[derive(Debug, Clone, PartialEq)]
pub struct ContentView {
    /// The tabs, each split into windows that each show one thing.
    pub windows: Content<Tab>,
    /// The worktree, or the directory outside git, the middle is of.
    pub place: Option<Place>,
    /// The area that has the keys.
    pub focus: Area,
    /// Where the keys were before where they are now.
    pub came_from: Option<Spot>,
    /// The session last shown in each workspace, by its root, which the
    /// next workspace's session goes back to.
    pub last_shown: HashMap<PathBuf, String>,
    /// A border between windows being dragged.
    pub drag: Option<WindowDrag>,
    pub drafts: Drafts,
    /// Where a session started with nothing open runs.
    scratch: PathBuf,
    /// Where the keys are.
    spot: Spot,
    /// The tab shown when the window last closed, until the first listing
    /// gives it its chance to come back.
    restore: Option<Tab>,
    /// Whether the first listing was looked at, and so whether what is shown
    /// is kept for the next run and what starts waiting is told of.
    pub restored: bool,
    /// The tree as last looked at, against which the next listing is.
    looked_at: Tree,
    /// What the listing was last looked at with.
    looked: Option<(Seq, BTreeSet<String>)>,
    /// The sessions shown live here, or being attached to: one started a
    /// moment ago may not be listed yet.
    live: BTreeSet<String>,
    /// What the kept tab was last made to say.
    kept: Option<Option<Tab>>,
    /// The tab the place last followed.
    placed_for: Option<Tab>,
    /// The tab the focus last settled for.
    settled_for: Option<Tab>,
    /// Whether each side panel was shown when last looked at.
    panels_seen: Option<(bool, bool)>,
}

impl Default for ContentView {
    fn default() -> Self {
        Self {
            windows: Content::default(),
            place: None,
            focus: Area::Sidebar,
            came_from: None,
            last_shown: HashMap::new(),
            drag: None,
            drafts: Drafts::default(),
            scratch: PathBuf::new(),
            spot: Spot::SIDEBAR,
            restore: None,
            restored: false,
            looked_at: Tree::default(),
            looked: None,
            live: BTreeSet::new(),
            kept: None,
            placed_for: None,
            settled_for: None,
            panels_seen: None,
        }
    }
}

/// The options chosen for each draft shown, those chosen with nothing open
/// for the next draft, and the agent last chosen.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Drafts {
    pub options: HashMap<Tab, DraftOptions>,
    pub next: Option<DraftOptions>,
    pub last_agent: Option<Agent>,
}

/// A border between windows being dragged: where the pointer was pressed,
/// along the border's axis, and how long the split it divides is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowDrag {
    pub border: Border,
    pub from: f64,
    pub px: f64,
}

/// What the content's focused window shows, and the worktree it is in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Showing {
    pub tab: Option<Tab>,
    pub place: Option<Place>,
}

/// A shell or a Neovim to start: where, on which file, in which window, and
/// whether the keys move to it.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalAsk {
    pub place: Place,
    pub runs: TerminalProgram,
    pub file: Option<PathBuf>,
    pub opening: Opening,
    pub focus: bool,
}

impl ContentView {
    /// What the focused window shows.
    pub fn active(&self) -> Option<&Tab> {
        self.windows.focused()
    }

    pub fn showing(&self) -> Showing {
        Showing {
            tab: self.active().cloned(),
            place: self.place.clone(),
        }
    }

    /// The worktree the details describe and the header names: the one in
    /// the middle, while a window shows something there.
    pub fn described(&self) -> Option<&Place> {
        self.place.as_ref().filter(|_| self.active().is_some())
    }

    /// Where a session started with nothing open runs.
    pub fn scratch(&self) -> &std::path::Path {
        &self.scratch
    }
}

/// The window opened, with the tab it showed when it last closed and where a
/// scratch session runs.
pub(crate) fn opened(view: &mut ContentView, kept: Option<Tab>, scratch: PathBuf) -> Vec<Effect> {
    view.restore = kept;
    view.scratch = scratch;
    Vec::new()
}

pub(crate) fn focus(view: &mut ContentView, area: Area) -> Vec<Effect> {
    view.focus = area;
    Vec::new()
}

/// Show `thing` where the user is, or as `how` says, in the worktree
/// `place` when one is given.
pub(crate) fn show(
    view: &mut ContentView,
    thing: Tab,
    place: Option<Place>,
    how: Option<Opening>,
) -> Vec<Effect> {
    if place.is_some() {
        view.place = place;
    }
    match how {
        Some(how) => view.windows.open_in(thing, how),
        None => view.windows.open(thing),
    }
    Vec::new()
}

/// A new session's draft in `worktree`, with the options chosen for the next
/// one.
pub(crate) fn draft_at(view: &mut ContentView, worktree: PathBuf) -> Tab {
    let draft = Tab::draft(worktree);
    view.drafts.options.insert(
        draft.clone(),
        DraftOptions::for_new_draft(&mut view.drafts.next),
    );
    draft
}

/// A new session's draft shown in `worktree`, which the middle is then of;
/// `write` gives the keys to the prompt to write its first prompt.
pub(crate) fn draft(view: &mut ContentView, worktree: PathBuf, write: bool) -> Vec<Effect> {
    let draft = draft_at(view, worktree.clone());
    view.place = Some(Place { worktree });
    view.windows.open(draft);
    if write {
        view.focus = Area::Prompt;
        return vec![Effect::StartWriting];
    }
    Vec::new()
}

/// `from` shows as `to` in every window, as a draft does once it starts.
pub(crate) fn replace(view: &mut ContentView, from: &Tab, to: Tab) -> Vec<Effect> {
    view.windows.replace(from, to);
    Vec::new()
}

/// Close windows by `f`. The prompt's text for a draft no window shows any
/// longer goes to what the focused window shows next, or to nothing open
/// when that has no prompt.
pub(crate) fn closing(view: &mut ContentView, f: impl FnOnce(&mut Content<Tab>)) -> Vec<Effect> {
    let was = view.active().cloned();
    f(&mut view.windows);
    match was {
        Some(draft @ Tab::Draft { .. }) if !view.windows.shows_anywhere(&draft) => {
            // A shell has no prompt to take it; it waits with nothing open.
            let to = view.active().cloned().filter(|t| shows_prompt(Some(t)));
            vec![Effect::PromptClose(to)]
        }
        _ => Vec::new(),
    }
}

/// Close every window showing what `gone` says is gone, and show `after`
/// once nothing is left open; whether the focused window now shows
/// something else.
pub(crate) fn close_showing(
    view: &mut ContentView,
    gone: impl Fn(&Tab) -> bool,
    after: Option<Tab>,
) -> bool {
    let was = view.active().cloned();
    view.windows.close_showing(gone);
    if view.windows.is_empty()
        && let Some(next) = after
    {
        view.windows.open(next);
    }
    view.active() != was.as_ref()
}

/// The shell `id` closed: its windows close, and another thing of its
/// worktree shows if no window is left. The prompt's text goes with the
/// focus, as when a draft's tab closes.
pub(crate) fn shell_closed(
    view: &mut ContentView,
    work: Option<&WorkSnapshot>,
    id: u128,
) -> Vec<Effect> {
    let shell = Tab::Terminal(id);
    if !view.windows.shows_anywhere(&shell) {
        return Vec::new();
    }
    // The host's list holds the shell until it is told it closed.
    let next = view.place.as_ref().and_then(|p| {
        let left: Vec<u128> = crate::tree::terminals(work)
            .iter()
            .filter(|s| s.id != id && s.cwd == p.worktree)
            .map(|s| s.id)
            .collect();
        crate::tree::nav::tab_after_closing(crate::tree::listed(work), p, &left)
    });
    if close_showing(view, |t| *t == shell, next) {
        return vec![Effect::PromptClose(view.active().cloned())];
    }
    Vec::new()
}

pub(crate) fn drag_start(
    view: &mut ContentView,
    border: Border,
    from: f64,
    px: f64,
) -> Vec<Effect> {
    if px > 0.0 {
        view.drag = Some(WindowDrag { border, from, px });
    }
    Vec::new()
}

/// The pointer, dragging a border, moved to `pos` along its axis.
pub(crate) fn drag_move(view: &mut ContentView, pos: f64) -> Vec<Effect> {
    if let Some(d) = view.drag.clone() {
        let to = d.border.at + (pos - d.from) / d.px;
        view.windows.drag(&d.border.split, d.border.after, to);
    }
    Vec::new()
}

pub(crate) fn drag_end(view: &mut ContentView) -> Vec<Effect> {
    view.drag = None;
    Vec::new()
}

/// The agent chosen for the draft `tab`, or for the next draft with none.
pub(crate) fn choose_agent(view: &mut ContentView, tab: Option<Tab>, agent: Agent) -> Vec<Effect> {
    let drafts = &mut view.drafts;
    draft::choose(&mut drafts.options, tab, agent, &mut drafts.last_agent);
    Vec::new()
}

/// The options chosen for the draft `tab`, or for the next draft with none.
pub(crate) fn choose_options(
    view: &mut ContentView,
    tab: Option<Tab>,
    options: DraftOptions,
) -> Vec<Effect> {
    match tab {
        Some(tab) => {
            view.drafts.options.insert(tab, options);
        }
        None => view.drafts.next = Some(options),
    }
    Vec::new()
}

/// The worktree in the middle follows what the focused window shows, however
/// the focus came to it.
pub(crate) fn follow_place(view: &mut ContentView, work: Option<&WorkSnapshot>) {
    let now = view.active().cloned();
    if now == view.placed_for {
        return;
    }
    view.placed_for = now.clone();
    let Some(now) = now else {
        return;
    };
    let at = crate::tree::place_of_tab(
        crate::tree::listed(work),
        crate::tree::terminals(work),
        &now,
    );
    if at.is_some() && view.place != at {
        view.place = at;
    }
}

/// The session shown is remembered as its workspace's last.
pub(crate) fn remember_shown(view: &mut ContentView, work: Option<&WorkSnapshot>) {
    let Some(Tab::Session(id)) = view.active() else {
        return;
    };
    let Some(root) = crate::tree::listed(work)
        .find(id)
        .map(|(ws, _, _)| ws.root.clone())
    else {
        return;
    };
    if view.last_shown.get(&root) != Some(id) {
        let id = id.clone();
        view.last_shown.insert(root, id);
    }
}

/// A draft no window shows any longer is given up, with what was chosen for
/// it.
pub(crate) fn forget_drafts(view: &mut ContentView) {
    let shown = view.windows.shown();
    if view.drafts.options.keys().any(|t| !shown.contains(&t)) {
        let shown: Vec<Tab> = shown.into_iter().cloned().collect();
        view.drafts.options.retain(|t, _| shown.contains(t));
    }
}

/// What the focused window shows is kept for the next run, once the tab kept
/// from the last run had its chance to come back.
pub(crate) fn keep_shown(view: &mut ContentView) -> Vec<Effect> {
    if !view.restored {
        return Vec::new();
    }
    let now = view.active().cloned();
    if view.kept.as_ref() == Some(&now) {
        return Vec::new();
    }
    view.kept = Some(now.clone());
    vec![Effect::KeepShown(now)]
}

/// Hiding the panel that has the keys gives them to the console.
pub(crate) fn follow_panels(view: &mut ContentView, layout: &Layout) {
    let now = (layout.show_sidebar, layout.show_details);
    let was = view.panels_seen.replace(now);
    let Some((sidebar, details)) = was else {
        return;
    };
    let hidden = (sidebar && !now.0 && view.focus == Area::Sidebar)
        || (details && !now.1 && view.focus == Area::Details);
    if hidden {
        view.focus = Area::Console;
    }
}

/// A shell shown takes the keys from a prompt that goes away with it.
pub(crate) fn settle_focus(view: &mut ContentView) {
    let now = view.active().cloned();
    if now == view.settled_for {
        return;
    }
    view.settled_for = now;
    let to = settled(view.focus, shows_prompt(view.active()));
    if to != view.focus {
        view.focus = to;
    }
}

/// Where the keys were is remembered as they move; whether they came into
/// the prompt from elsewhere.
pub(crate) fn follow_spot(view: &mut ContentView) -> bool {
    let spot = spot_of(view.focus, &view.windows);
    let before = view.spot;
    if before == spot {
        return false;
    }
    view.spot = spot;
    view.came_from = Some(before);
    view.focus == Area::Prompt && before.area != Area::Prompt
}

/// Where the keys are, in `area`: in the console or the prompt of the
/// content's focused window, or in a side panel.
pub fn spot_of(area: Area, content: &Content<Tab>) -> Spot {
    match area {
        Area::Console | Area::Prompt => Spot {
            area,
            window: content.focused_window(),
        },
        Area::Sidebar => Spot::SIDEBAR,
        Area::Details => Spot::DETAILS,
    }
}

#[cfg(test)]
mod tests;
