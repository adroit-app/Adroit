//! One row of the tree: its marks, its name and what it says beside it, the
//! tabs that show it, and what a click or a right click on it does.

use std::collections::HashSet;
use std::path::Path;

use adroit_core::overlay::context_menu::Target;
use adroit_core::settings::Settings;
use adroit_core::tree;
use adroit_model::{EnvironmentId, InboxItem, TerminalSession, Tree};
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::completion::Completions;
use crate::components::{
    AgentBadge, DraftMark, FileLook, FileRow, ListRow, PlaceMark, SessionMark, StateGlyph,
    TerminalMark, glyph, runs_of, session_mark,
};
use crate::connection;
use crate::content::Tab;
use crate::files;
use crate::icons::{Icon, IconView};
use crate::nav::{self, Item};
use crate::rename::{Place as RenamePlace, SessionName};
use crate::shell::overlays::menu_opener;

/// What a sidebar row is, told when the pointer rests on it.
pub(crate) fn row_tip(item: &Item) -> &'static str {
    match item {
        Item::Environment(_) => {
            "An environment, how its connection stands and the work in it. Enter folds it"
        }
        Item::Workspace(_, _) => {
            "A workspace: a git repository or a directory sessions run in. Enter folds it; n starts a session there"
        }
        Item::Worktree(_, _) => {
            "A worktree: a checkout of the repository, on a branch of its own. Enter or l unfolds it while folded, else opens its session; n starts a session in it, s a shell"
        }
        Item::Session(_, _) | Item::Waiting(_, _) => "A session. Enter opens it",
        Item::Terminal(_, _) => {
            "A terminal adroit runs. Enter shows it; closing its tab leaves it running"
        }
        Item::Heading(..) => "What runs in the worktree, of one kind. Enter folds it",
        Item::File { dir: true, .. } => {
            "A directory of the worktree. l goes into it, h folds it or the one it is in, Enter folds or unfolds it"
        }
        Item::File { .. } => "A file of the worktree",
    }
}

/// The label of the agent a session's row runs, and the profile it started
/// from as `settings` name it.
pub(crate) fn row_badge(tree: &Tree, settings: &Settings, item: &Item) -> Option<String> {
    match item {
        Item::Session(_, id) | Item::Waiting(_, id) => nav::find(tree, id).map(|(_, s)| {
            settings.started_as(s.agent, &s.profile, s.home.as_deref(), &s.profile_name)
        }),
        _ => None,
    }
}

/// Whether `shown` shows the file at `path`: in its viewer, or in the Neovim
/// started on it.
pub(crate) fn shows_file(shown: Option<&Tab>, terminals: &[TerminalSession], path: &Path) -> bool {
    match shown {
        Some(Tab::Viewer(_, p)) => p == path,
        Some(Tab::Terminal(_, id)) => terminals
            .iter()
            .any(|t| t.id == *id && t.file.as_deref() == Some(path)),
        _ => false,
    }
}

fn short(path: &Path) -> String {
    adroit_core::picker::short(path, &crate::paths::home())
}

/// What a row reaches outside the tree and the content: the sessions'
/// states, and the drafts written for them.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct RowContext {
    pub(crate) completions: ReadSignal<Completions>,
    pub(crate) drafted: ReadSignal<HashSet<Tab>>,
}

/// What a row asks the window to do.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct RowEvents {
    /// Open the row as a click does.
    pub(crate) onopen: EventHandler<Item>,
    /// Start a session where the row works.
    pub(crate) onstart: EventHandler<Item>,
}

#[component]
pub(crate) fn TreeRow(item: Item, context: RowContext, on: RowEvents) -> Element {
    let app = use_context::<AppCtx>();
    let view = app.tree.read();
    let hosts = app.hosts.read();
    let work = hosts.work(item.environment());
    let t = tree::listed(work);
    let terminals = tree::terminals(work);
    let marks = work.map(|w| &w.tree.marks);
    let content = app.content.read();
    let content_now = &content.windows;
    let shows = content_now.focused().cloned();
    let it = item;
    let is_sel = view.selected.as_ref() == Some(&it);
    let open = view.folds.is_open(&it);
    // The rows of the environments the tree is not at stand back; those
    // waiting on the user, gathered from all of them, do not.
    let elsewhere = !matches!(it, Item::Waiting(..))
        && hosts.each().nth(1).is_some()
        && *it.environment() != view.environment_at(&app.shown_environment());
    if let Item::File {
        environment,
        worktree,
        path,
        dir,
    } = &it
    {
        // Hidden or ignored files stand back, as fern draws them.
        let back = files::is_hidden(path)
            || marks
                .and_then(|m| m.get(worktree))
                .is_some_and(|m| m.ignores(path));
        let look = FileLook {
            dir: *dir,
            back,
            shown: !*dir && shows_file(shows.as_ref(), terminals, path),
            editing: nav::edited(&it, terminals),
            elsewhere,
        };
        let status = marks
            .and_then(|m| m.get(worktree))
            .and_then(|m| m.of(path, *dir));
        // A file stands in its viewer's tabs and its Neovim's.
        let chips = if *dir {
            String::new()
        } else {
            let mut at = content_now.tabs_showing(&Tab::Viewer(environment.clone(), path.clone()));
            for t in terminals.iter().filter(|t| t.file.as_ref() == Some(path)) {
                at.extend(content_now.tabs_showing(&Tab::Terminal(environment.clone(), t.id)));
            }
            at.sort_unstable();
            at.dedup();
            at.iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        };
        let here = it.clone();
        return rsx! {
            FileRow {
                path: path.clone(),
                depth: view.depth(&it),
                look,
                open,
                selected: is_sel,
                title: row_tip(&it).to_owned(),
                onclick: move |_| on.onopen.call(here.clone()),
                oncontextmenu: menu_opener(app, Target::Row(it.clone())),
                if !chips.is_empty() {
                    span { class: "tab-where", title: "The tabs showing it", "{chips}" }
                }
                if let Some(mark) = status {
                    span { class: if mark == 'A' { "file-status added" } else { "file-status" }, "{mark}" }
                }
            }
        };
    }
    if let Item::Environment(environment) = &it {
        let standing = connection::standing_of(&app, environment);
        let waiting = if open {
            0
        } else {
            view.waiting_in(environment)
        };
        let kind = match (standing.mark == connection::Mark::Stopped, elsewhere) {
            (true, true) => "head environment dim elsewhere",
            (true, false) => "head environment dim",
            (false, true) => "head environment elsewhere",
            (false, false) => "head environment",
        };
        let here = it.clone();
        return rsx! {
            ListRow {
                kind,
                selected: is_sel,
                style: "padding-left:8px",
                title: row_tip(&it).to_owned(),
                onclick: move |_| on.onopen.call(here.clone()),
                oncontextmenu: menu_opener(app, Target::Row(it.clone())),
                PlaceMark { icon: if open { Icon::ChevronDown } else { Icon::ChevronRight } }
                connection::StandingDot { mark: standing.mark }
                span { class: "kind-mark", IconView { icon: standing.kind.icon(), size: 14 } }
                span { class: "list-row-name",
                    SessionName {
                        environment: environment.clone(), id: String::new(), name: app.environment_name(environment), place: RenamePlace::Heading,
                    }
                }
                if let Some(note) = standing.note {
                    connection::StandingPill { note }
                }
                if waiting > 0 {
                    span { class: "waiting-count", title: "Sessions waiting on you", "{waiting}" }
                }
            }
        };
    }
    let mark = match &it {
        Item::Session(_, id) => nav::find(t, id).and_then(|(_, s)| session_mark(s.reads_only())),
        _ => None,
    };
    let (kind, text, meta, glyph_class) = match &it {
        Item::Workspace(_, root) => {
            let name = t
                .workspace_of(root)
                .map(|w| w.name.clone())
                .unwrap_or_else(|| short(root));
            ("head", name, String::new(), None)
        }
        Item::Worktree(_, path) => {
            let node = t.worktrees().find(|w| &w.worktree.path == path);
            let label = node
                .and_then(|w| w.worktree.branch.clone())
                .or_else(|| path.file_name().map(|f| f.to_string_lossy().into_owned()))
                .unwrap_or_else(|| short(path));
            let empty = node.is_none_or(|w| w.sessions.is_empty() && w.terminals.is_empty());
            (
                if empty { "dim mono vacant" } else { "dim mono" },
                label,
                String::new(),
                None,
            )
        }
        Item::Heading(_, path, heading) => {
            let shown = view.narrowed(it.environment());
            let count = shown
                .worktrees()
                .find(|w| &w.worktree.path == path)
                .map_or(0, |w| nav::under(it.environment(), w, *heading).len());
            let count = if count > 0 {
                count.to_string()
            } else {
                String::new()
            };
            ("group", heading.name().to_string(), count, None)
        }
        Item::Session(_, id) => {
            let s = nav::find(t, id).map(|(_, s)| s.clone());
            (
                "",
                s.as_ref()
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| id.clone()),
                String::new(),
                s.map(|s| glyph(context.completions.read().state(&s))),
            )
        }
        Item::Terminal(_, id) => {
            let name = t
                .terminal(*id)
                .map(|(_, _, s)| s.name.clone())
                .unwrap_or_default();
            ("", name, String::new(), None)
        }
        Item::Environment(_) | Item::File { .. } | Item::Waiting(_, _) => {
            ("", String::new(), String::new(), None)
        }
    };
    let indent = 14 * view.depth(&it);
    // The tabs of the content that show it.
    let chips = match &it {
        Item::Session(environment, id) => {
            content_now.tabs_showing(&Tab::Session(environment.clone(), id.clone()))
        }
        Item::Terminal(environment, id) => {
            content_now.tabs_showing(&Tab::Terminal(environment.clone(), *id))
        }
        _ => Vec::new(),
    }
    .iter()
    .map(usize::to_string)
    .collect::<Vec<_>>()
    .join(" ");
    let row_title = row_tip(&it).to_owned();
    let terminal = match &it {
        Item::Terminal(_, id) => Some(runs_of(terminals, *id)),
        _ => None,
    };
    let place_icon = match &it {
        Item::Workspace(_, root) if t.workspace_of(root).is_some_and(|w| w.scratch) => {
            Some(Icon::Notes)
        }
        Item::Workspace(_, _) if !open => Some(Icon::Folder),
        Item::Workspace(_, _) => Some(Icon::FolderOpen),
        Item::Worktree(_, _) => Some(Icon::GitBranch),
        Item::Heading(..) if open => Some(Icon::ChevronDown),
        Item::Heading(..) => Some(Icon::ChevronRight),
        _ => None,
    };
    // What the console shows stands out wherever the selection is.
    let kind = match &it {
        Item::Session(environment, id)
            if shows == Some(Tab::Session(environment.clone(), id.clone())) =>
        {
            format!("{kind} shown")
        }
        Item::Terminal(environment, id)
            if shows == Some(Tab::Terminal(environment.clone(), *id)) =>
        {
            format!("{kind} shown")
        }
        _ => kind.to_string(),
    };
    let kind = if elsewhere {
        format!("{kind} elsewhere")
    } else {
        kind
    };
    let target = Target::Row(it.clone());
    // Where a session can start, a button starts one there.
    let starts = matches!(it, Item::Workspace(_, _) | Item::Worktree(_, _));
    let has_draft = match &it {
        Item::Session(environment, id) | Item::Waiting(environment, id) => context
            .drafted
            .read()
            .contains(&Tab::Session(environment.clone(), id.clone())),
        _ => false,
    };
    let badge = row_badge(t, &app.config.read().settings, &it);
    let here = it.clone();
    let start = it.clone();
    rsx! {
        ListRow {
            kind,
            selected: is_sel,
            style: "padding-left:{8 + indent}px",
            title: row_title,
            onclick: move |_| on.onopen.call(here.clone()),
            oncontextmenu: menu_opener(app, target),
            if let Some((g, icon, tip)) = glyph_class {
                StateGlyph { class: g, icon, size: 13, title: tip }
            }
            if let Some(icon) = place_icon {
                PlaceMark { icon }
            }
            if let Some(runs) = terminal { TerminalMark { runs } }
            span { class: "list-row-name",
                if let Item::Session(environment, id) = &it {
                    SessionName {
                        environment: environment.clone(), id: id.clone(), name: text.clone(), place: RenamePlace::Sidebar,
                    }
                } else { "{text}" }
            }
            SessionMark { mark }
            if has_draft {
                DraftMark {}
            }
            span { class: "list-row-meta", "{meta}" }
            if !chips.is_empty() {
                span { class: "tab-where", title: "The tabs showing it", "{chips}" }
            }
            if let Some(label) = badge {
                AgentBadge { label }
            }
            if starts {
                span {
                    class: "list-row-start",
                    title: "Start a session here",
                    onclick: move |e: Event<MouseData>| {
                        e.stop_propagation();
                        on.onstart.call(start.clone());
                    },
                    IconView { icon: Icon::Plus, size: 13 }
                }
            }
        }
    }
}

/// A session waiting on the user, listed again below the tree.
#[component]
pub(crate) fn WaitingRow(environment: EnvironmentId, waiting: InboxItem, on: RowEvents) -> Element {
    let app = use_context::<AppCtx>();
    let item = Item::Waiting(environment.clone(), waiting.session.id.clone());
    let is_sel = app.tree.read().selected.as_ref() == Some(&item);
    let shown = app.content.read().active()
        == Some(&Tab::Session(
            environment.clone(),
            waiting.session.id.clone(),
        ));
    let badge = {
        let hosts = app.hosts.read();
        row_badge(
            tree::listed(hosts.work(&environment)),
            &app.config.read().settings,
            &item,
        )
    };
    let sub = format!("{} · {}", waiting.workspace, waiting.wait);
    let target = Target::Row(item.clone());
    rsx! {
        ListRow {
            kind: if shown { "shown" } else { "" },
            selected: is_sel,
            onclick: move |_| on.onopen.call(item.clone()),
            oncontextmenu: menu_opener(app, target),
            span { class: "state-glyph wait", title: "waiting", style: "margin-top:2px",
                IconView { icon: Icon::Bell, size: 13 }
            }
            div { class: "sidebar-inbox-text",
                span { class: "list-row-name",
                    "{waiting.session.name}"
                    SessionMark { mark: session_mark(waiting.session.reads_only()) }
                }
                span { class: "sidebar-inbox-sub", "{sub}" }
            }
            if let Some(label) = badge {
                AgentBadge { label }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use adroit_model::EnvironmentId;
    use adroit_model::TerminalProgram;
    use adroit_model::testing::{session, tree};

    use super::*;
    use crate::shell::status_bar::mode_tip;

    #[test]
    fn a_file_is_shown_by_its_viewer_or_by_the_neovim_started_on_it() {
        let nvim = TerminalSession {
            id: 7,
            program: TerminalProgram::Neovim,
            file: Some("/w/a.rs".into()),
            ..Default::default()
        };
        let terminals = [nvim];
        let path = Path::new("/w/a.rs");
        assert!(shows_file(
            Some(&Tab::Terminal(EnvironmentId::unbound(), 7)),
            &terminals,
            path
        ));
        assert!(!shows_file(
            Some(&Tab::Terminal(EnvironmentId::unbound(), 8)),
            &terminals,
            path
        ));
        assert!(shows_file(
            Some(&Tab::Viewer(EnvironmentId::unbound(), "/w/b.png".into())),
            &terminals,
            Path::new("/w/b.png")
        ));
        assert!(!shows_file(None, &terminals, path));
    }

    #[test]
    fn sidebar_session_badges_identify_the_agent_even_when_names_match() {
        use adroit_model::Agent;

        let sessions = [Agent::Claude, Agent::Codex].map(|agent| {
            session(agent.key())
                .agent(agent)
                .name("Review")
                .cwd("/project")
        });
        let tree = tree().sessions(sessions).build();
        let settings = Settings::default();
        for (id, label) in [("claude", "Claude Code"), ("codex", "Codex")] {
            let badge = |item| row_badge(&tree, &settings, &item);
            assert_eq!(
                badge(Item::Session(EnvironmentId::unbound(), id.into())).as_deref(),
                Some(label)
            );
            assert_eq!(
                badge(Item::Waiting(EnvironmentId::unbound(), id.into())).as_deref(),
                Some(label)
            );
        }
        for item in [
            Item::Workspace(EnvironmentId::unbound(), "/project".into()),
            Item::Worktree(EnvironmentId::unbound(), "/project".into()),
            Item::Terminal(EnvironmentId::unbound(), 1),
            Item::Session(EnvironmentId::unbound(), "missing".into()),
        ] {
            assert_eq!(row_badge(&tree, &settings, &item), None);
        }
    }

    #[test]
    fn tips_that_hold_for_any_agent_name_the_session_rather_than_claude_code() {
        for tip in [
            mode_tip("DIRECT", false),
            row_tip(&Item::Session(EnvironmentId::unbound(), "s1".into())),
            row_tip(&Item::Waiting(EnvironmentId::unbound(), "s1".into())),
        ] {
            assert!(!tip.contains("Claude"), "{tip}");
        }
    }
}
