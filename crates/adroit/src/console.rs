//! The middle of the window: the console's screen and the prompt card below
//! it. Each follows its own frame counter, so output in one redraws neither
//! the other nor the rest of the window.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use adroit_nvim::{Embedded, UiState};
use adroit_term::{Daemon, Palette, Pane};
use dioxus::html::geometry::WheelDelta;
use dioxus::prelude::*;
use tokio::sync::mpsc;

use crate::components::{EmptyState, KeyHint};
use crate::content::Tab;
use crate::icons::{Icon, IconView};
use crate::style::use_cell_height;
use crate::view::ScreenView;

/// How many completions the menu shows at a time.
const MENU_ITEMS: usize = 10;

/// The first of `items` completions the menu shows `shown` at a time, having
/// shown from `top`: it moves only as far as keeps the `chosen` one in view,
/// as Neovim's own menu does.
fn menu_top(top: usize, chosen: Option<usize>, items: usize, shown: usize) -> usize {
    let top = top.min(items.saturating_sub(shown));
    match chosen {
        Some(c) if c < top => c,
        Some(c) if c >= top + shown => c + 1 - shown,
        _ => top,
    }
}

/// The completion menu's least and greatest width.
const MENU_WIDTH: (f64, f64) = (260.0, 520.0);

/// Where a scroll bar's thumb sits and how long it is, as fractions of the
/// bar, for text of `lines` shown `rows` at a time from line `top`; `None` when
/// it all shows.
fn scroll_thumb(top: usize, rows: usize, lines: usize) -> Option<(f64, f64)> {
    if lines <= rows || rows == 0 {
        return None;
    }
    let size = rows as f64 / lines as f64;
    let top = (top as f64 / lines as f64).min(1.0 - size);
    Some((top, size))
}

/// How far above the bottom of a grid of `rows` the completion menu sits, for
/// a menu opened at `row`. A grid that shrank under an open menu leaves the
/// row past its end.
fn menu_bottom(rows: usize, row: usize, cell_height: f64) -> f64 {
    rows.saturating_sub(row) as f64 * cell_height + 8.0
}

/// Where the completion menu starts and how wide it may be, for a cursor at
/// `x` in a grid `width` wide: at the cursor, moved left as far as the
/// menu needs to stay inside the grid.
fn menu_span(x: f64, width: f64) -> (f64, f64, f64) {
    let (least, most) = MENU_WIDTH;
    let left = x.min(width - least).max(0.0);
    let max = (width - left).min(most);
    (left, least.min(max), max)
}

/// The sessions shown in the console, by tab.
pub type Panes = Rc<RefCell<HashMap<Tab, Arc<Pane>>>>;

/// What the window holds of the sessions, kept outside the reactive state
/// and shared with its components through the context.
#[derive(Clone)]
pub struct Hub {
    /// The daemon the sessions run in.
    pub daemon: Daemon,
    pub panes: Panes,
    /// The tabs whose sessions are being attached to.
    pub attaching: Rc<RefCell<std::collections::HashSet<Tab>>>,
    /// Told which pane has new output.
    pub wake: mpsc::UnboundedSender<Tab>,
    /// The theme's palette, which a new theme replaces.
    pub palette: Signal<Palette>,
}

/// The window's own handles on the focused window's console, its reader and
/// the prompt, which stay out of the view state: their Neovims, the frames
/// they are drawn again by and the sizes they are drawn at.
#[derive(Clone, Copy)]
pub(crate) struct Consoles {
    /// Moves when a pane on screen has new output.
    pub frame: Signal<u64>,
    /// Moves when a pane is started, for what the console says around it.
    pub attached: Signal<u64>,
    pub nvim: Signal<Option<Arc<Embedded>>>,
    pub prompt_frame: Signal<u64>,
    /// The line the prompt shows from, and how many it has.
    pub prompt_view: Signal<(usize, usize)>,
    /// The lines the prompt shows.
    pub prompt_rows: Signal<usize>,
    /// The console read in a Neovim of its own, while the console says it
    /// is read.
    pub reader: Signal<Option<Arc<Embedded>>>,
    pub reader_frame: Signal<u64>,
    /// The line and column the reader shows from, which images follow.
    pub reader_view: Signal<(usize, usize)>,
    /// How many lines the reader holds, and how many of them are the screen,
    /// the last.
    pub read_rows: Signal<(usize, usize)>,
    pub cell_width: Signal<f64>,
    /// How large the focused window's console is drawn.
    pub console_box: Signal<(f64, f64)>,
    pub composer_height: Signal<f64>,
    pub prompt_width: Signal<f64>,
    /// The settings the console's font is drawn by.
    pub font: Signal<crate::settings::Settings>,
    pub prompts: crate::prompts::Prompts,
    /// The wheel over the reader, by lines.
    pub scroll_reader: Callback<i32>,
    /// A click on the reader, at a column and a row of it.
    pub click_reader: Callback<(usize, usize)>,
}

impl Consoles {
    /// Whether the prompt's Neovim is inserting.
    pub(crate) fn writing(&self) -> bool {
        self.nvim
            .peek()
            .as_ref()
            .is_some_and(|n| n.state().mode().starts_with("insert"))
    }

    /// Start writing in the prompt, whatever mode it was left in: a draft
    /// asks for its first prompt.
    pub(crate) fn start_writing(&self) {
        if let Some(nv) = self.nvim.peek().clone() {
            spawn(async move {
                let _ = nv.start_writing().await;
            });
        }
    }
}

/// The prompt's mode as the window names it: a style class and a label.
pub fn mode_of(state: &UiState) -> (&'static str, &'static str) {
    let m = state.mode();
    if m.starts_with("cmdline") && state.cmdline().is_some() {
        ("cmd", "COMMAND")
    } else if m.starts_with("insert") {
        ("insert", "INSERT")
    } else if m.starts_with("visual") {
        ("normal", "VISUAL")
    } else {
        ("normal", "NORMAL")
    }
}

/// What the pane of `tab` shows, redrawn whenever `frame` moves.
#[component]
pub fn ConsoleScreen(tab: Tab, frame: Signal<u64>, cell_width: f64, focused: bool) -> Element {
    let ch = use_cell_height();
    let _ = frame();
    let hub = use_context::<Hub>();
    let pane = hub.panes.borrow().get(&tab).cloned();
    let Some(pane) = pane else {
        return rsx! {
            EmptyState { "Attaching…" }
        };
    };
    let screen = pane.screen();
    let pictures = pane.pictures();
    let rows = screen.rows as f64;
    let mut frame = frame;
    let mut rest = use_signal(|| 0.0);
    let onwheel = {
        let pane = pane.clone();
        move |e: WheelEvent| {
            let by = match e.delta() {
                WheelDelta::Pixels(v) => v.y / ch,
                WheelDelta::Lines(v) => v.y,
                WheelDelta::Pages(v) => v.y * rows,
            };
            let (lines, left) = lines_back(*rest.peek(), by);
            rest.set(left);
            if lines != 0 {
                let at = e.element_coordinates();
                let (col, row) = cell_at(at.x, at.y, cell_width, ch);
                pane.wheel(lines, col, row);
                *frame.write() += 1;
            }
        }
    };
    rsx! {
        div { class: "console-screen", onwheel,
            ScreenView { screen, cell_width, show_cursor: focused, pictures }
            Clicks { cell_width, onclick: { let pane = pane.clone(); move |(col, row)| pane.click(col, row) } }
        }
        if pane.exited() {
            EmptyState { kind: "detached", title: "Ended",
                div { "What ran here has exited." }
            }
        }
    }
}

/// The console as its reader shows it: Neovim's grid in the console's
/// place, with its command line or message on the last row. The wheel is
/// told to `onscroll`, in lines back, or forward when negative, and a click
/// to `onclick`, by the reader's column and row.
#[component]
pub fn ReaderScreen(
    reader: Signal<Option<Arc<Embedded>>>,
    reader_frame: Signal<u64>,
    cell_width: f64,
    focused: bool,
    /// The console's images, placed over the reader's rows.
    pictures: Vec<adroit_term::Picture>,
    onscroll: EventHandler<i32>,
    onclick: EventHandler<(usize, usize)>,
) -> Element {
    let ch = use_cell_height();
    let _ = reader_frame();
    let Some(r) = reader() else {
        return rsx! {};
    };
    let (screen, cmdline, message) = {
        let s = r.state();
        (
            s.screen(),
            s.cmdline().cloned(),
            s.message().map(str::to_string),
        )
    };
    // The grid's last row, past the screen's top padding: the command line
    // or a message covers it while it shows, as with Vim's `cmdheight=0`.
    let line_top = 10.0 + screen.rows.saturating_sub(1) as f64 * ch;
    let rows = screen.rows as f64;
    let mut rest = use_signal(|| 0.0);
    let onwheel = move |e: WheelEvent| {
        let by = match e.delta() {
            WheelDelta::Pixels(v) => v.y / ch,
            WheelDelta::Lines(v) => v.y,
            WheelDelta::Pages(v) => v.y * rows,
        };
        let (lines, left) = lines_back(*rest.peek(), by);
        rest.set(left);
        if lines != 0 {
            onscroll.call(lines);
        }
    };
    rsx! {
        div {
            class: "console-screen reading",
            // Images stop at the grid's end.
            style: "bottom:auto;height:{line_top + ch}px",
            onwheel,
            ScreenView { screen, cell_width, show_cursor: focused, pictures }
            Clicks { cell_width, onclick }
            if let Some(cmd) = cmdline {
                div { class: "console-reader-line", style: "top:{line_top}px", "{cmd.firstc}{cmd.text}" }
            } else if let Some(m) = message {
                div { class: "console-reader-line", style: "top:{line_top}px", "{m}" }
            }
        }
    }
}

/// The cell, by column and row, at `x` and `y` pixels from the grid's top left.
fn cell_at(x: f64, y: f64, cell_width: f64, cell_height: f64) -> (usize, usize) {
    let cell = |at: f64, size: f64| (at / size).floor().max(0.0) as usize;
    (cell(x, cell_width), cell(y, cell_height))
}

/// A layer over the grid that tells `onclick` of the cell clicked.
#[component]
fn Clicks(cell_width: f64, onclick: EventHandler<(usize, usize)>) -> Element {
    let ch = use_cell_height();
    rsx! {
        div {
            class: "console-clicks",
            onclick: move |e: Event<MouseData>| {
                let at = e.element_coordinates();
                onclick.call(cell_at(at.x, at.y, cell_width, ch));
            },
        }
    }
}

/// How many lines a wheel turned by `by` lines down the page, after `rest`
/// left from turns too small to move, scrolls back; and what is left now.
fn lines_back(rest: f64, by: f64) -> (i32, f64) {
    let total = rest - by;
    let whole = total.trunc();
    (whole as i32, total - whole)
}

pub fn prompt_rows(text_rows: usize, has_images: bool, min: usize, max: usize) -> usize {
    text_rows.clamp(if has_images { 1 } else { min }, max)
}

/// The prompt card: Neovim's grid, its completion menu, command line and
/// messages, redrawn whenever `prompt_frame` moves.
#[component]
pub fn Composer(
    nvim: Signal<Option<Arc<Embedded>>>,
    prompt_frame: Signal<u64>,
    cell_width: f64,
    focused: bool,
    target: String,
    composer_height: Signal<f64>,
    prompt_width: Signal<f64>,
    /// Told when the card is pressed, to take the keys.
    onpick: EventHandler<()>,
    /// The keys that send what is written.
    send_keys: Option<String>,
    /// The line the prompt shows from, and how many it has.
    view: (usize, usize),
    /// The lines the prompt shows.
    rows: usize,
    /// Whether what is written is a shell line for the session.
    shell: bool,
    /// Whether the keys typed here answer the session waiting above instead.
    answering: bool,
    images: Vec<std::path::PathBuf>,
    selected_image: Option<std::path::PathBuf>,
    image_keys: Option<String>,
    onselectimage: EventHandler<std::path::PathBuf>,
    onremoveimage: EventHandler<std::path::PathBuf>,
) -> Element {
    let previews = use_memo(use_reactive((&images,), |(images,)| {
        images
            .into_iter()
            .map(|path| {
                let thumb = crate::pictures::file_thumb(&path);
                (path, thumb)
            })
            .collect::<Vec<_>>()
    }));
    let ch = use_cell_height();
    let thumb = scroll_thumb(view.0, rows, view.1);
    let _ = prompt_frame();
    let cw = cell_width;
    let prompt = nvim().map(|n| {
        let s = n.state();
        (
            s.screen(),
            s.popup().cloned(),
            s.cmdline().cloned(),
            s.message().map(str::to_string),
        )
    });
    let menu_from = use_hook(|| Rc::new(Cell::new(0usize)));
    let shown_from = match prompt.as_ref().and_then(|(_, popup, ..)| popup.as_ref()) {
        Some(pm) => menu_top(menu_from.get(), pm.selected, pm.items.len(), MENU_ITEMS),
        None => 0,
    };
    menu_from.set(shown_from);
    // Placed in the grid's padding box: its content width and the padding.
    let menu = prompt
        .as_ref()
        .and_then(|(_, popup, ..)| popup.as_ref())
        .map(|pm| menu_span(14.0 + pm.col as f64 * cw, prompt_width() + 28.0));
    rsx! {
        div {
            class: format!(
                "prompt{}{}{}",
                if focused { " focused" } else { "" },
                if shell { " shell" } else { "" },
                if answering { " answering" } else { "" },
            ),
            // Stopped here, or the console around the card takes the keys back.
            onmousedown: move |e: Event<MouseData>| {
                e.stop_propagation();
                onpick.call(());
            },
            // Resize events bubble: each handler stops its own, or the
            // console would take the card's size for its own.
            onresize: move |e: Event<ResizeData>| {
                e.stop_propagation();
                if let Ok(sz) = e.data().get_border_box_size() {
                    composer_height.set(sz.height);
                }
            },
            if let Some((_, _, Some(cmd), _)) = &prompt {
                div { class: "vim-cmdline", "{cmd.firstc}{cmd.text}" }
            }
            div {
                class: "prompt-grid",
                onresize: move |e: Event<ResizeData>| {
                    e.stop_propagation();
                    if let Ok(sz) = e.data().get_content_box_size() {
                        prompt_width.set(sz.width);
                    }
                },
                if let Some((top, size)) = thumb {
                    div { class: "prompt-scroll",
                        div { class: "prompt-scroll-thumb", style: "top:{top * 100.0}%;height:{size * 100.0}%" }
                    }
                }
                if let Some((screen, popup, cmd, _)) = &prompt {
                    ScreenView { screen: screen.clone(), cell_width: cw, show_cursor: focused && cmd.is_none() && !answering && selected_image.is_none() }
                    if let (Some(pm), Some((left, least, most))) = (popup, menu) {
                        div {
                            class: "vim-menu",
                            style: "left:{left}px;min-width:{least}px;max-width:{most}px;bottom:{menu_bottom(screen.rows, pm.row, ch)}px",
                            for (i , item) in pm.items.iter().enumerate().skip(shown_from).take(MENU_ITEMS) {
                                div {
                                    key: "{i}",
                                    class: if pm.selected == Some(i) { "vim-menu-item selected" } else { "vim-menu-item" },
                                    span { class: "vim-menu-word", "{item.word}" }
                                    span { class: "vim-menu-detail", "{item.menu} {item.kind}" }
                                }
                                if pm.selected == Some(i) && !item.info.is_empty() {
                                    div { class: "vim-menu-info", "{item.info}" }
                                }
                            }
                        }
                    }
                }
            }
            if !images.is_empty() {
                div {
                    class: "prompt-attachments",
                    role: "group",
                    aria_label: "Attached images",
                    for (path, preview) in previews().iter() {
                        div {
                            key: "{path.display()}",
                            class: if selected_image.as_ref() == Some(path) { "prompt-attachment selected" } else { "prompt-attachment" },
                            title: "{path.file_name().unwrap_or_default().to_string_lossy()} — arrows move; Delete removes; Esc returns",
                            onmousedown: { let path = path.clone(); move |e: Event<MouseData>| {
                                e.stop_propagation();
                                e.prevent_default();
                                onselectimage.call(path.clone());
                            } },
                            if let Some(preview) = preview {
                                img { src: "{preview}", alt: "Attached image: {path.file_name().unwrap_or_default().to_string_lossy()}" }
                            } else {
                                span { class: "attachment-missing", "Image unavailable" }
                            }
                            button {
                                class: "attachment-remove",
                                r#type: "button",
                                aria_label: "Remove attached image {path.file_name().unwrap_or_default().to_string_lossy()}",
                                title: "Remove image",
                                onmousedown: move |e: Event<MouseData>| { e.stop_propagation(); e.prevent_default(); },
                                onclick: { let path = path.clone(); move |e: Event<MouseData>| { e.stop_propagation(); onremoveimage.call(path.clone()); } },
                                "×"
                            }
                        }
                    }
                }
            }
            div { class: "prompt-foot",
                if shell {
                    span {
                        class: "prompt-shell",
                        title: "Sent, this line runs as a shell command in the session, its output added to the conversation",
                        "! shell"
                    }
                }
                span { class: "prompt-foot-target", IconView { icon: Icon::ArrowRight, size: 12 } "{target}" }
                if let Some((_, _, _, Some(msg))) = &prompt {
                    span { class: "prompt-foot-message", "{msg}" }
                }
                span { class: "flex-spacer" }
                if !images.is_empty() {
                    if selected_image.is_some() {
                        span { class: "kbd-list", "← → select · Delete remove · Esc return" }
                    } else if let Some(k) = image_keys {
                        KeyHint { keys: k, does: "images" }
                    }
                }
                if answering {
                    span { class: "prompt-foot-answering", "keys answer above" }
                } else if let Some(k) = send_keys {
                    KeyHint { keys: k, does: "send" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attached_prompts_use_their_actual_text_rows_instead_of_the_minimum() {
        assert_eq!(prompt_rows(1, true, 3, 12), 1);
        assert_eq!(prompt_rows(2, true, 3, 12), 2);
        assert_eq!(prompt_rows(5, true, 3, 12), 5);
        assert_eq!(prompt_rows(20, true, 3, 12), 12);
        assert_eq!(prompt_rows(0, true, 3, 12), 1);
        assert_eq!(prompt_rows(1, false, 3, 12), 3);
        assert_eq!(prompt_rows(20, false, 3, 12), 12);
    }

    #[test]
    fn a_click_lands_on_the_cell_under_it() {
        assert_eq!(cell_at(0.0, 0.0, 8.0, 17.0), (0, 0));
        assert_eq!(cell_at(23.9, 34.0, 8.0, 17.0), (2, 2));
        // Past the left or the top, as a fractional pixel may be.
        assert_eq!(cell_at(-0.5, -0.5, 8.0, 17.0), (0, 0));
    }

    #[test]
    fn a_prompt_taller_than_it_shows_a_thumb_as_long_as_what_shows() {
        assert_eq!(scroll_thumb(0, 12, 12), None);
        assert_eq!(scroll_thumb(0, 12, 24), Some((0.0, 0.5)));
        assert_eq!(scroll_thumb(12, 12, 24), Some((0.5, 0.5)));
        // Never past the end of the bar.
        assert_eq!(scroll_thumb(20, 12, 24), Some((0.5, 0.5)));
    }

    #[test]
    fn a_wheel_scrolls_whole_lines_and_keeps_the_rest_for_the_next_turn() {
        // Down the page is towards the bottom, so back is up.
        assert_eq!(lines_back(0.0, -2.0), (2, 0.0));
        assert_eq!(lines_back(0.0, 0.6), (0, -0.6));
        let (n, rest) = lines_back(-0.6, 0.6);
        assert_eq!(n, -1);
        assert!((rest + 0.2).abs() < 1e-9, "{rest}");
    }

    #[test]
    fn the_menu_moves_only_as_far_as_the_choice_needs_to_stay_in_view() {
        // (shown from, chosen, items) -> shown from, ten at a time
        let table = [
            (0, Some(9), 20, 0),
            (0, Some(10), 20, 1),
            (1, Some(5), 20, 1),
            (5, Some(4), 20, 4),
            // Nothing chosen keeps the view; wrapping round goes to the end.
            (5, None, 20, 5),
            (0, Some(19), 20, 10),
            // Fewer items than before never leave the view past its end.
            (10, Some(2), 5, 0),
            (10, None, 12, 2),
        ];
        for (top, chosen, items, want) in table {
            assert_eq!(
                menu_top(top, chosen, items, 10),
                want,
                "{top} {chosen:?} {items}"
            );
        }
    }

    #[test]
    fn the_menu_sits_above_the_cursor_row_and_never_below_the_grid() {
        assert_eq!(menu_bottom(3, 1, 17.0), 2.0 * 17.0 + 8.0);
        // A grid that shrank while the menu was open.
        assert_eq!(menu_bottom(1, 2, 17.0), 8.0);
    }

    #[test]
    fn the_menu_opens_at_the_cursor_when_it_fits() {
        assert_eq!(menu_span(40.0, 600.0), (40.0, 260.0, 520.0));
    }

    #[test]
    fn near_the_right_end_the_menu_moves_left_to_stay_inside() {
        assert_eq!(menu_span(500.0, 600.0), (340.0, 260.0, 260.0));
    }

    #[test]
    fn a_grid_narrower_than_the_menu_gets_a_menu_as_wide_as_itself() {
        assert_eq!(menu_span(100.0, 200.0), (0.0, 200.0, 200.0));
    }
}
