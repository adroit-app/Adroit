//! A window onto one session in the daemon: the screen and images it sends,
//! mirrored as they change, and the input sent to it.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use base64::Engine;
use felis_client_core::renderer_effect::{RendererEffect, renderer_effect};
use felis_client_core::{
    AttachIntent, ImageId, ImageLayout, ImageShadow, OutgoingFrame, ShadowScreen,
};
use felis_grid::image_layout::ImageDraw;
use felis_protocol::MessageKind;
use felis_protocol::messages::{
    ConnToClientMsg, GridMsg, ImageMsg, InputMods, InputMsg, MouseAction, MouseButton, MouseEvent,
    PushMsg, RequestedDims, SessionToDaemonMsg, ThemeChannel,
};
use felis_transport::{Delivery, Incoming};
use tokio::sync::mpsc;

use crate::daemon::{Daemon, Result};
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

type Waker = Arc<dyn Fn() + Send + Sync>;

struct State {
    shadow: ShadowScreen,
    images: ImageShadow,
    layout: ImageLayout,
    overrides: Overrides,
    palette: Palette,
    /// Each image as a `data:` URL, made once for what it shows.
    urls: HashMap<ImageId, (u64, Arc<str>)>,
    exited: bool,
    /// A cell's pixels as the daemon was last told them.
    cell: (f32, f32),
    closed: bool,
}

/// A window onto one session.
pub struct Pane {
    id: u128,
    state: Arc<Mutex<State>>,
    out: mpsc::UnboundedSender<OutgoingFrame>,
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
        let state = Arc::new(Mutex::new(State {
            shadow: ShadowScreen::new(info.dims.rows, info.dims.cols),
            images: ImageShadow::new(),
            layout: ImageLayout::new(),
            overrides: Overrides::default(),
            palette: palette.clone(),
            urls: HashMap::new(),
            exited: info.exited,
            cell: {
                let (w, h) = size.cell_px();
                (w as f32, h as f32)
            },
            closed: false,
        }));
        let felis_client_core::Connection {
            mut reader,
            mut writer,
            mut driver,
            ..
        } = conn;
        let (out, mut queue) = mpsc::unbounded_channel::<OutgoingFrame>();
        tokio::spawn(async move {
            while let Some(frame) = queue.recv().await {
                if writer.send_checked_unflushed(frame.frame()).await.is_err()
                    || writer.flush().await.is_err()
                {
                    break;
                }
            }
        });
        let waker: Waker = Arc::new(waker);
        let reading = state.clone();
        let reader = tokio::spawn(async move {
            while let Ok(Some(frame)) = reader.next_frame().await {
                let Ok(incoming) = driver.classify(&frame) else {
                    break;
                };
                let p = match incoming {
                    Incoming::Payload(p) => p,
                    Incoming::Control(ConnToClientMsg::Error { .. }) => break,
                    _ => continue,
                };
                let mut s = reading.lock().unwrap();
                let ok = match p.kind {
                    MessageKind::Grid => match driver.decode::<GridMsg>(&p) {
                        Ok(Delivery::Deliver(d)) => s.grid(&d.msg),
                        Ok(_) => true,
                        Err(_) => false,
                    },
                    MessageKind::Image => match driver.decode::<ImageMsg>(&p) {
                        Ok(Delivery::Deliver(d)) => s.images.apply(&d.msg).is_ok(),
                        Ok(_) => true,
                        Err(_) => false,
                    },
                    MessageKind::Push => match driver.decode::<PushMsg>(&p) {
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
            state,
            out,
            reader,
        };
        pane.send_session(&theme(palette));
        pane.send(InputMsg::ColorScheme {
            dark: dark(palette),
        });
        pane.resize(size);
        Ok(pane)
    }

    pub fn id(&self) -> u128 {
        self.id
    }

    fn send(&self, msg: InputMsg) {
        if let Ok(frame) = OutgoingFrame::input(&msg) {
            let _ = self.out.send(frame);
        }
    }

    fn send_session(&self, msg: &SessionToDaemonMsg) {
        if let Ok(frame) = OutgoingFrame::ordered(msg) {
            let _ = self.out.send(frame);
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

    /// Send a key, as the program's modes ask; `false` when the key never
    /// reaches a program.
    pub fn key(&self, key: &Key) -> bool {
        match key.event() {
            Some(e) => {
                self.to_bottom();
                self.send(InputMsg::Key(e));
                true
            }
            None => false,
        }
    }

    /// Send text as if it were typed.
    pub fn type_text(&self, text: &str) {
        self.to_bottom();
        self.send(InputMsg::KeyBytes(text.as_bytes().to_vec()));
    }

    /// Paste text, bracketed when the program asks for it.
    pub fn paste(&self, text: &str) {
        self.to_bottom();
        self.send(InputMsg::Paste(text.as_bytes().to_vec()));
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

    #[test]
    fn tiles_of_different_images_stay_apart() {
        assert_eq!(merged(vec![draw(1, 0.0, 0.0), draw(2, 1.0, 0.0)]).len(), 2);
    }
}
