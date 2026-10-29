//! The asking side of the work host's socket, as the window and the commands
//! reach it: what the host says first, a connection to send requests on and
//! read events from, and asking the host to stop.

use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

use crate::{Event, Hello, Request, decode, encode};

/// How long a work host is given to say hello.
const HELLO: Duration = Duration::from_secs(2);
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

/// A bounded frame reader whose partial input survives cancellation.
pub struct Frames<R> {
    reader: R,
    bytes: Vec<u8>,
}

impl<R: AsyncBufRead + Unpin> Frames<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            bytes: Vec::new(),
        }
    }
    pub async fn next(&mut self) -> std::io::Result<Option<String>> {
        read_frame(&mut self.reader, &mut self.bytes).await
    }
}

pub async fn read_line(
    reader: &mut (impl AsyncBufRead + Unpin),
) -> std::io::Result<Option<String>> {
    let mut bytes = Vec::new();
    read_frame(reader, &mut bytes).await
}

async fn read_frame(
    reader: &mut (impl AsyncBufRead + Unpin),
    bytes: &mut Vec<u8>,
) -> std::io::Result<Option<String>> {
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(std::io::Error::other("truncated work protocol frame"))
            };
        }
        let n = available
            .iter()
            .position(|b| *b == b'\n')
            .map_or(available.len(), |p| p + 1);
        let ended = available[n - 1] == b'\n';
        if bytes.len() + n > MAX_FRAME {
            return Err(std::io::Error::other(
                "work protocol frame exceeds its limit",
            ));
        }
        bytes.extend_from_slice(&available[..n]);
        reader.consume(n);
        if ended {
            return String::from_utf8(std::mem::take(bytes))
                .map(Some)
                .map_err(|e| std::io::Error::other(e.to_string()));
        }
    }
}

/// What the work host at `socket` says: `None` where none listens, and
/// `Some(None)` where one listens but says nothing it can be judged by.
pub async fn hello(socket: &Path) -> Option<Option<Hello>> {
    let stream = UnixStream::connect(socket).await.ok()?;
    let said = tokio::time::timeout(HELLO, async {
        let line = read_line(&mut BufReader::new(stream)).await.ok()??;
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
    let mut lines = BufReader::new(read);
    // Its hello, then its word that it stops, or its hanging up.
    while read_line(&mut lines)
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
        let environment = std::env::var("ADROIT_ENVIRONMENT").ok();
        Self::open_in(socket, environment.as_deref()).await
    }

    /// Discover a service without selecting or creating a working environment.
    pub async fn open_unscoped(socket: &Path) -> std::io::Result<Self> {
        Self::open_in(socket, None).await
    }

    pub async fn open_in(socket: &Path, environment: Option<&str>) -> std::io::Result<Self> {
        let stream = UnixStream::connect(socket).await?;
        let (read, mut write) = stream.into_split();
        let mut lines = Frames::new(BufReader::new(read));
        let line = tokio::time::timeout(HELLO, lines.next())
            .await
            .map_err(|_| std::io::Error::other("the work host said no hello"))??
            .ok_or_else(|| std::io::Error::other("the work host hung up"))?;
        let mut hello: Hello = decode(line).map_err(std::io::Error::other)?;
        if let Some(id) = environment {
            if !adroit_model::paths::is_instance(id) {
                return Err(std::io::Error::other("Invalid work environment"));
            }
            write
                .write_all(
                    format!("{}\n", serde_json::json!({"op": "environment", "id": id})).as_bytes(),
                )
                .await?;
            let line = tokio::time::timeout(HELLO, lines.next())
                .await
                .map_err(|_| {
                    std::io::Error::other("work host does not support environment profiles")
                })??
                .ok_or_else(|| std::io::Error::other("work host refused the environment"))?;
            let reply: serde_json::Value =
                serde_json::from_str(&line).map_err(std::io::Error::other)?;
            if reply.get("environment").and_then(|v| v.as_str()) != Some(id) {
                return Err(std::io::Error::other("work host refused the environment"));
            }
            hello.mutation_epoch = Some(
                serde_json::from_value(reply["epoch"].clone()).map_err(std::io::Error::other)?,
            );
        }
        Ok(Self {
            hello,
            events: Events { lines },
            requests: Requests { write },
        })
    }
}

/// What a work host tells on a [`Connection`].
pub struct Events {
    lines: Frames<BufReader<OwnedReadHalf>>,
}

impl Events {
    /// The next event the host tells; none once it hangs up.
    pub async fn next(&mut self) -> std::io::Result<Option<Event>> {
        while let Some(line) = self.lines.next().await? {
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
        let bytes = encode(request);
        if bytes.len() > MAX_FRAME {
            return Err(std::io::Error::other("work request exceeds its limit"));
        }
        tokio::time::timeout(
            Duration::from_secs(5),
            self.write.write_all(bytes.as_bytes()),
        )
        .await
        .map_err(|_| std::io::Error::other("writing the work request timed out"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelling_a_partial_read_keeps_its_bytes_for_the_next_poll() {
        let (mut sent, read) = tokio::io::duplex(64);
        let mut frames = Frames::new(BufReader::with_capacity(2, read));
        sent.write_all("日".as_bytes()).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), frames.next())
                .await
                .is_err()
        );
        sent.write_all("本語\nnext\n".as_bytes()).await.unwrap();
        assert_eq!(frames.next().await.unwrap().as_deref(), Some("日本語\n"));
        assert_eq!(frames.next().await.unwrap().as_deref(), Some("next\n"));
    }

    #[tokio::test]
    async fn frames_preserve_utf8_and_reject_partial_or_oversized_input() {
        let mut input = BufReader::with_capacity(2, "日本語\nnext\n".as_bytes());
        assert_eq!(
            read_line(&mut input).await.unwrap(),
            Some("日本語\n".into())
        );
        assert_eq!(read_line(&mut input).await.unwrap(), Some("next\n".into()));
        assert_eq!(read_line(&mut input).await.unwrap(), None);
        assert!(
            read_line(&mut BufReader::new(&b"unfinished"[..]))
                .await
                .is_err()
        );
        assert!(
            read_line(&mut BufReader::new(&b"\xff\n"[..]))
                .await
                .is_err()
        );
        let oversized = vec![b'x'; MAX_FRAME + 1];
        assert!(
            read_line(&mut BufReader::new(oversized.as_slice()))
                .await
                .is_err()
        );
    }
}
