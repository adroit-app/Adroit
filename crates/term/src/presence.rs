//! A window onto one session that shows nothing and only tells it the
//! theme. felis answers a program asking for its colours from the windows
//! attached to its session, and with none attached, with xterm's black on
//! white; a presence kept on a session lets a program asking at any time be
//! told the theme's, whether a window shows the session or not.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use felis_client_core::{AttachIntent, OutgoingFrame};
use felis_protocol::MessageKind;
use felis_protocol::messages::{ConnToClientMsg, InputMsg, PushMsg};
use felis_transport::{Delivery, Incoming};
use tokio::sync::{mpsc, oneshot};

use crate::Palette;
use crate::daemon::{Daemon, Error, Result};
use crate::pane::{dark, theme};

/// What tells a session a palette, with where to say once it has taken it.
type Telling = (Vec<OutgoingFrame>, oneshot::Sender<Result<()>>);

/// Dropped, it lets its connection go, which detaches it.
pub struct Presence {
    id: u128,
    out: mpsc::UnboundedSender<Telling>,
    ended: Arc<AtomicBool>,
}

impl Daemon {
    /// Attach a [`Presence`] to the session `id`, returning once the session
    /// has taken `palette` from it.
    ///
    /// It never sends input or a size, so it never takes the session's size
    /// from a window that shows it; and it asks for no frames, so the daemon
    /// sends it next to nothing.
    pub async fn present(&self, id: u128, palette: &Palette) -> Result<Presence> {
        let mut conn = self.paced_window().await?;
        let info = conn.attach(id, AttachIntent::Deliberate).await?;
        let ended = Arc::new(AtomicBool::new(info.exited));
        let (out, mut queue) = mpsc::unbounded_channel::<Telling>();
        let seen = ended.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    telling = queue.recv() => {
                        let Some((frames, reply)) = telling else { break };
                        let mut told = Ok(());
                        for frame in &frames {
                            if let Err(e) = conn.writer.send_checked_unflushed(frame.frame()).await {
                                told = Err(Error::Protocol(e.to_string()));
                                break;
                            }
                        }
                        if told.is_ok() && let Err(e) = conn.writer.flush().await {
                            told = Err(Error::Protocol(e.to_string()));
                        }
                        if told.is_ok() && let Err(e) = conn.input_fence().await {
                            told = Err(Error::Protocol(e.to_string()));
                        }
                        let failed = told.is_err();
                        let _ = reply.send(told);
                        if failed {
                            break;
                        }
                    }
                    frame = conn.next_frame() => {
                        let Ok(Some(frame)) = frame else { break };
                        match conn.driver.classify(&frame) {
                            Ok(Incoming::Payload(p)) if p.kind == MessageKind::Push => {
                                if let Ok(Delivery::Deliver(d)) = conn.driver.decode::<PushMsg>(&p)
                                    && matches!(d.msg, PushMsg::SessionExited { .. })
                                {
                                    break;
                                }
                            }
                            Ok(Incoming::Control(ConnToClientMsg::Error { .. })) | Err(_) => break,
                            Ok(_) => {}
                        }
                    }
                }
            }
            seen.store(true, Ordering::Relaxed);
        });
        let presence = Presence { id, out, ended };
        presence.set_palette(palette).await?;
        Ok(presence)
    }
}

impl Presence {
    pub fn id(&self) -> u128 {
        self.id
    }

    /// Tell the session `palette` from now on, returning once it has taken
    /// it.
    pub async fn set_palette(&self, palette: &Palette) -> Result<()> {
        let frames = OutgoingFrame::ordered(&theme(palette))
            .into_iter()
            .chain(OutgoingFrame::input(&InputMsg::ColorScheme {
                dark: dark(palette),
            }))
            .collect();
        let (reply, taken) = oneshot::channel();
        let gone = || Error::Protocol("the session no longer takes a theme".into());
        self.out.send((frames, reply)).map_err(|_| gone())?;
        taken.await.map_err(|_| gone())?
    }

    /// Whether the session's program has exited, or the daemon let the
    /// presence go.
    pub fn ended(&self) -> bool {
        self.ended.load(Ordering::Relaxed)
    }
}
