//! A window of the content: the strip on top that names what it shows, the
//! body under it, and the borders between windows that a drag moves.

use std::path::PathBuf;

use adroit_core::completion::Completions;
use adroit_core::tree as tree_view;
use adroit_model::{State, TerminalProgram, TerminalSession, Tree};
use dioxus::prelude::*;

use super::short;
use crate::bridge::AppCtx;
use crate::components::{AgentBadge, FileGlyph, Spacer, StateGlyph, TerminalMark, glyph, runs_of};
use crate::content::{Axis, Border, Rect, Tab, WindowId, thing_name};
use crate::file_icons;
use crate::icons::{Icon, IconView};
use crate::nav;
use adroit_core::actions::Action;
use adroit_core::msg::Msg;

/// What a window's strip says of what it shows.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Strip {
    mark: StripMark,
    name: String,
    agent: Option<String>,
    worktree: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
enum StripMark {
    State(State),
    Terminal(TerminalProgram),
    Draft,
    /// A file's glyph, and the style that colours it.
    File(&'static str, String),
}

pub(super) fn window_strip(
    tree: &Tree,
    terminals: &[TerminalSession],
    completions: &Completions,
    settings: &adroit_core::settings::Settings,
    shows: &Tab,
) -> Strip {
    let session = match shows {
        Tab::Session(id) => nav::find(tree, id).map(|(_, s)| s),
        _ => None,
    };
    let mark = match shows {
        Tab::Session(_) => StripMark::State(
            session
                .map(|s| completions.state(s).clone())
                .unwrap_or_default(),
        ),
        Tab::Terminal(id) => StripMark::Terminal(runs_of(terminals, *id)),
        Tab::Draft { .. } => StripMark::Draft,
        Tab::Viewer(path) => {
            let icon = file_icons::find(path, file_icons::Kind::File);
            let colour = icon
                .colour
                .map(|c| format!("color:{}", c.css()))
                .unwrap_or_default();
            StripMark::File(icon.glyph, colour)
        }
    };
    Strip {
        mark,
        name: thing_name(tree, terminals, shows),
        agent: session.map(|s| settings.started_as(s.agent, &s.profile)),
        worktree: tree_view::place_of_tab(tree, terminals, shows).map(|p| p.worktree),
    }
}

/// The strip on top of a window: what it shows, by its state, name and
/// agent, the worktree it is in, and a button that closes the window `id`;
/// in the `preview`, a badge saying it is only looked at.
#[component]
fn WindowStrip(id: WindowId, strip: Strip, preview: bool) -> Element {
    let app = use_context::<AppCtx>();
    let (g, icon, tip) = match &strip.mark {
        StripMark::State(state) => glyph(state),
        StripMark::Terminal(_) => ("", Icon::Terminal2, ""),
        StripMark::Draft => ("place-mark", Icon::MessagePlus, "new session"),
        StripMark::File(..) => ("", Icon::File, ""),
    };
    let shell = matches!(strip.mark, StripMark::Terminal(_));
    rsx! {
        div { class: if shell { "win-bar shell" } else { "win-bar" },
            match strip.mark {
                StripMark::Terminal(runs) => rsx! { TerminalMark { runs } },
                StripMark::File(glyph, colour) => rsx! { FileGlyph { glyph, style: colour } },
                _ => rsx! { StateGlyph { class: g, icon, size: 13, title: tip } },
            }
            span { class: "win-name", "{strip.name}" }
            if let Some(label) = strip.agent {
                AgentBadge { label }
            }
            Spacer {}
            if preview {
                span {
                    class: "win-preview",
                    title: "Only looked at: what is looked at next takes its place; open it to keep it",
                    IconView { icon: Icon::Eye, size: 12 }
                    "Preview"
                }
            }
            if let Some(w) = &strip.worktree {
                span { class: "win-where", title: "{short(w)}",
                    "{w.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| short(w))}"
                }
            }
            span {
                class: "win-close",
                title: "Close the window; what it shows runs on",
                onclick: move |e: Event<MouseData>| {
                    e.stop_propagation();
                    app.dispatch(Msg::WindowFocus(id));
                    app.dispatch(Msg::Window(Action::WindowQuit));
                },
                IconView { icon: Icon::X, size: 12 }
            }
        }
    }
}

/// The window `id`, where its share `r` of the content puts it, with its
/// strip and `body`, drawn apart in the `preview` and `marked` while it has
/// the keys; pressing it gives it the focus.
#[component]
pub(super) fn ContentWindow(
    id: WindowId,
    r: Rect,
    marked: bool,
    preview: bool,
    strip: Strip,
    body: Element,
) -> Element {
    let app = use_context::<AppCtx>();
    let style = format!(
        "left:calc({}% + 2px);top:calc({}% + 2px);width:calc({}% - 4px);height:calc({}% - 4px)",
        r.left * 100.0,
        r.top * 100.0,
        r.width * 100.0,
        r.height * 100.0
    );
    rsx! {
        div {
            class: match (marked, preview) {
                (true, true) => "win focused preview",
                (true, false) => "win focused",
                (false, true) => "win preview",
                (false, false) => "win",
            },
            style: "{style}",
            onmousedown: move |_| app.dispatch(Msg::WindowFocus(id)),
            WindowStrip { id, strip, preview }
            {body}
        }
    }
}

/// A border between windows, which a drag moves; `drawn` is how large the
/// content's windows are drawn, all of them.
#[component]
pub(super) fn WindowBorder(border: Border, drawn: ReadSignal<(f64, f64)>) -> Element {
    let app = use_context::<AppCtx>();
    let b = border;
    let (x, y) = match b.axis {
        Axis::Row => (b.within.left + b.at * b.within.width, b.within.top),
        Axis::Column => (b.within.left, b.within.top + b.at * b.within.height),
    };
    let (class, style) = match b.axis {
        Axis::Row => (
            "win-border row",
            format!(
                "left:{}%;top:{}%;height:{}%",
                x * 100.0,
                y * 100.0,
                b.within.height * 100.0
            ),
        ),
        Axis::Column => (
            "win-border column",
            format!(
                "left:{}%;top:{}%;width:{}%",
                x * 100.0,
                y * 100.0,
                b.within.width * 100.0
            ),
        ),
    };
    rsx! {
        div {
            class: "{class}",
            style: "{style}",
            title: "Drag to resize the windows beside it",
            onmousedown: move |e: Event<MouseData>| {
                e.stop_propagation();
                e.prevent_default();
                let at = e.client_coordinates();
                let (w, h) = *drawn.peek();
                let (from, px) = match b.axis {
                    Axis::Row => (at.x, b.within.width * w),
                    Axis::Column => (at.y, b.within.height * h),
                };
                app.dispatch(Msg::WindowDragStart { border: b.clone(), from, px });
            },
        }
    }
}
