//! The gateway on the work machine and the forwarded sockets on the client
//! that reach it: a paired client sees the hosts' sockets as if they were
//! local.

use crate::pair::{PAIR_ALPN, Pairing, pairing_server_config};
use crate::tls::{ALPN, client_config, peer_speaking, server_config};
use crate::{Error, Fingerprint, Identity, Peers};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};
use tokio::sync::watch;

/// How long a caller has to finish its handshake and name its channel.
const HANDSHAKE: Duration = Duration::from_secs(10);
/// How long a client waits for a work machine to accept its connection.
const REACH: Duration = Duration::from_secs(5);
/// Connections the gateway serves at once, paired or not.
const MAX_CONNECTIONS: usize = 64;
/// The longest line the work protocol sends; the work host's own limit.
const MAX_LINE: usize = 16 * 1024 * 1024;
const MAX_INFO: u64 = 64 * 1024;

/// What a connection asks for once its handshake is done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Channel {
    /// The hosts' identities, as `adroit remote info` prints them.
    Info = b'i',
    Work = b'w',
    Terminal = b't',
}

impl Channel {
    fn from_byte(byte: u8) -> Option<Self> {
        [Self::Info, Self::Work, Self::Terminal]
            .into_iter()
            .find(|channel| *channel as u8 == byte)
    }
}

/// The hosts on this machine the gateway relays to.
pub trait Hosts: Send + Sync {
    /// The hosts' identities and sockets, read afresh for every connection
    /// so a replaced host is reached at its new socket.
    fn current(&self) -> Result<HostsNow, String>;
}

pub struct HostsNow {
    pub info: Vec<u8>,
    pub work: PathBuf,
    pub terminal: PathBuf,
}

pub struct Gateway {
    work: Arc<rustls::ServerConfig>,
    pairing_config: Arc<rustls::ServerConfig>,
    pairing: Arc<Pairing>,
    hosts: Arc<dyn Hosts>,
    live: Mutex<HashMap<Fingerprint, watch::Sender<()>>>,
}

impl Gateway {
    pub fn new(
        identity: &Identity,
        peers: Arc<dyn Peers>,
        pairing: Arc<Pairing>,
        hosts: Arc<dyn Hosts>,
    ) -> Result<Arc<Self>, Error> {
        Ok(Arc::new(Self {
            work: server_config(identity, peers)?,
            pairing_config: pairing_server_config(identity)?,
            pairing,
            hosts,
            live: Mutex::default(),
        }))
    }

    /// Serves every connection `listener` accepts until it fails.
    pub async fn listen(self: &Arc<Self>, listener: TcpListener) -> std::io::Result<()> {
        let slots = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));
        loop {
            let (stream, _) = listener.accept().await?;
            // Over the limit, a caller is closed at once rather than queued.
            let Ok(slot) = slots.clone().try_acquire_owned() else {
                continue;
            };
            let _ = stream.set_nodelay(true);
            let gateway = self.clone();
            tokio::spawn(async move {
                let _ = gateway.serve(stream).await;
                drop(slot);
            });
        }
    }

    /// Serves one connection: pairing or a paired client's channel.
    pub async fn serve<S>(self: &Arc<Self>, stream: S) -> Result<(), Error>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let start = tokio::time::timeout(
            HANDSHAKE,
            tokio_rustls::LazyConfigAcceptor::new(rustls::server::Acceptor::default(), stream),
        )
        .await
        .map_err(|_| Error::Protocol)??;
        let offered = |alpn: &[u8]| {
            start
                .client_hello()
                .alpn()
                .is_some_and(|mut offered| offered.any(|name| name == alpn))
        };
        if offered(PAIR_ALPN) {
            let stream =
                tokio::time::timeout(HANDSHAKE, start.into_stream(self.pairing_config.clone()))
                    .await
                    .map_err(|_| Error::Protocol)??;
            return self
                .pairing
                .serve(stream)
                .await
                .map(drop)
                .map_err(|_| Error::Protocol);
        }
        if !offered(ALPN) {
            return Err(Error::Protocol);
        }
        let (stream, peer, channel) = tokio::time::timeout(HANDSHAKE, async {
            let mut stream = start.into_stream(self.work.clone()).await?;
            let peer = peer_speaking(stream.get_ref().1, ALPN)?;
            let channel = Channel::from_byte(stream.read_u8().await?).ok_or(Error::Protocol)?;
            Ok::<_, Error>((stream, peer, channel))
        })
        .await
        .map_err(|_| Error::Protocol)??;
        let mut revoked = self.watch(peer);
        tokio::select! {
            served = self.relay(stream, channel) => served,
            _ = revoked.changed() => Ok(()),
        }
    }

    /// Ends every connection `peer` has open.
    pub fn disconnect(&self, peer: &Fingerprint) {
        if let Some(live) = self.live.lock().unwrap().remove(peer) {
            let _ = live.send(());
        }
    }

    fn watch(&self, peer: Fingerprint) -> watch::Receiver<()> {
        self.live
            .lock()
            .unwrap()
            .entry(peer)
            .or_insert_with(|| watch::channel(()).0)
            .subscribe()
    }

    async fn relay<S>(&self, mut stream: S, channel: Channel) -> Result<(), Error>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let hosts = self.hosts.current().map_err(|_| Error::Protocol)?;
        match channel {
            Channel::Info => {
                stream.write_all(&hosts.info).await?;
                stream.shutdown().await?;
            }
            Channel::Terminal => {
                let mut host = UnixStream::connect(&hosts.terminal).await?;
                let _ = tokio::io::copy_bidirectional(&mut stream, &mut host).await;
            }
            Channel::Work => {
                let host = UnixStream::connect(&hosts.work).await?;
                let (host_read, host_write) = host.into_split();
                let (client_read, mut client_write) = tokio::io::split(stream);
                tokio::select! {
                    _ = without_stop(client_read, host_write) => {}
                    _ = async {
                        let mut host_read = host_read;
                        tokio::io::copy(&mut host_read, &mut client_write).await
                    } => {}
                }
            }
        }
        Ok(())
    }
}

/// Copies the work protocol's lines from a client to the work host until the
/// client asks the host to stop, which only a local caller may.
async fn without_stop<R, W>(read: R, mut write: W) -> Result<(), Error>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut read = BufReader::new(read);
    let mut line = Vec::new();
    loop {
        line.clear();
        let read_bytes = (&mut read)
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut line)
            .await?;
        if read_bytes == 0 || line.len() > MAX_LINE {
            return Ok(());
        }
        if asks_to_stop(&line) {
            return Ok(());
        }
        write.write_all(&line).await?;
    }
}

/// Whether `line` is one of the work host's own operations other than
/// choosing an environment: today, `stop`.
fn asks_to_stop(line: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(line).is_ok_and(|value| {
        value
            .get("op")
            .is_some_and(|op| op.as_str() != Some("environment"))
    })
}

/// A paired work machine, reached at `address`.
#[derive(Clone, Debug)]
pub struct Remote {
    identity: Identity,
    server: Fingerprint,
    address: String,
}

impl Remote {
    pub fn new(identity: Identity, server: Fingerprint, address: impl Into<String>) -> Self {
        Self {
            identity,
            server,
            address: address.into(),
        }
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub async fn info(&self) -> Result<Vec<u8>, Error> {
        let stream = self.open(Channel::Info).await?;
        let mut info = Vec::new();
        stream.take(MAX_INFO + 1).read_to_end(&mut info).await?;
        if info.len() as u64 > MAX_INFO {
            return Err(Error::Protocol);
        }
        Ok(info)
    }

    /// Listens at `work` and `terminal` and carries every connection made
    /// there to the work machine's host of the same kind.
    pub fn forward(&self, work: &Path, terminal: &Path) -> Result<Forwarding, Error> {
        let mut forwarding = Forwarding {
            tasks: Vec::new(),
            sockets: Vec::new(),
        };
        for (path, channel) in [(work, Channel::Work), (terminal, Channel::Terminal)] {
            match std::fs::remove_file(path) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    return Err(error.into());
                }
                _ => {}
            }
            let listener = UnixListener::bind(path)?;
            forwarding.sockets.push(path.to_path_buf());
            let remote = self.clone();
            forwarding.tasks.push(tokio::spawn(async move {
                while let Ok((mut local, _)) = listener.accept().await {
                    let remote = remote.clone();
                    tokio::spawn(async move {
                        if let Ok(mut far) = remote.open(channel).await {
                            let _ = tokio::io::copy_bidirectional(&mut local, &mut far).await;
                        }
                    });
                }
            }));
        }
        Ok(forwarding)
    }

    async fn open(
        &self,
        channel: Channel,
    ) -> Result<tokio_rustls::client::TlsStream<TcpStream>, Error> {
        let config = client_config(&self.identity, self.server)?;
        let unreachable = || Error::Unreachable(self.address.clone());
        let tcp = tokio::time::timeout(REACH, TcpStream::connect(&self.address))
            .await
            .map_err(|_| unreachable())?
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::HostUnreachable
                | std::io::ErrorKind::NetworkUnreachable
                | std::io::ErrorKind::TimedOut => unreachable(),
                _ => Error::Io(error),
            })?;
        let _ = tcp.set_nodelay(true);
        tokio::time::timeout(HANDSHAKE, async {
            let mut stream = crate::connect(config, tcp).await?;
            stream.write_all(&[channel as u8]).await?;
            stream.flush().await?;
            Ok(stream)
        })
        .await
        .map_err(|_| Error::Protocol)?
    }
}

/// Forwarded sockets, closed and removed when dropped.
pub struct Forwarding {
    tasks: Vec<tokio::task::JoinHandle<()>>,
    sockets: Vec<PathBuf>,
}

impl Drop for Forwarding {
    fn drop(&mut self) {
        self.tasks.iter().for_each(|task| task.abort());
        for socket in &self.sockets {
            let _ = std::fs::remove_file(socket);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Enroll, PairingCode, PeerFile, pair};
    use tokio::io::AsyncBufReadExt;

    /// Two fake hosts: the work host records each line and answers it, the
    /// terminal host echoes bytes.
    struct FakeHosts {
        dir: tempfile::TempDir,
        lines: Arc<Mutex<Vec<String>>>,
        accepted: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl FakeHosts {
        fn start() -> Arc<Self> {
            let dir = tempfile::Builder::new()
                .prefix("an-")
                .tempdir_in("/tmp")
                .unwrap();
            let lines = Arc::new(Mutex::new(Vec::new()));
            let accepted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let work = UnixListener::bind(dir.path().join("w")).unwrap();
            let terminal = UnixListener::bind(dir.path().join("t")).unwrap();
            {
                let (lines, accepted) = (lines.clone(), accepted.clone());
                tokio::spawn(async move {
                    while let Ok((stream, _)) = work.accept().await {
                        accepted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        let lines = lines.clone();
                        tokio::spawn(async move {
                            let (read, mut write) = stream.into_split();
                            let mut read = BufReader::new(read).lines();
                            write.write_all(b"{\"hello\":1}\n").await.unwrap();
                            while let Ok(Some(line)) = read.next_line().await {
                                lines.lock().unwrap().push(line.clone());
                                let answer = format!("{{\"echo\":{line}}}\n");
                                if write.write_all(answer.as_bytes()).await.is_err() {
                                    break;
                                }
                            }
                        });
                    }
                });
            }
            {
                let accepted = accepted.clone();
                tokio::spawn(async move {
                    while let Ok((stream, _)) = terminal.accept().await {
                        accepted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        tokio::spawn(async move {
                            let (mut read, mut write) = stream.into_split();
                            let _ = tokio::io::copy(&mut read, &mut write).await;
                        });
                    }
                });
            }
            Arc::new(Self {
                dir,
                lines,
                accepted,
            })
        }
    }

    impl Hosts for FakeHosts {
        fn current(&self) -> Result<HostsNow, String> {
            Ok(HostsNow {
                info: b"{\"instance\":\"default\"}".to_vec(),
                work: self.dir.path().join("w"),
                terminal: self.dir.path().join("t"),
            })
        }
    }

    struct Setup {
        server: Identity,
        peers: Arc<PeerFile>,
        pairing: Arc<Pairing>,
        gateway: Arc<Gateway>,
        hosts: Arc<FakeHosts>,
        address: String,
        _state: tempfile::TempDir,
    }

    async fn setup() -> Setup {
        let state = tempfile::tempdir().unwrap();
        let server = Identity::generate().unwrap();
        let peers = Arc::new(PeerFile::open(state.path().join("peers.json")).unwrap());
        let pairing = Arc::new(Pairing::new(server.fingerprint(), peers.clone()));
        let hosts = FakeHosts::start();
        let gateway = Gateway::new(&server, peers.clone(), pairing.clone(), hosts.clone()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        {
            let gateway = gateway.clone();
            tokio::spawn(async move { gateway.listen(listener).await });
        }
        Setup {
            server,
            peers,
            pairing,
            gateway,
            hosts,
            address,
            _state: state,
        }
    }

    fn paired(setup: &Setup) -> (Identity, Remote) {
        let client = Identity::generate().unwrap();
        setup.peers.enroll(client.fingerprint(), "laptop").unwrap();
        let remote = Remote::new(client.clone(), setup.server.fingerprint(), &setup.address);
        (client, remote)
    }

    fn sockets() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::Builder::new()
            .prefix("an-")
            .tempdir_in("/tmp")
            .unwrap();
        let (work, terminal) = (dir.path().join("w"), dir.path().join("t"));
        (dir, work, terminal)
    }

    async fn next_line<R: AsyncBufReadExt + Unpin>(read: &mut R) -> Option<String> {
        let mut line = String::new();
        match tokio::time::timeout(Duration::from_secs(5), read.read_line(&mut line)).await {
            Ok(Ok(0)) | Ok(Err(_)) => None,
            Ok(Ok(_)) => Some(line),
            Err(_) => panic!("no answer"),
        }
    }

    #[tokio::test]
    async fn a_paired_client_sees_the_hosts_identities() {
        let setup = setup().await;
        let (_, remote) = paired(&setup);
        assert_eq!(remote.info().await.unwrap(), b"{\"instance\":\"default\"}");
    }

    #[tokio::test]
    async fn a_forwarded_work_socket_reaches_the_work_host_but_never_stops_it() {
        let setup = setup().await;
        let (_, remote) = paired(&setup);
        let (_dir, work, terminal) = sockets();
        let _forwarding = remote.forward(&work, &terminal).unwrap();
        let stream = UnixStream::connect(&work).await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut read = BufReader::new(read);
        assert_eq!(next_line(&mut read).await.unwrap(), "{\"hello\":1}\n");
        write
            .write_all(b"{\"op\":\"environment\",\"id\":\"p\"}\n{\"ask\":1}\n")
            .await
            .unwrap();
        assert!(next_line(&mut read).await.unwrap().contains("environment"));
        assert!(next_line(&mut read).await.unwrap().contains("ask"));
        write
            .write_all(b"{\"op\":\"stop\"}\n{\"ask\":2}\n")
            .await
            .unwrap();
        assert_eq!(next_line(&mut read).await, None, "the relay ends");
        assert_eq!(
            *setup.hosts.lines.lock().unwrap(),
            vec![
                "{\"op\":\"environment\",\"id\":\"p\"}".to_string(),
                "{\"ask\":1}".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn a_forwarded_terminal_socket_carries_bytes_both_ways() {
        let setup = setup().await;
        let (_, remote) = paired(&setup);
        let (_dir, work, terminal) = sockets();
        let _forwarding = remote.forward(&work, &terminal).unwrap();
        let mut stream = UnixStream::connect(&terminal).await.unwrap();
        let bytes: Vec<u8> = (0..=255).cycle().take(200_000).collect();
        let (mut read, mut write) = stream.split();
        let (sent, echoed) = tokio::join!(
            async {
                write.write_all(&bytes).await.unwrap();
            },
            async {
                let mut echoed = vec![0; bytes.len()];
                read.read_exact(&mut echoed).await.unwrap();
                echoed
            }
        );
        let () = sent;
        assert_eq!(echoed, bytes);
    }

    #[tokio::test]
    async fn an_unpaired_client_never_reaches_a_host() {
        let setup = setup().await;
        let stranger = Identity::generate().unwrap();
        let remote = Remote::new(stranger, setup.server.fingerprint(), &setup.address);
        assert!(remote.info().await.is_err());
        let (_dir, work, terminal) = sockets();
        let _forwarding = remote.forward(&work, &terminal).unwrap();
        let stream = UnixStream::connect(&work).await.unwrap();
        let mut read = BufReader::new(stream);
        assert_eq!(next_line(&mut read).await, None);
        assert_eq!(
            setup
                .hosts
                .accepted
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }

    #[tokio::test]
    async fn a_client_refuses_a_gateway_with_another_key() {
        let setup = setup().await;
        let (client, _) = paired(&setup);
        let elsewhere = Identity::generate().unwrap().fingerprint();
        let remote = Remote::new(client, elsewhere, &setup.address);
        assert!(remote.info().await.is_err());
    }

    #[tokio::test]
    async fn an_address_no_one_answers_is_reported_as_unreachable() {
        let client = Identity::generate().unwrap();
        let server = Identity::generate().unwrap().fingerprint();
        // TEST-NET-1 is never routed, so the connection neither succeeds nor
        // is refused.
        let remote = Remote::new(client, server, "192.0.2.1:47130");
        let error = remote.info().await.unwrap_err();
        assert!(
            matches!(&error, Error::Unreachable(address) if address == "192.0.2.1:47130"),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn revoking_a_client_ends_its_connections_and_refuses_new_ones() {
        let setup = setup().await;
        let (client, remote) = paired(&setup);
        let (_dir, work, terminal) = sockets();
        let _forwarding = remote.forward(&work, &terminal).unwrap();
        let stream = UnixStream::connect(&work).await.unwrap();
        let mut read = BufReader::new(stream);
        assert!(next_line(&mut read).await.is_some());
        assert!(setup.peers.revoke(&client.fingerprint()).unwrap());
        setup.gateway.disconnect(&client.fingerprint());
        assert_eq!(next_line(&mut read).await, None);
        assert!(remote.info().await.is_err());
    }

    #[tokio::test]
    async fn a_client_paired_through_the_gateway_reaches_the_hosts() {
        let setup = setup().await;
        setup.pairing.open(Duration::from_secs(60));
        let client = Identity::generate().unwrap();
        let code = PairingCode::generate();
        let asking = {
            let (client, code, address) = (client.clone(), code.clone(), setup.address.clone());
            tokio::spawn(async move {
                let stream = TcpStream::connect(address).await.unwrap();
                pair(&client, stream, "laptop", &code).await
            })
        };
        let request = loop {
            if let Some(request) = setup.pairing.requests().pop() {
                break request;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        };
        setup.pairing.approve(request.id, code).await.unwrap();
        let server = asking.await.unwrap().unwrap();
        assert_eq!(server, setup.server.fingerprint());
        let remote = Remote::new(client, server, &setup.address);
        assert_eq!(remote.info().await.unwrap(), b"{\"instance\":\"default\"}");
    }
}
