//! One row of the tree: its marks, its name and what it says beside it, the
//! tabs that show it, and what a click or a right click on it does.

use std::collections::HashSet;
use std::path::Path;

use adroit_core::msg::Msg;
use adroit_core::overlay::context_menu::Target;
use adroit_core::tree;
use adroit_model::{InboxItem, TerminalSession, Tree};
use dioxus::prelude::*;

use crate::bridge::AppCtx;
use crate::completion::Completions;
use crate::components::{SessionMark, TerminalMark, glyph, runs_of, session_mark};
use crate::content::Tab;
use crate::file_icons;
use crate::files;
use crate::icons::{Icon, IconView};
use crate::nav::{self, Item};
use crate::rename::{Place as RenamePlace, SessionName};

/// What a sidebar row is, told when the pointer rests on it.
pub(crate) fn row_tip(item: &Item) -> &'static str {
    match item {
        Item::Workspace(_) => {
            "A workspace: a git repository or a directory sessions run in. Enter folds it; n starts a session there"
        }
        Item::Worktree(_) => {
            "A worktree: a checkout of the repository, on a branch of its own. Enter or l unfolds it while folded, else opens its session; n starts a session in it, s a shell"
        }
        Item::Session(_) | Item::Waiting(_) => {
            "A session. Enter opens it, or unfolds the sessions it started while they are folded"
        }
        Item::Terminal(_) => {
            "A terminal adroit runs. Enter shows it; closing its tab leaves it running"
        }
        Item::Heading(..) => "What runs in the worktree, of one kind. Enter folds it",
        Item::File { dir: true, .. } => {
            "A directory of the worktree. Enter or l unfolds it, h folds it"
        }
        Item::File { .. } => "A file of the worktree",
    }
}

/// The label of the agent a session's row runs.
pub(crate) fn row_badge(tree: &Tree, item: &Item) -> Option<&'static str> {
    match item {
        Item::Session(id) | Item::Waiting(id) => nav::find(tree, id).map(|(_, s)| s.agent.label()),
        _ => None,
    }
}

/// Whether `shown` shows the file at `path`: in its viewer, or in the Neovim
/// started on it.
pub(crate) fn shows_file(shown: Option<&Tab>, terminals: &[TerminalSession], path: &Path) -> bool {
    match shown {
        Some(Tab::Viewer(p)) => p == path,
        Some(Tab::Terminal(id)) => terminals
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
    let host = app.host.read();
    let work = host.work.as_ref();
    let t = tree::listed(work);
    let terminals = tree::terminals(work);
    let marks = work.map(|w| &w.tree.marks);
    let content = app.content.read();
    let content_now = &content.windows;
    let shows = content_now.focused().cloned();
    let it = item;
    let is_sel = view.selected.as_ref() == Some(&it);
    let open = view.folds.is_open(&it);
    let mark = match &it {
        Item::Session(id) => nav::find(t, id).and_then(|(_, s)| session_mark(s.reads_only())),
        _ => None,
    };
    let (class, text, meta, glyph_class) = match &it {
        Item::Workspace(root) => {
            let name = t
                .workspace_of(root)
                .map(|w| w.name.clone())
                .unwrap_or_else(|| short(root));
            ("list-row head", name, String::new(), None)
        }
        Item::Worktree(path) => {
            let node = t.worktrees().find(|w| &w.worktree.path == path);
            let label = node
                .and_then(|w| w.worktree.branch.clone())
                .or_else(|| path.file_name().map(|f| f.to_string_lossy().into_owned()))
                .unwrap_or_else(|| short(path));
            let empty = node.is_none_or(|w| w.sessions.is_empty() && w.terminals.is_empty());
            (
                if empty {
                    "list-row dim mono vacant"
                } else {
                    "list-row dim mono"
                },
                label,
                String::new(),
                None,
            )
        }
        Item::Heading(path, heading) => {
            let shown = &view.narrowed;
            let count = shown
                .worktrees()
                .find(|w| &w.worktree.path == path)
                .map_or(0, |w| nav::under(shown, w, *heading).len());
            let count = if count > 0 {
                count.to_string()
            } else {
                String::new()
            };
            ("list-row group", heading.name().to_string(), count, None)
        }
        Item::Session(id) => {
            let s = nav::find(t, id).map(|(_, s)| s.clone());
            let elsewhere = s
                .as_ref()
                .and_then(|s| nav::runs_at(t, s))
                .unwrap_or_default();
            (
                "list-row",
                s.as_ref()
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| id.clone()),
                elsewhere,
                s.map(|s| glyph(context.completions.read().state(&s))),
            )
        }
        Item::Terminal(id) => {
            let name = t
                .terminal(*id)
                .map(|(_, _, s)| s.name.clone())
                .unwrap_or_default();
            ("list-row", name, String::new(), None)
        }
        Item::File {
            worktree,
            path,
            dir,
        } => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            // Hidden or ignored files stand back, as fern draws them.
            let back = files::is_hidden(path)
                || marks
                    .and_then(|m| m.get(worktree))
                    .is_some_and(|m| m.ignores(path));
            let class = match (*dir, back) {
                (true, true) => "list-row file dir back",
                (true, false) => "list-row file dir",
                (false, true) => "list-row file back",
                (false, false) => "list-row file",
            };
            (
                class,
                if *dir { format!("{name}/") } else { name },
                String::new(),
                None,
            )
        }
        Item::Waiting(_) => ("list-row", String::new(), String::new(), None),
    };
    // The rows are of the narrowed tree, where a workspace can be flat that
    // is not in full.
    let indent = 14 * nav::depth(&view.narrowed, &it);
    // The tabs of the content that show it.
    let chips = match &it {
        Item::Session(id) => content_now.tabs_showing(&Tab::Session(id.clone())),
        Item::Terminal(id) => content_now.tabs_showing(&Tab::Terminal(*id)),
        // A file stands in its viewer's tabs and its Neovim's.
        Item::File {
            path, dir: false, ..
        } => {
            let mut at = content_now.tabs_showing(&Tab::Viewer(path.clone()));
            for t in terminals.iter().filter(|t| t.file.as_ref() == Some(path)) {
                at.extend(content_now.tabs_showing(&Tab::Terminal(t.id)));
            }
            at.sort_unstable();
            at.dedup();
            at
        }
        _ => Vec::new(),
    }
    .iter()
    .map(usize::to_string)
    .collect::<Vec<_>>()
    .join(" ");
    let row_title = row_tip(&it).to_owned();
    let (file_glyph, file_mark) = match &it {
        Item::File {
            worktree,
            path,
            dir,
        } => {
            let kind = if *dir {
                file_icons::Kind::Directory { open }
            } else {
                file_icons::Kind::File
            };
            let icon = file_icons::find(path, kind);
            let colour = icon
                .colour
                .map(|c| format!("color:{}", c.css()))
                .unwrap_or_default();
            let mark = marks
                .and_then(|m| m.get(worktree))
                .and_then(|m| m.of(path, *dir));
            (Some((icon.glyph, colour)), mark)
        }
        _ => (None, None),
    };
    let terminal = match &it {
        Item::Terminal(id) => Some(runs_of(terminals, *id)),
        _ => None,
    };
    let place_icon = match &it {
        Item::Workspace(root) if t.workspace_of(root).is_some_and(|w| w.scratch) => {
            Some(Icon::Notes)
        }
        Item::Workspace(_) if !open => Some(Icon::Folder),
        Item::Workspace(_) => Some(Icon::FolderOpen),
        Item::Worktree(_) => Some(Icon::GitBranch),
        Item::Heading(..) if open => Some(Icon::ChevronDown),
        Item::Heading(..) => Some(Icon::ChevronRight),
        _ => None,
    };
    let class = if is_sel {
        format!("{class} selected")
    } else {
        class.to_string()
    };
    // What the console shows stands out wherever the selection is.
    let class = match &it {
        Item::Session(id) if shows == Some(Tab::Session(id.clone())) => format!("{class} shown"),
        Item::Terminal(id) if shows == Some(Tab::Terminal(*id)) => format!("{class} shown"),
        Item::File {
            path, dir: false, ..
        } if shows_file(shows.as_ref(), terminals, path) => format!("{class} shown"),
        _ => class,
    };
    let target = Target::Row(it.clone());
    // Where a session can start, a button starts one there.
    let starts = matches!(it, Item::Workspace(_) | Item::Worktree(_));
    let has_draft = match &it {
        Item::Session(id) | Item::Waiting(id) => {
            context.drafted.read().contains(&Tab::Session(id.clone()))
        }
        _ => false,
    };
    let badge = row_badge(t, &it);
    let here = it.clone();
    let start = it.clone();
    rsx! {
        div {
            class: "{class}",
            style: "padding-left:{8 + indent}px",
            title: "{row_title}",
            onclick: move |_| on.onopen.call(here.clone()),
            oncontextmenu: move |e: Event<MouseData>| {
                e.prevent_default();
                let at = e.client_coordinates();
                app.dispatch(Msg::MenuOpen { x: at.x, y: at.y, target: target.clone() });
            },
            if let Some((g, icon, tip)) = glyph_class {
                span { class: "{g}", title: "{tip}", IconView { icon, size: 13 } }
            }
            if let Some(icon) = place_icon {
                span { class: "place-mark", IconView { icon } }
            }
            if let Some((glyph, colour)) = &file_glyph {
                span { class: "nf", style: "{colour}", "aria-hidden": "true", "{glyph}" }
            }
            if let Some(runs) = terminal { TerminalMark { runs } }
            span { class: "list-row-name",
                if let Item::Session(id) = &it {
                    SessionName {
                        id: id.clone(), name: text.clone(), place: RenamePlace::Sidebar,
                    }
                } else { "{text}" }
            }
            SessionMark { mark }
            if has_draft {
                span { class: "draft-mark", title: "Something is written for it in the prompt", "✎" }
            }
            span { class: "list-row-meta", "{meta}" }
            if !chips.is_empty() {
                span { class: "tab-where", title: "The tabs showing it", "{chips}" }
            }
            if let Some(mark) = file_mark {
                span { class: if mark == 'A' { "file-status added" } else { "file-status" }, "{mark}" }
            }
            if let Some(label) = badge {
                span { class: "agent-badge", title: "Session agent", "{label}" }
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
pub(crate) fn WaitingRow(waiting: InboxItem, on: RowEvents) -> Element {
    let app = use_context::<AppCtx>();
    let item = Item::Waiting(waiting.session.id.clone());
    let is_sel = app.tree.read().selected.as_ref() == Some(&item);
    let shown = app.content.read().active() == Some(&Tab::Session(waiting.session.id.clone()));
    let badge = {
        let host = app.host.read();
        row_badge(tree::listed(host.work.as_ref()), &item)
    };
    let sub = format!("{} · {}", waiting.workspace, waiting.wait);
    let target = Target::Row(item.clone());
    rsx! {
        div {
            class: match (is_sel, shown) {
                (true, true) => "list-row selected shown",
                (true, false) => "list-row selected",
                (false, true) => "list-row shown",
                (false, false) => "list-row",
            },
            onclick: move |_| on.onopen.call(item.clone()),
            oncontextmenu: move |e: Event<MouseData>| {
                e.prevent_default();
                let at = e.client_coordinates();
                app.dispatch(Msg::MenuOpen { x: at.x, y: at.y, target: target.clone() });
            },
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
                span { class: "agent-badge", title: "Session agent", "{label}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
        assert!(shows_file(Some(&Tab::Terminal(7)), &terminals, path));
        assert!(!shows_file(Some(&Tab::Terminal(8)), &terminals, path));
        assert!(shows_file(
            Some(&Tab::Viewer("/w/b.png".into())),
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
        for (id, label) in [("claude", "Claude Code"), ("codex", "Codex")] {
            assert_eq!(row_badge(&tree, &Item::Session(id.into())), Some(label));
            assert_eq!(row_badge(&tree, &Item::Waiting(id.into())), Some(label));
        }
        for item in [
            Item::Workspace("/project".into()),
            Item::Worktree("/project".into()),
            Item::Terminal(1),
            Item::Session("missing".into()),
        ] {
            assert_eq!(row_badge(&tree, &item), None);
        }
    }

    #[test]
    fn tips_that_hold_for_any_agent_name_the_session_rather_than_claude_code() {
        for tip in [
            mode_tip("DIRECT", false),
            row_tip(&Item::Session("s1".into())),
            row_tip(&Item::Waiting("s1".into())),
        ] {
            assert!(!tip.contains("Claude"), "{tip}");
        }
    }
}
