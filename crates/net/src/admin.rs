//! What the operator of a work machine asks its running gateway, over a
//! socket only the user it runs as can reach.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::{Error, Fingerprint, PeerFile};

/// How long the operator keeps pairing open at a time.
pub const PAIRING_PERIOD: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// What the operator asks the running gateway.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Admin {
    Open { seconds: u64 },
    Close,
    Requests,
    Approve { id: u64, code: String },
    Reject { id: u64 },
    Revoke { fingerprint: Fingerprint },
    Status,
}

/// What the gateway answers its operator.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminAnswer {
    Done,
    Requests(Vec<Waiting>),
    Paired(Fingerprint),
    Status {
        server: Fingerprint,
        listening: Vec<String>,
        open: bool,
        /// Whether it announces itself on the local network now.
        #[serde(default)]
        announcing: bool,
    },
    Failed(String),
}

/// A client waiting for the operator to type the code it shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiting {
    pub id: u64,
    pub name: String,
    pub client: Fingerprint,
    /// Whether the client is paired already, so pairing again renews it.
    #[serde(default)]
    pub paired: bool,
}

/// Where a gateway keeps its key, its paired clients and its admin socket.
#[derive(Debug, Clone)]
pub struct GatewayFiles {
    pub key: PathBuf,
    pub peers: PathBuf,
    pub admin: PathBuf,
}

impl GatewayFiles {
    pub fn in_dir(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        Self {
            key: dir.join("gateway.key"),
            peers: dir.join("peers.json"),
            admin: dir.join("gateway.sock"),
        }
    }
}

/// Asks the gateway whose admin socket is `socket`.
pub async fn ask_gateway(socket: impl AsRef<Path>, admin: &Admin) -> std::io::Result<AdminAnswer> {
    let stream = UnixStream::connect(socket).await?;
    let (read, mut write) = stream.into_split();
    let mut line = serde_json::to_vec(admin).map_err(std::io::Error::other)?;
    line.push(b'\n');
    write.write_all(&line).await?;
    let mut answer = String::new();
    BufReader::new(read)
        .take(1024 * 1024)
        .read_line(&mut answer)
        .await?;
    serde_json::from_str(&answer).map_err(std::io::Error::other)
}

/// Forgets the paired client `fingerprint`: through the running gateway,
/// which also ends its connections, or in the file when none runs.
pub async fn revoke_paired(files: &GatewayFiles, fingerprint: Fingerprint) -> Result<(), Error> {
    match ask_gateway(&files.admin, &Admin::Revoke { fingerprint }).await {
        Ok(AdminAnswer::Done) => Ok(()),
        Ok(AdminAnswer::Failed(text)) => Err(Error::Gateway(text)),
        Ok(_) => Err(Error::Gateway("unexpected answer".into())),
        // With no gateway running there is no connection to end.
        Err(_) => PeerFile::open(&files.peers)?.revoke(&fingerprint).map(drop),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Enroll, Identity};

    #[tokio::test]
    async fn a_client_is_revoked_in_the_file_when_no_gateway_runs() {
        let dir = tempfile::tempdir().unwrap();
        let files = GatewayFiles::in_dir(dir.path());
        let client = Identity::generate().unwrap().fingerprint();
        PeerFile::open(&files.peers)
            .unwrap()
            .enroll(client, "laptop")
            .unwrap();
        revoke_paired(&files, client).await.unwrap();
        assert!(PeerFile::open(&files.peers).unwrap().list().is_empty());
    }
}
