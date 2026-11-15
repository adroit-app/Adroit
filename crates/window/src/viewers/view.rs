//! The viewers as components, each drawn within the box it is given.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::desktop::use_asset_handler;
use dioxus::desktop::wry::http::{Response, StatusCode, header};
use dioxus::prelude::*;

use super::markdown::{
    Images, Unshown, image_id, image_response, link_target, markdown_document, render, stamp,
    theme_message, update_message,
};
use super::pdf;
use super::serve::{self, Scope};
use crate::themes::Document;
use adroit_core::viewers::FileKind;
use adroit_core::viewers::{
    Effect, ImageState, MarkdownState, Media, Scroll, ViewerAction, ViewerState, Zoom,
};

/// The viewers' look, put in the page with each viewer.
pub const CSS: &str = include_str!("../../assets/css/viewer.css");

/// An action handed to a viewer. `seq` tells one press from the next, so
/// the same action pressed twice runs twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    pub seq: u64,
    pub action: ViewerAction,
}

static NEXT: AtomicU64 = AtomicU64::new(1);

/// How a document's viewer was left in a window: its zoom and how far down
/// it was read.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Left {
    pub zoom: Zoom,
    pub top: f64,
}

/// How each document's viewer was left, by the window it was in, so a
/// window switched to the document's Neovim and back reads on where it was.
pub type LeftAt = HashMap<(u64, PathBuf), Left>;

/// The file at `path` in the viewer of its kind. Actions arrive through
/// `command`; `on_close` is called when one asks for the viewer to close.
/// A document reads the images it shows from under `root` alone, is painted
/// in Arto's `theme`, calls `on_link` with the file a link it shows leads
/// to, and, given the window it is in, comes back as it was left there.
#[component]
pub fn Viewer(
    path: PathBuf,
    root: PathBuf,
    theme: ReadSignal<Document>,
    command: ReadSignal<Option<Command>>,
    on_close: EventHandler<()>,
    on_link: EventHandler<PathBuf>,
    #[props(default)] left: Option<(u64, Signal<LeftAt>)>,
) -> Element {
    let key = path.to_string_lossy().into_owned();
    match ViewerState::new(FileKind::of(&path)) {
        // Keyed within a fragment: a key on the root of what a component
        // renders is not compared, and the viewer of the file shown before
        // would be kept, hooks and all, for the next one.
        Some(_) => rsx! {
            {std::iter::once(rsx! {
                ViewerOf { key: "{key}", path, root, theme, command, on_close, on_link, left }
            })}
        },
        None => rsx! {
            div { class: "viewer",
                style { {CSS} }
                ViewerEmpty { "No viewer shows this file" }
            }
        },
    }
}

/// The width a PDF page is drawn at, in pixels, for a page `factor` times
/// as large as the one that fits the window.
pub fn page_width(factor: f64) -> u32 {
    (1600.0 * factor).round().clamp(400.0, 8000.0) as u32
}

/// The page and width a request for a PDF page names, as
/// `page/<index>/<width>.png`.
pub fn page_request(rel: &str) -> Option<(usize, u32)> {
    let rest = rel.strip_prefix("page/")?;
    let (index, width) = rest.split_once('/')?;
    Some((
        index.parse().ok()?,
        width.strip_suffix(".png")?.parse().ok()?,
    ))
}

/// A script that scrolls the element `id` as `scroll` asks.
pub fn scroll_script(id: &str, scroll: Scroll) -> String {
    let how = match scroll {
        Scroll::Lines(n) => format!("e.scrollBy(0, {} * 40)", n),
        Scroll::HalfPages(n) => format!("e.scrollBy(0, {} * e.clientHeight / 2)", n),
        Scroll::Top => "e.scrollTo(0, 0)".into(),
        Scroll::Bottom => "e.scrollTo(0, e.scrollHeight)".into(),
    };
    format!("{{const e = document.getElementById({id:?}); if (e) {how};}}")
}

/// A script that hands the page in the frame `id` the message `message`,
/// a JSON object, for the script the page is served with.
pub fn frame_script(id: &str, message: &str) -> String {
    format!(
        "{{const f = document.getElementById({id:?}); \
         if (f && f.contentWindow) f.contentWindow.postMessage(Object.assign({{adroitViewer: 1}}, {message}), '*');}}"
    )
}

pub fn frame_scroll(scroll: Scroll) -> String {
    match scroll {
        Scroll::Lines(n) => format!("{{\"lines\": {n}}}"),
        Scroll::HalfPages(n) => format!("{{\"pages\": {n}}}"),
        Scroll::Top => "{\"scroll\": \"top\"}".into(),
        Scroll::Bottom => "{\"scroll\": \"bottom\"}".into(),
    }
}

fn media_script(id: &str, media: Media) -> String {
    let how = match media {
        Media::Play => "v.play()".to_string(),
        Media::Pause => "v.pause()".to_string(),
        Media::SeekTo(t) => format!("v.currentTime = {t}"),
    };
    format!("{{const v = document.getElementById({id:?}); if (v) {how};}}")
}

/// `seconds` as the movie viewer shows it.
pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0).floor() as u64;
    match (s / 3600, s / 60 % 60, s % 60) {
        (0, m, s) => format!("{m}:{s:02}"),
        (h, m, s) => format!("{h}:{m:02}:{s:02}"),
    }
}

fn answer(scope: &Scope, kind: FileKind, rel: &str, range: Option<&str>) -> Response<Vec<u8>> {
    let mut r = match (kind, page_request(rel)) {
        (FileKind::Pdf, Some((index, width))) => match pdf::page_png(scope.entry(), index, width) {
            Some(png) => {
                let mut r = Response::new(png);
                if let Ok(v) = "image/png".parse() {
                    r.headers_mut().insert(header::CONTENT_TYPE, v);
                }
                r
            }
            None => {
                let mut r = Response::new(Vec::new());
                *r.status_mut() = StatusCode::NOT_FOUND;
                r
            }
        },
        _ => serve::respond(scope, rel, range),
    };
    // A page in the HTML viewer runs with an origin of its own, and asks for
    // its scripts and data as from another origin.
    if let Ok(v) = "*".parse() {
        r.headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, v);
    }
    r
}

#[component]
fn ViewerOf(
    path: PathBuf,
    root: PathBuf,
    theme: ReadSignal<Document>,
    command: ReadSignal<Option<Command>>,
    on_close: EventHandler<()>,
    on_link: EventHandler<PathBuf>,
    left: Option<(u64, Signal<LeftAt>)>,
) -> Element {
    let kind = FileKind::of(&path);
    let prefix = use_hook(|| format!("adroit-viewer-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let scope = use_hook(|| {
        Arc::new(match kind {
            FileKind::Html => Scope::page(path.clone()),
            _ => Scope::File(path.clone()),
        })
    });
    let was = use_hook(|| {
        left.and_then(|(window, at)| at.peek().get(&(window, path.clone())).copied())
            .filter(|_| kind == FileKind::Markdown)
    });
    let mut state =
        use_signal(
            || match (ViewerState::new(kind).expect("a kind with a viewer"), was) {
                (ViewerState::Markdown(_), Some(was)) => {
                    ViewerState::Markdown(MarkdownState { zoom: was.zoom })
                }
                (fresh, _) => fresh,
            },
        );
    // How far down the document is read, as its frame last told.
    let top = use_hook(|| Rc::new(Cell::new(was.map_or(0.0, |w| w.top))));
    use_drop({
        let (path, top) = (path.clone(), top.clone());
        move || {
            if let (Some((window, mut at)), ViewerState::Markdown(s)) = (left, *state.peek()) {
                let left = Left {
                    zoom: s.zoom,
                    top: top.get(),
                };
                at.write().insert((window, path), left);
            }
        }
    });
    // A command sent before the viewer opened is not for it.
    let last = use_hook(|| Rc::new(Cell::new(command.peek().map_or(0, |c| c.seq))));
    let images: Images = use_hook(Images::default);

    use_asset_handler(&prefix, {
        let scope = scope.clone();
        let prefix = prefix.clone();
        let images = images.clone();
        move |request, responder| {
            let rel = request
                .uri()
                .path()
                .trim_start_matches('/')
                .strip_prefix(prefix.as_str())
                .unwrap_or_default()
                .trim_start_matches('/')
                .to_string();
            let range = request
                .headers()
                .get(header::RANGE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let scope = scope.clone();
            let images = images.clone();
            std::thread::spawn(move || {
                let r = match (kind, rel.strip_prefix("img/")) {
                    // A document is served to its frame whole; what the frame
                    // asks of the viewer is the images it shows, and nothing
                    // else of the disk.
                    (FileKind::Markdown, image) => {
                        let shown = images
                            .lock()
                            .ok()
                            .and_then(|i| i.get(image_id(image?)).cloned());
                        image_response(shown.as_ref())
                    }
                    _ => answer(&scope, kind, &rel, range.as_deref()),
                };
                responder.respond(r);
            });
        }
    });

    let mut page = use_signal(|| None::<Option<String>>);
    // The frame's document is set once: a frame given another reloads and
    // loses its scroll, so what changes later reaches it by message.
    let frame_doc = use_signal(|| None::<Result<String, Unshown>>);
    let gone = use_signal(|| false);
    let box_id = format!("{prefix}-box");
    let media_id = format!("{prefix}-media");
    use_hook(|| {
        let path = path.clone();
        match kind {
            FileKind::Markdown => {
                spawn(follow_markdown(Followed {
                    path: path.clone(),
                    root: root.clone(),
                    image_base: format!("{}{prefix}/img", serve::ORIGIN),
                    images: images.clone(),
                    frame: media_id.clone(),
                    theme,
                    shown: frame_doc,
                    gone,
                }));
                spawn(follow_frame(path, media_id.clone(), on_link, top.clone()));
            }
            FileKind::Pdf => {
                spawn(async move {
                    let pages = tokio::task::spawn_blocking(move || pdf::page_count(&path))
                        .await
                        .ok()
                        .flatten();
                    if let ViewerState::Pdf(s) = &mut *state.write() {
                        s.read(pages);
                    }
                });
            }
            FileKind::Html => {
                let base = format!("{}{prefix}/", serve::ORIGIN);
                spawn(async move {
                    let read = tokio::task::spawn_blocking(move || std::fs::read(&path))
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .map(|b| serve::page_document(&String::from_utf8_lossy(&b), &base));
                    page.set(Some(read));
                });
            }
            _ => {}
        }
    });

    use_effect({
        let media_id = media_id.clone();
        move || {
            let theme = theme();
            if kind == FileKind::Markdown {
                document::eval(&frame_script(&media_id, &theme_message(&theme)));
            }
        }
    });

    use_effect({
        let (box_id, media_id, path) = (box_id.clone(), media_id.clone(), path.clone());
        move || {
            let Some(cmd) = command() else {
                return;
            };
            if cmd.seq == last.get() {
                return;
            }
            last.set(cmd.seq);
            let effect = state.write().apply(cmd.action);
            let turned = matches!(cmd.action, ViewerAction::PageNext | ViewerAction::PagePrev);
            match effect {
                Effect::Scroll(s) if matches!(kind, FileKind::Html | FileKind::Markdown) => {
                    document::eval(&frame_script(&media_id, &frame_scroll(s)));
                }
                Effect::Scroll(s) => {
                    document::eval(&scroll_script(&box_id, s));
                }
                Effect::Redraw if turned => {
                    document::eval(&scroll_script(&box_id, Scroll::Top));
                }
                Effect::Media(m) => {
                    document::eval(&media_script(&media_id, m));
                }
                Effect::CopyPath => {
                    let _ = crate::clipboard::copy_to_clipboard(&path.to_string_lossy());
                }
                Effect::Close => on_close.call(()),
                Effect::Redraw | Effect::Reload | Effect::Nothing => {}
            }
        }
    });

    // The page in the frame is told of its zoom, as the frame's contents are
    // out of the window's reach.
    use_effect({
        let media_id = media_id.clone();
        move || {
            let zoom = match state() {
                ViewerState::Html(s) => s.zoom,
                ViewerState::Markdown(s) => s.zoom,
                _ => return,
            };
            document::eval(&frame_script(
                &media_id,
                &format!("{{\"zoom\": {}}}", zoom.factor()),
            ));
        }
    });

    let url = format!("/{prefix}/{}", scope.entry_url());
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let now = state();
    let detail = match now {
        ViewerState::Image(s) => format!("{}%", s.zoom.percent()),
        ViewerState::Html(s) => format!("{}%", s.zoom.percent()),
        ViewerState::Markdown(s) if gone() => format!("deleted · {}%", s.zoom.percent()),
        ViewerState::Markdown(s) => format!("{}%", s.zoom.percent()),
        ViewerState::Pdf(s) => match s.pages {
            Some(n) => format!("{} / {} · {}%", s.page + 1, n, s.zoom.percent()),
            None => "—".into(),
        },
        ViewerState::Movie(s) => match s.duration {
            Some(d) => format!("{} / {}", clock(s.position), clock(d)),
            None => clock(s.position),
        },
    };
    let body = match now {
        ViewerState::Image(s) => {
            let pct = s.zoom.factor() * 100.0;
            rsx! {
                div { class: "viewer-stage", style: "width: {pct}%; height: {pct}%;",
                    img { class: "viewer-fit", src: "{url}", alt: "{name}" }
                }
            }
        }
        ViewerState::Pdf(s) => {
            let pct = s.zoom.factor() * 100.0;
            match s.pages {
                Some(_) => {
                    let page = format!(
                        "/{prefix}/page/{}/{}.png",
                        s.page,
                        page_width(s.zoom.factor())
                    );
                    rsx! {
                        div { class: "viewer-stage", style: "width: {pct}%; height: {pct}%;",
                            img { class: "viewer-fit", src: "{page}", alt: "{name}" }
                        }
                    }
                }
                None => rsx! {
                    ViewerEmpty { "This PDF cannot be shown" }
                },
            }
        }
        ViewerState::Movie(_) => {
            let media_id = media_id.clone();
            rsx! {
                video {
                    id: "{media_id}",
                    class: "viewer-movie",
                    src: "{url}",
                    preload: "metadata",
                    controls: true,
                    onmounted: move |_| {
                        let media_id = media_id.clone();
                        async move {
                            let mut heard = document::eval(&format!(
                                "const v = document.getElementById({media_id:?});
                                 const send = () => dioxus.send([!v.paused, v.currentTime,
                                   isFinite(v.duration) ? v.duration : null]);
                                 for (const e of ['timeupdate', 'play', 'pause', 'seeked',
                                   'loadedmetadata', 'durationchange']) v.addEventListener(e, send);
                                 send();
                                 await new Promise(() => {{}});"
                            ));
                            while let Ok((playing, at, length)) =
                                heard.recv::<(bool, f64, Option<f64>)>().await
                            {
                                if let ViewerState::Movie(s) = &mut *state.write() {
                                    s.observe(playing, at, length);
                                }
                            }
                        }
                    },
                }
            }
        }
        ViewerState::Html(_) if page.read().is_none() => rsx! {},
        ViewerState::Html(_) if page.read().as_ref().is_some_and(Option::is_none) => rsx! {
            ViewerEmpty { "This page cannot be read" }
        },
        ViewerState::Html(s) => {
            let document = page.read().clone().flatten().unwrap_or_default();
            page_frame(
                media_id.clone(),
                box_id.clone(),
                s.zoom.factor(),
                0.0,
                None,
                document,
            )
        }
        ViewerState::Markdown(s) => match frame_doc.read().clone() {
            None => rsx! {},
            Some(Err(why)) => rsx! {
                ViewerEmpty { "{why.says()}" }
            },
            Some(Ok(document)) => page_frame(
                media_id.clone(),
                box_id.clone(),
                s.zoom.factor(),
                top.get(),
                Some(theme.read().scheme),
                document,
            ),
        },
    };
    rsx! {
        div { class: "viewer",
            style { {CSS} }
            div { id: "{box_id}", class: "viewer-box", {body} }
            div { class: "viewer-status",
                span { class: "name", "{name}" }
                span { "{detail}" }
            }
        }
    }
}

/// What the Markdown viewer follows of its file, and where it shows it.
struct Followed {
    path: PathBuf,
    root: PathBuf,
    image_base: String,
    images: Images,
    /// The frame's element id.
    frame: String,
    theme: ReadSignal<Document>,
    /// The frame's document, set once it is first rendered.
    shown: Signal<Option<Result<String, Unshown>>>,
    /// The file was removed after it was shown.
    gone: Signal<bool>,
}

/// How often the viewer looks whether its file changed.
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_millis(500);

/// Render the document whenever its file changes, for as long as the viewer
/// is open. The first render becomes the frame's document; later ones reach
/// the frame by message, so it keeps the place read. A file that cannot be
/// rendered again, or is removed, leaves what was shown last.
async fn follow_markdown(mut f: Followed) {
    let mut seen = None;
    loop {
        let now = {
            let path = f.path.clone();
            tokio::task::spawn_blocking(move || stamp(&path))
                .await
                .ok()
                .flatten()
        };
        if now != seen {
            let shown = matches!(*f.shown.peek(), Some(Ok(_)));
            match now {
                None if shown => f.gone.set(true),
                None => f.shown.set(Some(Err(Unshown::Unreadable))),
                Some(_) => {
                    f.gone.set(false);
                    let rendered = {
                        let (path, root, base) =
                            (f.path.clone(), f.root.clone(), f.image_base.clone());
                        tokio::task::spawn_blocking(move || render(&path, &root, base))
                            .await
                            .unwrap_or(Err(Unshown::Unrenderable))
                    };
                    match rendered {
                        Ok(r) => {
                            if shown {
                                document::eval(&frame_script(&f.frame, &update_message(&r)));
                            } else {
                                let doc = markdown_document(&r.html, r.libraries, &f.theme.read());
                                f.shown.set(Some(Ok(doc)));
                            }
                            if let Ok(mut i) = f.images.lock() {
                                *i = r.images;
                            }
                        }
                        Err(_) if shown => {}
                        Err(why) => f.shown.set(Some(Err(why))),
                    }
                }
            }
            seen = now;
        }
        tokio::time::sleep(LOOK_EVERY).await;
    }
}

/// Hear what the frame `frame`, which shows the document at `path`, tells:
/// the file each link pressed leads to, handed to `on_link` when there is
/// one, and how far down it is scrolled, kept in `top`.
async fn follow_frame(
    path: PathBuf,
    frame: String,
    on_link: EventHandler<PathBuf>,
    top: Rc<Cell<f64>>,
) {
    let mut heard = document::eval(&format!(
        "window.addEventListener('message', (e) => {{
           const f = document.getElementById({frame:?});
           if (!f || e.source !== f.contentWindow) return;
           const m = e.data;
           if (!m) return;
           if (m.adroitViewerLink === 1 && typeof m.path === 'string') dioxus.send(['link', m.path]);
           if (m.adroitViewerTop === 1 && typeof m.top === 'number') dioxus.send(['top', m.top]);
         }});
         await new Promise(() => {{}});"
    ));
    while let Ok((what, value)) = heard.recv::<(String, serde_json::Value)>().await {
        match (what.as_str(), value) {
            ("link", serde_json::Value::String(written)) => {
                if let Some(file) = link_target(&path, &written).filter(|f| f.is_file()) {
                    on_link.call(file);
                }
            }
            ("top", value) => top.set(value.as_f64().unwrap_or_default()),
            _ => {}
        }
    }
}

/// The frame `media_id` that draws `document` apart from the window, at
/// `zoom` and scrolled `top` down, handing the keys pressed in it to the
/// viewer's box `box_id`. A page of the window's own `scheme`, `light` or
/// `dark`, is framed in nothing of its own; any other is framed in white,
/// as a browser frames a page.
pub fn page_frame(
    media_id: String,
    box_id: String,
    zoom: f64,
    top: f64,
    scheme: Option<&'static str>,
    document: String,
) -> Element {
    let relay_id = media_id.clone();
    let (class, style) = match scheme {
        Some(scheme) => (
            "viewer-frame viewer-document",
            format!("color-scheme: {scheme}"),
        ),
        None => ("viewer-frame", String::new()),
    };
    rsx! {
        iframe {
            id: "{media_id}",
            class,
            style,
            "srcdoc": "{document}",
            "sandbox": "allow-scripts allow-forms",
            onload: move |_| {
                document::eval(&frame_script(
                    &media_id,
                    &format!("{{\"zoom\": {zoom}, \"top\": {top}}}"),
                ));
            },
            onmounted: move |_| {
                // Keys pressed in the page come back to the viewer,
                // which hands them on as if pressed on itself.
                document::eval(&format!(
                    "window.addEventListener('message', (e) => {{
                       const f = document.getElementById({relay_id:?});
                       const b = document.getElementById({box_id:?});
                       if (!f || !b || e.source !== f.contentWindow) return;
                       const m = e.data;
                       if (!m || m.adroitViewerKey !== 1) return;
                       b.dispatchEvent(new KeyboardEvent('keydown', {{key: m.key, code: m.code,
                         ctrlKey: m.ctrlKey, metaKey: m.metaKey, altKey: m.altKey,
                         shiftKey: m.shiftKey, bubbles: true, cancelable: true}}));
                     }});"
                ));
            },
        }
    }
}

/// An image a console drew, named `name`, in the image viewer, drawn from
/// `src`, the copy the window keeps, or saying it is gone once that is.
#[component]
pub fn DrawnViewer(
    src: Option<Arc<str>>,
    name: String,
    command: ReadSignal<Option<Command>>,
    on_close: EventHandler<()>,
) -> Element {
    let prefix = use_hook(|| format!("adroit-viewer-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let box_id = format!("{prefix}-box");
    let mut state = use_signal(ImageState::default);
    let last = use_hook(|| Rc::new(Cell::new(command.peek().map_or(0, |c| c.seq))));
    use_effect({
        let box_id = box_id.clone();
        move || {
            let Some(cmd) = command() else {
                return;
            };
            if cmd.seq == last.get() {
                return;
            }
            last.set(cmd.seq);
            let mut now = ViewerState::Image(state());
            let effect = now.apply(cmd.action);
            if let ViewerState::Image(s) = now {
                state.set(s);
            }
            match effect {
                Effect::Scroll(s) => {
                    document::eval(&scroll_script(&box_id, s));
                }
                Effect::Close => on_close.call(()),
                // A drawn image has no path to copy, and nothing to play.
                Effect::CopyPath
                | Effect::Media(_)
                | Effect::Redraw
                | Effect::Reload
                | Effect::Nothing => {}
            }
        }
    });
    let zoom = state().zoom;
    let pct = zoom.factor() * 100.0;
    let body = match src {
        Some(src) => rsx! {
            div { class: "viewer-stage", style: "width: {pct}%; height: {pct}%;",
                img { class: "viewer-fit", src: "{src}", alt: "{name}" }
            }
        },
        None => rsx! {
            ViewerEmpty { "This image is no longer kept" }
        },
    };
    rsx! {
        div { class: "viewer",
            style { {CSS} }
            div { id: "{box_id}", class: "viewer-box", {body} }
            div { class: "viewer-status",
                span { class: "name", "{name}" }
                span { "{zoom.percent()}%" }
            }
        }
    }
}

/// What a viewer says in place of a file it cannot show.
#[component]
fn ViewerEmpty(children: Element) -> Element {
    rsx! {
        div { class: "viewer-empty", {children} }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pdf_page_is_asked_for_by_index_and_width() {
        assert_eq!(page_request("page/3/1600.png"), Some((3, 1600)));
        assert_eq!(page_request("page/3/1600"), None);
        assert_eq!(page_request("paper.pdf"), None);
        assert_eq!(page_request("page/x/1600.png"), None);
    }

    #[test]
    fn a_pdf_page_is_drawn_sharper_as_it_is_zoomed_within_bounds() {
        assert_eq!(page_width(1.0), 1600);
        assert_eq!(page_width(2.0), 3200);
        assert_eq!(page_width(0.1), 400);
        assert_eq!(page_width(100.0), 8000);
    }

    #[test]
    fn a_movie_s_time_is_shown_in_minutes_and_hours_when_it_has_them() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(75.9), "1:15");
        assert_eq!(clock(3725.0), "1:02:05");
        assert_eq!(clock(-3.0), "0:00");
    }

    #[test]
    fn a_scroll_reaches_only_the_element_it_names() {
        let s = scroll_script("box\"1", Scroll::HalfPages(-1));
        assert!(s.contains(r#"getElementById("box\"1")"#), "{s}");
        assert!(s.contains("-1 * e.clientHeight / 2"), "{s}");
    }
}
