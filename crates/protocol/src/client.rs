//! The asking side of the work host's socket, as the window and the commands
//! reach it: what the host says first, a connection to send requests on and
//! read events from, and asking the host to stop.

use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

use crate::{Event, Hello, Request, decode, encode};

/// How long a work host is given to say hello.
const HELLO: Duration = Duration::from_secs(2);

/// What the work host at `socket` says: `None` where none listens, and
/// `Some(None)` where one listens but says nothing it can be judged by.
pub async fn hello(socket: &Path) -> Option<Option<Hello>> {
    let stream = UnixStream::connect(socket).await.ok()?;
    let said = tokio::time::timeout(HELLO, async {
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).await.ok()?;
        decode::<Hello>(&line).ok()
    })
    .await;
    Some(said.ok().flatten())
}

/// Ask the work host at `socket` to stop, returning once it has taken the
/// request.
pub async fn ask_stop(socket: &Path) -> std::io::Result<()> {
    let stream = UnixStream::connect(socket).await?;
    let (read, mut write) = stream.into_split();
    write.write_all(b"{\"op\":\"stop\"}\n").await?;
    let mut lines = BufReader::new(read).lines();
    // Its hello, then its word that it stops, or its hanging up.
    while lines
        .next_line()
        .await?
        .is_some_and(|line| !line.contains("stopping"))
    {}
    Ok(())
}

/// A connection to a work host, its hello read: the events it tells, and
/// the requests sent to it, to be used apart.
pub struct Connection {
    pub hello: Hello,
    pub events: Events,
    pub requests: Requests,
}

impl Connection {
    /// Connect to the work host at `socket` and read its hello.
    pub async fn open(socket: &Path) -> std::io::Result<Self> {
        let stream = UnixStream::connect(socket).await?;
        let (read, write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        let line = tokio::time::timeout(HELLO, lines.next_line())
            .await
            .map_err(|_| std::io::Error::other("the work host said no hello"))??
            .ok_or_else(|| std::io::Error::other("the work host hung up"))?;
        let hello = decode(line).map_err(std::io::Error::other)?;
        Ok(Self {
            hello,
            events: Events { lines },
            requests: Requests { write },
        })
    }
}

/// What a work host tells on a [`Connection`].
pub struct Events {
    lines: Lines<BufReader<OwnedReadHalf>>,
}

impl Events {
    /// The next event the host tells; none once it hangs up.
    pub async fn next(&mut self) -> std::io::Result<Option<Event>> {
        while let Some(line) = self.lines.next_line().await? {
            if let Ok(event) = decode(&line) {
                return Ok(Some(event));
            }
        }
        Ok(None)
    }
}

/// What is asked of a work host on a [`Connection`].
pub struct Requests {
    write: OwnedWriteHalf,
}

impl Requests {
    pub async fn send(&mut self, request: &Request) -> std::io::Result<()> {
        self.write.write_all(encode(request).as_bytes()).await
    }
}
