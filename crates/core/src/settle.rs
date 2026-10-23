//! What must hold across slices after every message: each rule brings one
//! slice in line with the others, and runs in the order listed.

use adroit_protocol::{Request, Sight};

use crate::effect::Effect;
use crate::state::ViewState;
use crate::{chrome, config, console, content, details, overlay, picker, prompt, tree};

type Rule = fn(&mut ViewState, &mut Vec<Effect>);

const RULES: &[Rule] = &[
    config_taken,
    chrome_zoom,
    content_listed,
    content_place,
    content_last_shown,
    content_drafts,
    content_kept,
    content_panels,
    content_focus,
    content_spot,
    prompt_shown,
    console_area,
    console_listed,
    console_live,
    console_freed,
    prompt_recalled,
    prompt_images,
    prompt_offered,
    tree_failing,
    tree_rows,
    tree_kept,
    tree_switched,
    tree_completions,
    tree_filter_left,
    overlay_rename,
    details_shown,
    details_told,
    details_usage,
    details_refreshed,
    details_gone,
    picker_offered,
    picker_listed,
    watched,
];

/// The configuration the host read is the one the window works by.
fn config_taken(state: &mut ViewState, effects: &mut Vec<Effect>) {
    effects.extend(config::take_in(state));
}

/// The window is drawn at the zoom the settings name.
fn chrome_zoom(state: &mut ViewState, effects: &mut Vec<Effect>) {
    let zoom = state.config.settings.zoom;
    effects.extend(chrome::follow_setting(&mut state.chrome, zoom));
}

/// What a new listing means for the windows: they follow their sessions,
/// give up those that ended, and bring back the tab kept from the last run.
fn content_listed(state: &mut ViewState, effects: &mut Vec<Effect>) {
    effects.extend(content::listed(state));
}

/// The worktree in the middle follows what the focused window shows.
fn content_place(state: &mut ViewState, _: &mut Vec<Effect>) {
    content::follow_place(&mut state.content, state.host.work.as_ref());
}

/// The session shown is its workspace's last shown.
fn content_last_shown(state: &mut ViewState, _: &mut Vec<Effect>) {
    content::remember_shown(&mut state.content, state.host.work.as_ref());
}

/// A draft no window shows is given up.
fn content_drafts(state: &mut ViewState, _: &mut Vec<Effect>) {
    content::forget_drafts(&mut state.content);
}

/// What the focused window shows is kept for the next run.
fn content_kept(state: &mut ViewState, effects: &mut Vec<Effect>) {
    effects.extend(content::keep_shown(&mut state.content));
}

/// Hiding the panel that has the keys gives them to the console.
fn content_panels(state: &mut ViewState, _: &mut Vec<Effect>) {
    content::follow_panels(&mut state.content, &state.chrome.panels.layout);
}

/// A shell shown takes the keys from a prompt that goes away with it.
fn content_focus(state: &mut ViewState, _: &mut Vec<Effect>) {
    content::settle_focus(&mut state.content);
}

/// Where the keys were is remembered as they move; coming into the prompt
/// from elsewhere stops reading the console and asks the session there for
/// what was typed into it.
fn content_spot(state: &mut ViewState, effects: &mut Vec<Effect>) {
    if content::follow_spot(&mut state.content) {
        effects.extend(console::prompt_entered(state));
    }
}

/// The prompt shows what is written for what the focused window shows.
fn prompt_shown(state: &mut ViewState, effects: &mut Vec<Effect>) {
    effects.extend(prompt::follow_shown(state));
}

/// Keys meant for one area do not carry over to another.
fn console_area(state: &mut ViewState, _: &mut Vec<Effect>) {
    console::keep_to_area(state);
}

/// A list of the sessions drops the inputs kept for those no longer listed.
fn console_listed(state: &mut ViewState, _: &mut Vec<Effect>) {
    console::keep_listed(state);
}

/// Another tab shown is shown live.
fn console_live(state: &mut ViewState, _: &mut Vec<Effect>) {
    console::live_when_shown(state);
}

/// What was typed into a session while every key went to it is asked for
/// once the keys stop going there.
fn console_freed(state: &mut ViewState, effects: &mut Vec<Effect>) {
    effects.extend(console::ask_when_freed(state));
}

/// The history the host read for a step through the prompts sent puts the
/// prompt it steps to in the prompt.
fn prompt_recalled(state: &mut ViewState, effects: &mut Vec<Effect>) {
    effects.extend(prompt::recalled(state));
}

/// Images attached or taken away leave none selected.
fn prompt_images(state: &mut ViewState, _: &mut Vec<Effect>) {
    prompt::keep_selection(&mut state.prompt);
}

/// The prompt's Neovim is offered after `@` what the window lists now.
fn prompt_offered(state: &mut ViewState, effects: &mut Vec<Effect>) {
    effects.extend(prompt::offer(state));
}

/// The status line says why the sessions cannot be listed, while they
/// cannot.
fn tree_failing(state: &mut ViewState, effects: &mut Vec<Effect>) {
    let work = state.host.work.as_ref();
    effects.extend(tree::tell_failing(
        &mut state.tree,
        &mut state.chrome.status,
        work,
    ));
}

/// The tree's rows are built from what the host listed.
fn tree_rows(state: &mut ViewState, _: &mut Vec<Effect>) {
    tree::build(&mut state.tree, state.host.work.as_ref());
}

/// A selected row that leaves the rows gives way to another.
fn tree_kept(state: &mut ViewState, _: &mut Vec<Effect>) {
    let showing = state.content.showing();
    tree::keep_selection(&mut state.tree, &showing, state.host.work.as_ref());
}

/// The selection follows the focused window to what it now shows.
fn tree_switched(state: &mut ViewState, _: &mut Vec<Effect>) {
    let showing = state.content.showing();
    tree::follow_switch(&mut state.tree, &showing, state.host.work.as_ref());
}

/// A finished session the user sees is no longer marked finished.
fn tree_completions(state: &mut ViewState, _: &mut Vec<Effect>) {
    let shown = state.content.active().cloned();
    let work = state.host.work.as_ref();
    tree::see_completions(&mut state.tree, shown.as_ref(), state.chrome.focused, work);
}

/// Leaving the sidebar, or opening a menu, stops typing into the filter.
fn tree_filter_left(state: &mut ViewState, effects: &mut Vec<Effect>) {
    let left =
        state.content.focus != crate::content::Area::Sidebar || state.overlays.menu.is_some();
    effects.extend(tree::leave_filter(&mut state.tree, left));
}

/// A name being typed goes when its row or its tab is no longer shown.
fn overlay_rename(state: &mut ViewState, _: &mut Vec<Effect>) {
    overlay::keep_rename(state);
}

/// The details panel reads by the session and the worktree the focused
/// window shows.
fn details_shown(state: &mut ViewState, _: &mut Vec<Effect>) {
    details::follow(&mut state.details, &state.content.showing());
}

/// The details panel is open with its side panel and reads by the settings.
fn details_told(state: &mut ViewState, _: &mut Vec<Effect>) {
    let open = state.chrome.panels.layout.show_details;
    details::follow_settings(&mut state.details, open, &state.config.settings.details);
}

/// The details panel shows the account usage of the agent the focused
/// window's prompt goes to, while that agent has usage limits.
fn details_usage(state: &mut ViewState, _: &mut Vec<Effect>) {
    let tab = state.content.active().cloned();
    let agent = content::agent_of(state, tab.as_ref());
    let facts = state.host.work.as_ref().and_then(|w| w.agents.of(agent));
    details::follow_usage(&mut state.details, tab.as_ref(), agent, facts);
}

/// What the host read again for the spot the details panel shows moves its
/// selection along, or steps back from a row opened that is gone.
fn details_refreshed(state: &mut ViewState, _: &mut Vec<Effect>) {
    details::refresh(&mut state.details, state.host.work.as_ref());
}

/// A subagent or process opened that the overview no longer lists is gone
/// back from.
fn details_gone(state: &mut ViewState, _: &mut Vec<Effect>) {
    let work = state.host.work.as_ref().map(|w| &w.details);
    details::forget_gone(&mut state.details, work);
}

/// What the host offered the picker is taken into its list and its tree.
fn picker_offered(state: &mut ViewState, _: &mut Vec<Effect>) {
    picker::take_offers(&mut state.picker, state.host.work.as_ref());
}

/// A directory opened in the picker's tree before it was listed is moved
/// into once it is.
fn picker_listed(state: &mut ViewState, _: &mut Vec<Effect>) {
    picker::take_listed(&mut state.picker, state.host.work.as_ref());
}

/// The host keeps read what the tree, the details panel and the picker
/// show, and the worktree in the middle, asked once each time it changes.
fn watched(state: &mut ViewState, effects: &mut Vec<Effect>) {
    let sight = sight(state);
    if state.host.watched == sight {
        return;
    }
    state.host.watched = sight.clone();
    effects.push(Effect::Host(Request::Watch(Box::new(sight))));
}

/// What the window shows that the host keeps read.
pub(crate) fn sight(state: &ViewState) -> Sight {
    Sight {
        tree: tree::sight(&state.tree),
        past: state.picker.past_sight(),
        pick: state.picker.sight(),
        place: state.content.place.as_ref().map(|p| p.worktree.clone()),
        ..details::sight(&state.details)
    }
}

/// `state` with every rule applied once, and what the rules ask done.
pub fn settle(state: &mut ViewState) -> Vec<Effect> {
    let mut effects = Vec::new();
    for rule in RULES {
        rule(state, &mut effects);
    }
    effects
}
