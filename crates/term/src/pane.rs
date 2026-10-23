//! A window onto one session in the daemon: the screen and images it sends,
//! mirrored as they change, and the input sent to it.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;

use crate::input::Event as _;
use felis_client_core::renderer_effect::{RendererEffect, renderer_effect};
use felis_client_core::{
    AttachIntent, Connection, ImageId, ImageLayout, ImageShadow, OutgoingFrame, ShadowScreen,
};
use felis_grid::image_layout::ImageDraw;
use felis_protocol::MessageKind;
use felis_protocol::messages::{
    ConnToClientMsg, GridMsg, ImageMsg, ImageTarget, InputMods, InputMsg, MouseAction, MouseButton,
    MouseEvent, PushMsg, RequestedDims, SessionInfo, SessionToDaemonMsg, ThemeChannel,
};
use felis_transport::local::{ReadHalf, WriteHalf};
use felis_transport::{Delivery, Incoming};
use tokio::sync::mpsc;

use crate::daemon::{Daemon, Error, Result, Spawn, spawn_args};
use crate::screen::{Overrides, screen_of};
use crate::{Key, Palette, Rgb, Screen};

/// How large a pane is, in cells and in the pixels of a cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
    /// A cell's pixels as drawn.
    pub cell_width: f64,
    pub cell_height: f64,
}

impl Size {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols,
            rows,
            cell_width: 8.0,
            cell_height: 17.0,
        }
    }

    /// A cell's pixels as the daemon knows them, whole: it divides the
    /// grid's pixels by its cells and drops the rest, so a fraction told to
    /// it is lost there, and an image would take rows by one size and be
    /// drawn by another.
    pub(crate) fn cell_px(self) -> (u32, u32) {
        let whole = |v: f64| (v.round() as u32).max(1);
        (whole(self.cell_width), whole(self.cell_height))
    }

    /// The size as the daemon takes it.
    pub(crate) fn dims(self) -> RequestedDims {
        let (cols, rows) = (u32::from(self.cols.max(1)), u32::from(self.rows.max(1)));
        let (w, h) = self.cell_px();
        RequestedDims {
            rows,
            cols,
            pixel_w: cols * w,
            pixel_h: rows * h,
        }
    }
}

/// A part of an image drawn over the screen: `src` of its own pixels,
/// `size` in all, drawn at `dst`, counted in cells from the top left;
/// both are `[x, y, width, height]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub url: Arc<str>,
    pub size: [u32; 2],
    pub src: [f32; 4],
    pub dst: [f32; 4],
}

/// An image a program sent, as it first came in whole: straight RGBA8
/// pixels, `width` by `height`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drawn {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The most images waiting to be taken; past it the oldest give way, so a
/// pane nobody takes from holds no more.
const WAITING: usize = 8;

type Waker = Arc<dyn Fn() + Send + Sync>;

struct State {
    shadow: ShadowScreen,
    images: ImageShadow,
    layout: ImageLayout,
    overrides: Overrides,
    palette: Palette,
    /// Images begun anew, not yet in whole.
    fresh: HashSet<ImageId>,
    /// Images in whole since they were last taken, oldest first.
    drawn: VecDeque<Drawn>,
    /// Each image as a `data:` URL, made once for what it shows.
    urls: HashMap<ImageId, (u64, Arc<str>)>,
    exited: bool,
    /// A cell's pixels as the daemon was last told them.
    cell: (f32, f32),
    closed: bool,
}

/// A window onto one session.
enum WriteRequest {
    Frame(OutgoingFrame),
    Fence(tokio::sync::oneshot::Sender<std::result::Result<(), String>>),
}

pub struct Pane {
    id: u128,
    last_input_ms: AtomicU64,
    state: Arc<Mutex<State>>,
    out: mpsc::UnboundedSender<WriteRequest>,
    reader: tokio::task::JoinHandle<()>,
}

impl Drop for Pane {
    fn drop(&mut self) {
        self.send_session(&SessionToDaemonMsg::Detach);
        self.reader.abort();
    }
}

fn theme(palette: &Palette) -> SessionToDaemonMsg {
    let rgb = |c: Rgb| (c.r, c.g, c.b);
    SessionToDaemonMsg::ConfigureTheme {
        fg: Some(rgb(palette.foreground)),
        bg: Some(rgb(palette.background)),
        cursor: Some(rgb(palette.cursor)),
        palette: (0..16u8)
            .map(|i| (i, rgb(palette.indexed(i))))
            .collect::<BTreeMap<_, _>>(),
    }
}

/// Holds a program back until a line comes in. felis runs a session's
/// program the moment it makes the session, before a window can tell the
/// session its colours, so a program asking for them at once would race the
/// window; held, it cannot ask before the line, which is sent after them.
/// The line's echo moves the cursor down, and is taken back.
const GATE: &str = r#"IFS= read -r _; printf '\033[H'; exec "$0" "$@""#;

/// What lets a [`gated`] program go.
const RELEASE: &[u8] = b"\r";

/// `spawn`, its program held back until [`RELEASE`] comes in.
fn gated(spawn: Spawn) -> Spawn {
    let args = ["-c".to_string(), GATE.to_string(), spawn.program]
        .into_iter()
        .chain(spawn.args)
        .collect();
    Spawn {
        program: "/bin/sh".into(),
        args,
        ..spawn
    }
}

/// Whether `spawn`'s program is one that can run, looked up on the `PATH`
/// it is started with.
fn found(spawn: &Spawn) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let runs = |p: &std::path::Path| {
        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if spawn.program.contains('/') {
        return runs(&spawn.cwd.join(&spawn.program));
    }
    let path = spawn
        .env
        .iter()
        .rev()
        .find(|(k, _)| k == "PATH")
        .map(|(_, v)| v.clone())
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
    std::env::split_paths(&path).any(|dir| runs(&dir.join(&spawn.program)))
}

fn dark(palette: &Palette) -> bool {
    let lum = |c: Rgb| 0.2126 * f64::from(c.r) + 0.7152 * f64::from(c.g) + 0.0722 * f64::from(c.b);
    lum(palette.background) < lum(palette.foreground)
}

impl Pane {
    /// Attach to session `id` at `size`, drawn in `palette`. `waker` runs
    /// on another task whenever the screen changes or the session ends.
    pub async fn attach(
        daemon: &Daemon,
        id: u128,
        size: Size,
        palette: &Palette,
        waker: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self> {
        let mut conn = daemon.window().await?;
        let info = conn.attach(id, AttachIntent::Deliberate).await?;
        Ok(Self::over(conn, &info, size, palette, waker))
    }

    /// Start a session as `spawn` says, at `size` rather than the size it
    /// names, and attach to it, drawn in `palette`. `waker_for` makes the
    /// waker [`Pane::attach`] takes, given the session's id. The program
    /// runs only once the session has the palette, so the first colour it
    /// asks for is the palette's.
    pub async fn start<W>(
        daemon: &Daemon,
        spawn: Spawn,
        size: Size,
        palette: &Palette,
        waker_for: impl FnOnce(u128) -> W,
    ) -> Result<Self>
    where
        W: Fn() + Send + Sync + 'static,
    {
        Self::start_prepared(daemon, spawn, size, palette, waker_for, |_| async {
            Ok(())
        })
        .await
    }

    /// Record ownership while the child is gated, before it can edit or report.
    /// A refused preparation destroys this newly created session.
    pub async fn start_prepared<W, F>(
        daemon: &Daemon,
        spawn: Spawn,
        size: Size,
        palette: &Palette,
        waker_for: impl FnOnce(u128) -> W,
        prepare: impl FnOnce(u128) -> F,
    ) -> Result<Self>
    where
        W: Fn() + Send + Sync + 'static,
        F: std::future::Future<Output = Result<()>>,
    {
        // Behind the gate the daemon starts a shell, which is always there,
        // so a program missing is told here, as the daemon would have.
        if !found(&spawn) {
            return Err(Error::NotFound(spawn.program));
        }
        let mut args = spawn_args(gated(spawn));
        args.dims = Some(size.dims());
        let mut conn = daemon.window().await?;
        let info = conn.create_with(args).await?;
        if let Err(error) = prepare(info.id).await {
            return match daemon.end(info.id).await {
                Ok(()) => Err(Error::PreparationAborted {
                    session: info.id,
                    reason: error.to_string(),
                }),
                Err(stopping) => Err(Error::Protocol(format!(
                    "{error}; cannot confirm stopping the startup gate: {stopping}"
                ))),
            };
        }
        let waker = waker_for(info.id);
        let pane = Self::over(conn, &info, size, palette, waker);
        pane.send(InputMsg::KeyBytes(RELEASE.to_vec()));
        pane.input_fence().await?;
        Ok(pane)
    }

    /// The pane over `conn`, attached to the session `info` lists.
    fn over(
        conn: Connection<ReadHalf, WriteHalf>,
        info: &SessionInfo,
        size: Size,
        palette: &Palette,
        waker: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let id = info.id;
        let state = Arc::new(Mutex::new(State {
            shadow: ShadowScreen::new(info.dims.rows, info.dims.cols),
            images: ImageShadow::new(),
            layout: ImageLayout::new(),
            overrides: Overrides::default(),
            palette: palette.clone(),
            fresh: HashSet::new(),
            drawn: VecDeque::new(),
            urls: HashMap::new(),
            exited: info.exited,
            cell: {
                let (w, h) = size.cell_px();
                (w as f32, h as f32)
            },
            closed: false,
        }));
        let mut conn = conn;
        let (out, mut queue) = mpsc::unbounded_channel::<WriteRequest>();
        let waker: Waker = Arc::new(waker);
        let reading = state.clone();
        let reader = tokio::spawn(async move {
            loop {
                let frame = tokio::select! {
                    request = queue.recv() => {
                        match request {
                            Some(WriteRequest::Frame(frame)) => {
                                if conn.writer.send_checked_unflushed(frame.frame()).await.is_err()
                                    || conn.writer.flush().await.is_err()
                                {
                                    break;
                                }
                            }
                            Some(WriteRequest::Fence(reply)) => {
                                let result = conn.input_fence().await.map_err(|error| error.to_string());
                                let failed = result.is_err();
                                let _ = reply.send(result);
                                if failed {
                                    break;
                                }
                            }
                            None => break,
                        }
                        continue;
                    }
                    frame = conn.next_frame() => {
                        match frame {
                            Ok(Some(frame)) => frame,
                            _ => break,
                        }
                    }
                };
                let Ok(incoming) = conn.driver.classify(&frame) else {
                    break;
                };
                let p = match incoming {
                    Incoming::Payload(p) => p,
                    Incoming::Control(ConnToClientMsg::Error { .. }) => break,
                    _ => continue,
                };
                let mut s = reading.lock().unwrap();
                let ok = match p.kind {
                    MessageKind::Grid => match conn.driver.decode::<GridMsg>(&p) {
                        Ok(Delivery::Deliver(d)) => s.grid(&d.msg),
                        Ok(_) => true,
                        Err(_) => false,
                    },
                    MessageKind::Image => match conn.driver.decode::<ImageMsg>(&p) {
                        Ok(Delivery::Deliver(d)) => s.image(&d.msg),
                        Ok(_) => true,
                        Err(_) => false,
                    },
                    MessageKind::Push => match conn.driver.decode::<PushMsg>(&p) {
                        Ok(Delivery::Deliver(d)) => {
                            if matches!(d.msg, PushMsg::SessionExited { .. }) {
                                s.exited = true;
                            }
                            true
                        }
                        Ok(_) => true,
                        Err(_) => false,
                    },
                    _ => true,
                };
                let drawn = !s.shadow.is_rehydrating();
                drop(s);
                if !ok {
                    break;
                }
                if drawn {
                    waker();
                }
            }
            reading.lock().unwrap().closed = true;
            waker();
        });
        let pane = Self {
            id,
            last_input_ms: AtomicU64::new(0),
            state,
            out,
            reader,
        };
        pane.send_session(&theme(palette));
        pane.send(InputMsg::ColorScheme {
            dark: dark(palette),
        });
        pane.resize(size);
        pane
    }

    /// The latest key, text, paste or mouse input sent to the program since
    /// this pane attached, in milliseconds since the Unix epoch.
    pub fn last_input_ms(&self) -> Option<u64> {
        match self.last_input_ms.load(Ordering::Relaxed) {
            0 => None,
            at => Some(at),
        }
    }

    fn record_input(&self) {
        if let Ok(elapsed) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            self.last_input_ms
                .fetch_max(elapsed.as_millis() as u64, Ordering::Relaxed);
        }
    }

    pub fn id(&self) -> u128 {
        self.id
    }

    async fn input_fence(&self) -> Result<()> {
        let (reply, accepted) = tokio::sync::oneshot::channel();
        self.out
            .send(WriteRequest::Fence(reply))
            .map_err(|_| Error::Protocol("the terminal writer stopped".into()))?;
        accepted
            .await
            .map_err(|_| Error::Protocol("the terminal stopped before accepting input".into()))?
            .map_err(Error::Protocol)
    }

    fn send(&self, msg: InputMsg) {
        if let Ok(frame) = OutgoingFrame::input(&msg) {
            let _ = self.out.send(WriteRequest::Frame(frame));
        }
    }

    fn send_session(&self, msg: &SessionToDaemonMsg) {
        if let Ok(frame) = OutgoingFrame::ordered(msg) {
            let _ = self.out.send(WriteRequest::Frame(frame));
        }
    }

    /// The screen as the view is scrolled now.
    pub fn screen(&self) -> Screen {
        let s = self.state.lock().unwrap();
        screen_of(
            s.shadow.screen(),
            &s.palette,
            &s.overrides,
            s.shadow.viewport() == 0,
        )
    }

    /// Everything its session keeps, as [`Daemon::everything`] reads it, in
    /// the colours this pane draws: the palette with what the program set
    /// over it.
    pub async fn everything(&self, daemon: &Daemon, cols: u16) -> Result<Screen> {
        let (palette, overrides) = {
            let s = self.state.lock().unwrap();
            (s.palette.clone(), s.overrides.clone())
        };
        daemon
            .everything_in(self.id, cols, &palette, &overrides)
            .await
    }

    /// The images over the screen as the view is scrolled now, back to
    /// front, laid out in cells of the pixels the daemon took rows for them
    /// by.
    pub fn pictures(&self) -> Vec<Picture> {
        let mut guard = self.state.lock().unwrap();
        let s = &mut *guard;
        let viewport = s.shadow.viewport();
        let cell = s.cell;
        s.images
            .layout_into(&mut s.layout, s.shadow.screen(), cell, viewport);
        let draws: Vec<ImageDraw> = s
            .layout
            .under_bg()
            .iter()
            .chain(s.layout.under_text())
            .chain(s.layout.above_text())
            .copied()
            .collect();
        let mut out = Vec::new();
        for d in merged(draws) {
            let Some(url) = s.url(d.image_id) else {
                continue;
            };
            let [x, y, w, h] = d.dst;
            out.push(Picture {
                url,
                size: d.image_size,
                src: d.src,
                dst: [x / cell.0, y / cell.1, w / cell.0, h / cell.1],
            });
        }
        let live: Vec<ImageId> = s.layout.live_images().to_vec();
        s.urls.retain(|id, _| live.contains(id));
        out
    }

    /// The images the program sent since they were last taken, oldest
    /// first, each once, whether still shown or not.
    pub fn take_drawn(&self) -> Vec<Drawn> {
        self.state.lock().unwrap().drawn.drain(..).collect()
    }

    /// Send a key, as the program's modes ask; `false` when the key never
    /// reaches a program.
    pub fn key(&self, key: &Key) -> bool {
        match key.event() {
            Some(e) => {
                self.to_bottom();
                self.send(InputMsg::Key(e));
                self.record_input();
                true
            }
            None => false,
        }
    }

    /// Send text as if it were typed.
    pub fn type_text(&self, text: &str) {
        self.to_bottom();
        self.send(InputMsg::KeyBytes(text.as_bytes().to_vec()));
        if !text.is_empty() {
            self.record_input();
        }
    }

    /// Paste text, bracketed when the program asks for it.
    pub fn paste(&self, text: &str) {
        self.to_bottom();
        self.send(InputMsg::Paste(text.as_bytes().to_vec()));
        self.record_input();
    }

    /// Show the bottom again, as a terminal does when typed into.
    fn to_bottom(&self) {
        if self.state.lock().unwrap().shadow.viewport() > 0 {
            self.send(InputMsg::Viewport {
                lines_from_bottom: 0,
                anchor: None,
            });
        }
    }

    /// Whether the program takes the mouse's wheel and clicks itself.
    pub fn takes_mouse(&self) -> bool {
        self.state.lock().unwrap().shadow.mouse_protocol_active()
    }

    fn mouse(&self, button: MouseButton, action: MouseAction, col: usize, row: usize) {
        let at = |n: usize| u16::try_from(n + 1).unwrap_or(u16::MAX);
        self.send(InputMsg::Mouse(MouseEvent {
            button: Some(button),
            action,
            mods: InputMods::empty(),
            x: at(col),
            y: at(row),
            px: 0,
            py: 0,
        }));
    }

    /// Click the cell at `col` and `row`, counted from 0, for a program
    /// that asked for the mouse.
    pub fn click(&self, col: usize, row: usize) {
        if self.takes_mouse() {
            self.record_input();
        }
        self.mouse(MouseButton::Left, MouseAction::Press, col, row);
        self.mouse(MouseButton::Left, MouseAction::Release, col, row);
    }

    /// Turn the wheel `lines` notches, up when positive, over `col` and
    /// `row`: to a program that asked for the mouse, as arrow keys to one
    /// on the alternate screen, and otherwise through the scrollback.
    pub fn wheel(&self, lines: i32, col: usize, row: usize) {
        let (mouse, alternate) = {
            let s = self.state.lock().unwrap();
            (s.shadow.mouse_protocol_active(), s.shadow.alt_screen())
        };
        let notches = lines.unsigned_abs().min(200);
        if notches > 0 && (mouse || alternate) {
            self.record_input();
        }
        if mouse {
            let button = if lines > 0 {
                MouseButton::WheelUp
            } else {
                MouseButton::WheelDown
            };
            for _ in 0..notches {
                self.mouse(button, MouseAction::Press, col, row);
            }
        } else if alternate {
            let arrow = Key::new(if lines > 0 { "ArrowUp" } else { "ArrowDown" });
            for _ in 0..notches {
                self.key(&arrow);
            }
        } else {
            self.scroll(lines);
        }
    }

    /// Scroll the view `lines` back through the scrollback, forward when
    /// negative.
    pub fn scroll(&self, lines: i32) {
        let (at, anchor) = {
            let s = self.state.lock().unwrap();
            (s.shadow.viewport(), s.shadow.viewport_anchor())
        };
        let to = at.saturating_add_signed(lines);
        self.send(InputMsg::Viewport {
            lines_from_bottom: to,
            anchor: if to == 0 { None } else { anchor },
        });
    }

    /// How many lines back the view is scrolled.
    pub fn scrolled(&self) -> u32 {
        self.state.lock().unwrap().shadow.viewport()
    }

    pub fn resize(&self, size: Size) {
        {
            let mut s = self.state.lock().unwrap();
            s.shadow.local_resize(size.rows.max(1), size.cols.max(1));
            let (w, h) = size.cell_px();
            s.cell = (w as f32, h as f32);
        }
        self.send(InputMsg::Resize { dims: size.dims() });
    }

    /// Draw by `palette`, and tell the program of it.
    pub fn set_palette(&self, palette: &Palette) {
        self.state.lock().unwrap().palette = palette.clone();
        self.send_session(&theme(palette));
        self.send(InputMsg::ColorScheme {
            dark: dark(palette),
        });
    }

    /// Whether the program wants pastes bracketed, which a program ready
    /// for input turns on.
    pub fn bracketed_paste(&self) -> bool {
        self.state.lock().unwrap().shadow.bracketed_paste()
    }

    /// Whether the program has exited, or the daemon let the pane go.
    pub fn exited(&self) -> bool {
        let s = self.state.lock().unwrap();
        s.exited || s.closed
    }
}

impl State {
    /// Take one grid message in; `false` when it cannot be, and the
    /// connection has to end.
    fn grid(&mut self, msg: &GridMsg) -> bool {
        if self.shadow.apply(msg).is_err() {
            return false;
        }
        let rgb = |(r, g, b)| Rgb::new(r, g, b);
        match renderer_effect(msg) {
            Some(RendererEffect::ThemeOverride { channel, rgb: c }) => match channel {
                ThemeChannel::Foreground => self.overrides.foreground = c.map(rgb),
                ThemeChannel::Background => self.overrides.background = c.map(rgb),
                _ => {}
            },
            Some(RendererEffect::PaletteOverride { index, rgb: c }) => match c {
                Some(c) => {
                    self.overrides.indexed.insert(index, rgb(c));
                }
                None => {
                    self.overrides.indexed.remove(&index);
                }
            },
            Some(RendererEffect::ResetPalette) => self.overrides.indexed.clear(),
            Some(RendererEffect::ResetSessionColors) => {
                let reverse = self.overrides.reverse;
                self.overrides = Overrides {
                    reverse,
                    ..Overrides::default()
                };
            }
            Some(RendererEffect::ReverseVideo(on)) => self.overrides.reverse = on,
            None => {}
        }
        true
    }

    /// Take one image message in, keeping an image as it comes in whole;
    /// `false` when it cannot be, and the connection has to end.
    fn image(&mut self, msg: &ImageMsg) -> bool {
        if self.images.apply(msg).is_err() {
            return false;
        }
        match msg {
            ImageMsg::Header {
                id,
                target: ImageTarget::New { .. },
            } => {
                self.fresh.insert(*id);
            }
            ImageMsg::Delete { id } => {
                self.fresh.remove(id);
            }
            ImageMsg::Complete { id } if self.fresh.remove(id) => {
                if let Some(image) = self.images.image(*id)
                    && image.is_complete()
                {
                    let mut rgba = Vec::new();
                    image.write_rgba8(&mut rgba);
                    if self.drawn.len() == WAITING {
                        self.drawn.pop_front();
                    }
                    self.drawn.push_back(Drawn {
                        width: image.width,
                        height: image.height,
                        rgba,
                    });
                }
            }
            _ => {}
        }
        true
    }

    fn url(&mut self, id: ImageId) -> Option<Arc<str>> {
        let image = self.images.image(id)?;
        let generation = image.generation();
        if let Some((g, url)) = self.urls.get(&id)
            && *g == generation
        {
            return Some(url.clone());
        }
        let mut pixels = Vec::new();
        image.write_rgba8(&mut pixels);
        let url = data_url(image.width, image.height, &pixels)?;
        self.urls.insert(id, (generation, url.clone()));
        Some(url)
    }
}

/// RGBA pixels as a PNG `data:` URL.
fn data_url(width: u32, height: u32, rgba: &[u8]) -> Option<Arc<str>> {
    let mut bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut bytes, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().ok()?;
        w.write_image_data(rgba).ok()?;
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:image/png;base64,{b64}").into())
}

/// Draws of an image that meet edge to edge, on the screen and in the
/// image alike, as one: an image put by placeholder cells comes a cell at
/// a time, and the window draws each draw as an element of its own.
fn merged(draws: Vec<ImageDraw>) -> Vec<ImageDraw> {
    let meets = |a: f32, b: f32| (a - b).abs() < 1e-3;
    let mut rows: Vec<ImageDraw> = Vec::new();
    for d in draws {
        match rows.last_mut() {
            Some(l)
                if l.image_id == d.image_id
                    && meets(l.dst[1], d.dst[1])
                    && meets(l.dst[3], d.dst[3])
                    && meets(l.dst[0] + l.dst[2], d.dst[0])
                    && meets(l.src[1], d.src[1])
                    && meets(l.src[3], d.src[3])
                    && meets(l.src[0] + l.src[2], d.src[0]) =>
            {
                l.dst[2] += d.dst[2];
                l.src[2] += d.src[2];
            }
            _ => rows.push(d),
        }
    }
    let mut out: Vec<ImageDraw> = Vec::new();
    for d in rows {
        let below = out.iter_mut().rev().find(|l| {
            l.image_id == d.image_id
                && meets(l.dst[0], d.dst[0])
                && meets(l.dst[2], d.dst[2])
                && meets(l.src[0], d.src[0])
                && meets(l.src[2], d.src[2])
                && meets(l.dst[1] + l.dst[3], d.dst[1])
                && meets(l.src[1] + l.src[3], d.src[1])
        });
        match below {
            Some(l) => {
                l.dst[3] += d.dst[3];
                l.src[3] += d.src[3];
            }
            None => out.push(d),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(id: u32, x: f32, y: f32) -> ImageDraw {
        ImageDraw {
            image_id: ImageId(id),
            image_size: [20, 20],
            src: [x * 10.0, y * 10.0, 10.0, 10.0],
            dst: [x, y, 1.0, 1.0],
        }
    }

    #[test]
    fn tiles_of_an_image_side_by_side_and_stacked_are_drawn_as_one() {
        let tiles = vec![
            draw(1, 0.0, 0.0),
            draw(1, 1.0, 0.0),
            draw(1, 0.0, 1.0),
            draw(1, 1.0, 1.0),
        ];
        let one = merged(tiles);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].dst, [0.0, 0.0, 2.0, 2.0]);
        assert_eq!(one[0].src, [0.0, 0.0, 20.0, 20.0]);
    }

    #[test]
    fn the_daemon_is_told_whole_pixels_a_cell() {
        let size = Size {
            cell_width: 8.4,
            cell_height: 18.0,
            ..Size::new(100, 30)
        };
        let dims = size.dims();
        assert_eq!((dims.pixel_w, dims.pixel_h), (800, 540));
    }

    async fn daemon(dir: &std::path::Path) -> Daemon {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = dir.join("d.sock");
        let serving = socket.clone();
        tokio::spawn(async move {
            Daemon::serve(&serving, std::time::Duration::from_secs(1), |_| {}).await
        });
        let d = Daemon::new(socket);
        for _ in 0..100 {
            if d.sessions().await.is_ok() {
                return d;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("the daemon did not answer");
    }

    #[tokio::test]
    async fn a_gated_program_runs_only_once_released_and_draws_from_the_top() {
        let dir = tempfile::Builder::new()
            .prefix("adroit-")
            .tempdir_in("/tmp")
            .unwrap();
        let d = daemon(dir.path()).await;
        let id = d
            .spawn(gated(Spawn {
                program: "/bin/sh".into(),
                args: vec!["-c".into(), "printf started; sleep 5".into()],
                cwd: "/tmp".into(),
                cols: 40,
                rows: 6,
                ..Spawn::default()
            }))
            .await
            .unwrap();
        let pane = Pane::attach(&d, id, Size::new(40, 6), &Palette::default(), || {})
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert!(
            pane.screen().text().iter().all(|l| !l.contains("started")),
            "ran before it was released"
        );
        pane.send(InputMsg::KeyBytes(RELEASE.to_vec()));
        for _ in 0..200 {
            if pane.screen().text()[0] == "started" {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("never drawn at the top: {:?}", pane.screen().text());
    }

    #[test]
    fn tiles_of_different_images_stay_apart() {
        assert_eq!(merged(vec![draw(1, 0.0, 0.0), draw(2, 1.0, 0.0)]).len(), 2);
    }
}
