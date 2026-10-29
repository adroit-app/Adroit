//! Pairing: how a client and a work machine learn each other's keys from a
//! short code the client shows and the operator types on the work machine.

use crate::tls::{Anyone, client_config_speaking, peer_speaking, server_config_speaking};
use crate::{Error, Fingerprint, Identity};
use hmac::{KeyInit, Mac};
use spake2::{Ed25519Group, Password, Spake2};
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::oneshot;
use tokio::time::Instant;

/// The application protocol of a pairing connection, apart from the work
/// protocol's so a key the work machine does not know never reaches its work.
pub const PAIR_ALPN: &[u8] = b"adroit-pair/1";

/// Requests that may wait for the operator at once.
pub(crate) const MAX_WAITING: usize = 4;

/// How long either side waits for a message once the operator has decided.
const STEP: Duration = Duration::from_secs(10);

const MAX_NAME: usize = 64;
const MAX_FRAME: usize = 256;
const MAC_LEN: usize = 32;
const EXPORTER_LABEL: &[u8] = b"EXPORTER-adroit-pair-v1";
const CONTEXT: &[u8] = b"adroit pair v1";

const HELLO: u8 = 1;
const REPLY: u8 = 2;
const CONFIRM: u8 = 3;
const DONE: u8 = 4;
const REFUSED: u8 = 5;
/// Not refused: the work machine is not accepting requests now, or has too
/// many waiting. The client may ask again with the same code, since no
/// exchange ran.
const CLOSED: u8 = 6;
const BUSY: u8 = 7;
/// The request waits where the operator sees it.
const QUEUED: u8 = 8;

/// A one-time code of six decimal digits.
#[derive(Clone, PartialEq, Eq)]
pub struct PairingCode(String);

impl PairingCode {
    pub fn generate() -> Self {
        use ring::rand::SecureRandom;
        // The largest multiple of a million below 2^32, so every code is
        // equally likely.
        const BOUND: u32 = 4_294_000_000;
        let random = ring::rand::SystemRandom::new();
        loop {
            let mut bytes = [0; 4];
            random.fill(&mut bytes).expect("the system's random source");
            let n = u32::from_le_bytes(bytes);
            if n < BOUND {
                return Self(format!("{:06}", n % 1_000_000));
            }
        }
    }
}

impl fmt::Display for PairingCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", &self.0[..3], &self.0[3..])
    }
}

impl fmt::Debug for PairingCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairingCode(..)")
    }
}

impl std::str::FromStr for PairingCode {
    type Err = PairError;

    fn from_str(text: &str) -> Result<Self, PairError> {
        let digits: String = text.chars().filter(|c| !matches!(c, ' ' | '-')).collect();
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(PairError::Code);
        }
        Ok(Self(digits))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PairError {
    #[error("the code must be six digits")]
    Code,
    #[error("the work machine is not accepting pairing requests")]
    Closed,
    #[error("the work machine has too many pairing requests waiting")]
    Busy,
    #[error("the pairing request was refused")]
    Refused,
    #[error("the pairing request is not waiting")]
    Unknown,
    #[error("the client stopped asking before the operator decided")]
    Gone,
    #[error("the codes did not match, or something stood between the machines")]
    Mismatch,
    #[error("the pairing took too long")]
    Timeout,
    #[error("the work machine {0} may have paired; connect to find out")]
    Unconfirmed(Fingerprint),
    #[error(transparent)]
    Net(#[from] Error),
}

impl From<std::io::Error> for PairError {
    fn from(error: std::io::Error) -> Self {
        Self::Net(Error::Io(error))
    }
}

/// A request waiting for the operator, as the operator sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub id: u64,
    pub name: String,
    pub client: Fingerprint,
}

/// Records a client the operator paired.
pub trait Enroll: Send + Sync {
    fn enroll(&self, client: Fingerprint, name: &str) -> std::io::Result<()>;
}

type Outcome = oneshot::Sender<Result<Fingerprint, PairError>>;

enum Decision {
    Try(PairingCode, Outcome),
    Reject,
}

struct Waiting {
    request: Request,
    decide: Option<oneshot::Sender<Decision>>,
}

#[derive(Default)]
struct State {
    open_until: Option<Instant>,
    waiting: Vec<Waiting>,
    next_id: u64,
}

impl State {
    fn is_open(&self) -> bool {
        self.open_until.is_some_and(|until| Instant::now() < until)
    }
}

/// A work machine's pairing requests and whether it accepts new ones.
pub struct Pairing {
    server: Fingerprint,
    enroll: Arc<dyn Enroll>,
    state: Mutex<State>,
}

impl Pairing {
    pub fn new(server: Fingerprint, enroll: Arc<dyn Enroll>) -> Self {
        Self {
            server,
            enroll,
            state: Mutex::default(),
        }
    }

    /// Accepts requests for `period`, until one pairs or `close` is called.
    pub fn open(&self, period: Duration) {
        self.state.lock().unwrap().open_until = Some(Instant::now() + period);
    }

    /// Stops accepting requests and refuses those still waiting.
    pub fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.open_until = None;
        for waiting in state.waiting.iter_mut() {
            if let Some(decide) = waiting.decide.take() {
                let _ = decide.send(Decision::Reject);
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.state.lock().unwrap().is_open()
    }

    pub fn requests(&self) -> Vec<Request> {
        let state = self.state.lock().unwrap();
        state
            .waiting
            .iter()
            .filter(|waiting| waiting.decide.is_some())
            .map(|waiting| waiting.request.clone())
            .collect()
    }

    /// Runs the request with `id` against the code the operator typed and
    /// returns the client it paired.
    pub async fn approve(&self, id: u64, code: PairingCode) -> Result<Fingerprint, PairError> {
        let (outcome, result) = oneshot::channel();
        self.decide(id, Decision::Try(code, outcome))?;
        result.await.unwrap_or(Err(PairError::Refused))
    }

    pub fn reject(&self, id: u64) -> Result<(), PairError> {
        self.decide(id, Decision::Reject)
    }

    fn decide(&self, id: u64, decision: Decision) -> Result<(), PairError> {
        let decide = {
            let mut state = self.state.lock().unwrap();
            state
                .waiting
                .iter_mut()
                .find(|waiting| waiting.request.id == id)
                .and_then(|waiting| waiting.decide.take())
                .ok_or(PairError::Unknown)?
        };
        decide.send(decision).map_err(|_| PairError::Unknown)
    }

    /// Serves one pairing connection whose handshake spoke [`PAIR_ALPN`].
    pub async fn serve<S>(
        &self,
        mut stream: tokio_rustls::server::TlsStream<S>,
    ) -> Result<Fingerprint, PairError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let client = peer_speaking(stream.get_ref().1, PAIR_ALPN)?;
        let exporter = exporter(stream.get_ref().1)?;
        let (name, from_client) = match tokio::time::timeout(STEP, read_hello(&mut stream)).await {
            Ok(hello) => hello?,
            Err(_) => return Err(PairError::Timeout),
        };
        let (id, decided, until) = match self.enqueue(&name, client) {
            Ok(entry) => entry,
            Err(error) => {
                let tag = if matches!(error, PairError::Busy) {
                    BUSY
                } else {
                    CLOSED
                };
                let _ = write_frame(&mut stream, tag, &[]).await;
                let _ = stream.shutdown().await;
                return Err(error);
            }
        };
        let _leave = Leave { pairing: self, id };
        if write_frame(&mut stream, QUEUED, &[]).await.is_err() {
            return Err(PairError::Gone);
        }
        // The client says nothing until the operator decides, so anything
        // read meanwhile, or the end of the stream, means it stopped asking.
        let mut byte = [0; 1];
        let decision = tokio::select! {
            decision = decided => decision.ok(),
            _ = tokio::time::sleep_until(until) => None,
            _ = stream.read(&mut byte) => return Err(PairError::Gone),
        };
        let Some(Decision::Try(code, outcome)) = decision else {
            let _ = refuse(&mut stream).await;
            return Err(PairError::Refused);
        };
        let result = self
            .confirm(&mut stream, &code, client, &name, &exporter, &from_client)
            .await;
        if result.is_ok() {
            self.state.lock().unwrap().open_until = None;
        }
        let _ = outcome.send(match &result {
            Ok(client) => Ok(*client),
            Err(PairError::Mismatch) => Err(PairError::Mismatch),
            Err(PairError::Timeout) => Err(PairError::Timeout),
            Err(_) => Err(PairError::Refused),
        });
        result
    }

    fn enqueue(
        &self,
        name: &str,
        client: Fingerprint,
    ) -> Result<(u64, oneshot::Receiver<Decision>, Instant), PairError> {
        let mut state = self.state.lock().unwrap();
        let Some(until) = state.open_until.filter(|_| state.is_open()) else {
            return Err(PairError::Closed);
        };
        // A client asks again when it lost its first request, or another of
        // its windows asks; the operator sees the client once, as it asks now.
        for waiting in state
            .waiting
            .iter_mut()
            .filter(|waiting| waiting.request.client == client)
        {
            if let Some(decide) = waiting.decide.take() {
                let _ = decide.send(Decision::Reject);
            }
        }
        state
            .waiting
            .retain(|waiting| waiting.request.client != client);
        if state.waiting.len() >= MAX_WAITING {
            return Err(PairError::Busy);
        }
        state.next_id += 1;
        let id = state.next_id;
        let (decide, decided) = oneshot::channel();
        state.waiting.push(Waiting {
            request: Request {
                id,
                name: name.into(),
                client,
            },
            decide: Some(decide),
        });
        Ok((id, decided, until))
    }

    async fn confirm<S>(
        &self,
        stream: &mut tokio_rustls::server::TlsStream<S>,
        code: &PairingCode,
        client: Fingerprint,
        name: &str,
        exporter: &[u8],
        from_client: &[u8],
    ) -> Result<Fingerprint, PairError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let (spake, from_server) = Spake2::<Ed25519Group>::start_b(
            &Password::new(code.0.as_bytes()),
            &spake2::Identity::new(client.as_bytes()),
            &spake2::Identity::new(self.server.as_bytes()),
        );
        let shared = spake.finish(from_client).map_err(|_| PairError::Mismatch)?;
        let keys = Confirmation::new(
            &shared,
            exporter,
            client,
            self.server,
            from_client,
            &from_server,
        );
        let mut reply = from_server.clone();
        reply.extend_from_slice(&keys.server_mac());
        write_frame(stream, REPLY, &reply).await?;
        let mac = match tokio::time::timeout(STEP, read_frame(stream)).await {
            Ok(Ok((CONFIRM, mac))) => mac,
            Ok(Ok(_)) | Ok(Err(_)) => return Err(PairError::Mismatch),
            Err(_) => return Err(PairError::Timeout),
        };
        if !keys.client_mac_matches(&mac) {
            let _ = refuse(stream).await;
            return Err(PairError::Mismatch);
        }
        self.enroll.enroll(client, name)?;
        write_frame(stream, DONE, &[]).await?;
        let _ = stream.shutdown().await;
        Ok(client)
    }
}

/// Takes a request off the list however its connection ends.
struct Leave<'a> {
    pairing: &'a Pairing,
    id: u64,
}

impl Drop for Leave<'_> {
    fn drop(&mut self) {
        let mut state = self.pairing.state.lock().unwrap();
        state
            .waiting
            .retain(|waiting| waiting.request.id != self.id);
    }
}

/// The server config a work machine answers pairing connections with.
pub fn pairing_server_config(identity: &Identity) -> Result<Arc<rustls::ServerConfig>, Error> {
    server_config_speaking(identity, Arc::new(Anyone), PAIR_ALPN)
}

/// Asks the work machine at the other end of `stream` to pair under `name`,
/// telling `queued` once its operator can see the request, and returns its
/// key once both sides proved they hold `code`.
pub async fn pair<S>(
    identity: &Identity,
    stream: S,
    name: &str,
    code: &PairingCode,
    queued: impl FnOnce(),
) -> Result<Fingerprint, PairError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let name: String = name.chars().filter(|c| !c.is_control()).collect();
    let name = truncate_bytes(&name, MAX_NAME);
    let config = client_config_speaking(identity, Arc::new(Anyone), PAIR_ALPN)?;
    let mut stream = tokio_rustls::TlsConnector::from(config)
        .connect(
            rustls::pki_types::ServerName::try_from("adroit.invalid").expect("a valid DNS name"),
            stream,
        )
        .await
        .map_err(Error::from)?;
    let server = peer_speaking(stream.get_ref().1, PAIR_ALPN)?;
    let exporter = exporter(stream.get_ref().1)?;
    let client = identity.fingerprint();
    let (spake, from_client) = Spake2::<Ed25519Group>::start_a(
        &Password::new(code.0.as_bytes()),
        &spake2::Identity::new(client.as_bytes()),
        &spake2::Identity::new(server.as_bytes()),
    );
    let mut hello = vec![name.len() as u8];
    hello.extend_from_slice(name.as_bytes());
    hello.extend_from_slice(&from_client);
    write_frame(&mut stream, HELLO, &hello).await?;
    let mut answer = read_frame(&mut stream).await?;
    if answer.0 == QUEUED {
        queued();
        answer = read_frame(&mut stream).await?;
    }
    let reply = match answer {
        (REPLY, reply) if reply.len() > MAC_LEN => reply,
        (REFUSED, _) => return Err(PairError::Refused),
        (CLOSED, _) => return Err(PairError::Closed),
        (BUSY, _) => return Err(PairError::Busy),
        _ => return Err(PairError::Net(Error::Protocol)),
    };
    let (from_server, mac) = reply.split_at(reply.len() - MAC_LEN);
    let shared = spake.finish(from_server).map_err(|_| PairError::Mismatch)?;
    let keys = Confirmation::new(
        &shared,
        &exporter,
        client,
        server,
        &from_client,
        from_server,
    );
    if !keys.server_mac_matches(mac) {
        return Err(PairError::Mismatch);
    }
    write_frame(&mut stream, CONFIRM, &keys.client_mac()).await?;
    match tokio::time::timeout(STEP, read_frame(&mut stream)).await {
        Ok(Ok((DONE, _))) => Ok(server),
        Ok(Ok((REFUSED, _))) => Err(PairError::Mismatch),
        _ => Err(PairError::Unconfirmed(server)),
    }
}

fn truncate_bytes(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn exporter<D>(connection: &rustls::ConnectionCommon<D>) -> Result<[u8; 32], Error> {
    connection
        .export_keying_material([0; 32], EXPORTER_LABEL, None)
        .map_err(Error::Tls)
}

/// The keys each side proves the shared secret with, bound to this TLS
/// connection and to both keys and messages of the exchange.
struct Confirmation {
    client_key: [u8; 32],
    server_key: [u8; 32],
    transcript: Vec<u8>,
}

impl Confirmation {
    fn new(
        shared: &[u8],
        exporter: &[u8],
        client: Fingerprint,
        server: Fingerprint,
        from_client: &[u8],
        from_server: &[u8],
    ) -> Self {
        let mut okm = [0; 64];
        hkdf::Hkdf::<sha2::Sha256>::new(Some(exporter), shared)
            .expand(b"adroit pair v1 confirmation", &mut okm)
            .expect("64 bytes is a valid HKDF-SHA256 length");
        let mut transcript = CONTEXT.to_vec();
        for part in [
            exporter,
            client.as_bytes(),
            server.as_bytes(),
            from_client,
            from_server,
        ] {
            transcript.extend_from_slice(&(part.len() as u16).to_be_bytes());
            transcript.extend_from_slice(part);
        }
        let (client_key, server_key) = okm.split_at(32);
        Self {
            client_key: client_key.try_into().unwrap(),
            server_key: server_key.try_into().unwrap(),
            transcript,
        }
    }

    fn mac(key: &[u8], role: &[u8], transcript: &[u8]) -> hmac::Hmac<sha2::Sha256> {
        let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(key).expect("any key length");
        mac.update(role);
        mac.update(transcript);
        mac
    }

    fn server_mac(&self) -> Vec<u8> {
        Self::mac(&self.server_key, b"server", &self.transcript)
            .finalize()
            .into_bytes()
            .to_vec()
    }

    fn client_mac(&self) -> Vec<u8> {
        Self::mac(&self.client_key, b"client", &self.transcript)
            .finalize()
            .into_bytes()
            .to_vec()
    }

    fn server_mac_matches(&self, mac: &[u8]) -> bool {
        Self::mac(&self.server_key, b"server", &self.transcript)
            .verify_slice(mac)
            .is_ok()
    }

    fn client_mac_matches(&self, mac: &[u8]) -> bool {
        Self::mac(&self.client_key, b"client", &self.transcript)
            .verify_slice(mac)
            .is_ok()
    }
}

async fn read_hello<S>(stream: &mut S) -> Result<(String, Vec<u8>), PairError>
where
    S: AsyncRead + Unpin,
{
    let (HELLO, body) = read_frame(stream).await? else {
        return Err(PairError::Net(Error::Protocol));
    };
    let (&length, rest) = body.split_first().ok_or(Error::Protocol)?;
    let length = usize::from(length);
    if length > MAX_NAME || rest.len() <= length {
        return Err(PairError::Net(Error::Protocol));
    }
    let name = std::str::from_utf8(&rest[..length]).map_err(|_| Error::Protocol)?;
    if name.chars().any(char::is_control) {
        return Err(PairError::Net(Error::Protocol));
    }
    Ok((name.to_string(), rest[length..].to_vec()))
}

async fn refuse<S>(stream: &mut S) -> Result<(), PairError>
where
    S: AsyncWrite + Unpin,
{
    write_frame(stream, REFUSED, &[]).await?;
    stream.shutdown().await?;
    Ok(())
}

async fn write_frame<S>(stream: &mut S, tag: u8, body: &[u8]) -> Result<(), PairError>
where
    S: AsyncWrite + Unpin,
{
    let length = u16::try_from(body.len() + 1).map_err(|_| Error::Protocol)?;
    let mut frame = length.to_be_bytes().to_vec();
    frame.push(tag);
    frame.extend_from_slice(body);
    stream.write_all(&frame).await?;
    stream.flush().await?;
    Ok(())
}

async fn read_frame<S>(stream: &mut S) -> Result<(u8, Vec<u8>), PairError>
where
    S: AsyncRead + Unpin,
{
    let mut length = [0; 2];
    stream.read_exact(&mut length).await?;
    let length = usize::from(u16::from_be_bytes(length));
    if length == 0 || length > MAX_FRAME {
        return Err(PairError::Net(Error::Protocol));
    }
    let mut frame = vec![0; length];
    stream.read_exact(&mut frame).await?;
    let tag = frame.remove(0);
    Ok((tag, frame))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::io::DuplexStream;

    #[derive(Default)]
    struct Enrolled(Mutex<Vec<(Fingerprint, String)>>);

    impl Enroll for Enrolled {
        fn enroll(&self, client: Fingerprint, name: &str) -> std::io::Result<()> {
            self.0.lock().unwrap().push((client, name.into()));
            Ok(())
        }
    }

    struct Server {
        identity: Identity,
        enrolled: Arc<Enrolled>,
        pairing: Arc<Pairing>,
    }

    fn server() -> Server {
        let identity = Identity::generate().unwrap();
        let enrolled = Arc::new(Enrolled::default());
        let pairing = Arc::new(Pairing::new(identity.fingerprint(), enrolled.clone()));
        Server {
            identity,
            enrolled,
            pairing,
        }
    }

    /// Accepts one pairing connection on `far` and serves it.
    fn serving(
        server: &Server,
        far: DuplexStream,
    ) -> tokio::task::JoinHandle<Result<Fingerprint, PairError>> {
        let config = pairing_server_config(&server.identity).unwrap();
        let pairing = server.pairing.clone();
        tokio::spawn(async move {
            let stream = tokio_rustls::TlsAcceptor::from(config)
                .accept(far)
                .await
                .map_err(Error::from)?;
            pairing.serve(stream).await
        })
    }

    async fn waiting(pairing: &Pairing) -> Request {
        for _ in 0..500 {
            if let Some(request) = pairing.requests().pop() {
                return request;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        panic!("no request arrived");
    }

    #[test]
    fn a_code_is_six_digits_and_reads_back_with_or_without_separators() {
        let code = PairingCode::generate();
        let shown = code.to_string();
        assert_eq!(shown.len(), 7, "{shown}");
        assert_eq!(shown.parse::<PairingCode>().unwrap(), code);
        let digits: String = shown.chars().filter(char::is_ascii_digit).collect();
        assert_eq!(digits.len(), 6);
        assert_eq!(digits.parse::<PairingCode>().unwrap(), code);
        assert_eq!(format!(" {digits} ").parse::<PairingCode>().unwrap(), code);
    }

    #[test]
    fn a_code_refuses_anything_but_six_digits() {
        for text in ["", "12345", "1234567", "12a456", "１２３４５６"] {
            assert!(text.parse::<PairingCode>().is_err(), "{text:?}");
        }
    }

    #[tokio::test]
    async fn the_right_code_pairs_both_sides_once() {
        let server = server();
        server.pairing.open(Duration::from_secs(60));
        let client = Identity::generate().unwrap();
        let code = PairingCode::generate();
        let (near, far) = tokio::io::duplex(64 * 1024);
        let served = serving(&server, far);
        let asking = {
            let (client, code) = (client.clone(), code.clone());
            tokio::spawn(async move { pair(&client, near, "laptop", &code, || {}).await })
        };
        let request = waiting(&server.pairing).await;
        assert_eq!(request.name, "laptop");
        assert_eq!(request.client, client.fingerprint());
        let paired = server.pairing.approve(request.id, code).await.unwrap();
        assert_eq!(paired, client.fingerprint());
        assert_eq!(
            asking.await.unwrap().unwrap(),
            server.identity.fingerprint()
        );
        assert_eq!(served.await.unwrap().unwrap(), client.fingerprint());
        assert_eq!(
            *server.enrolled.0.lock().unwrap(),
            vec![(client.fingerprint(), "laptop".to_string())]
        );
        assert!(!server.pairing.is_open(), "a success closes pairing");
        assert!(server.pairing.requests().is_empty());
    }

    #[tokio::test]
    async fn a_wrong_code_pairs_neither_side() {
        let server = server();
        server.pairing.open(Duration::from_secs(60));
        let client = Identity::generate().unwrap();
        let (near, far) = tokio::io::duplex(64 * 1024);
        let served = serving(&server, far);
        let asking = tokio::spawn(async move {
            pair(&client, near, "laptop", &"111-111".parse().unwrap(), || {}).await
        });
        let request = waiting(&server.pairing).await;
        let typed = "222-222".parse().unwrap();
        assert!(matches!(
            server.pairing.approve(request.id, typed).await,
            Err(PairError::Mismatch)
        ));
        assert!(matches!(asking.await.unwrap(), Err(PairError::Mismatch)));
        assert!(served.await.unwrap().is_err());
        assert!(server.enrolled.0.lock().unwrap().is_empty());
        assert!(server.pairing.requests().is_empty(), "a code is tried once");
    }

    #[tokio::test]
    async fn a_closed_work_machine_refuses_requests() {
        let server = server();
        let client = Identity::generate().unwrap();
        let (near, far) = tokio::io::duplex(64 * 1024);
        let served = serving(&server, far);
        let asked = pair(&client, near, "laptop", &PairingCode::generate(), || {}).await;
        assert!(matches!(asked, Err(PairError::Closed)));
        assert!(matches!(served.await.unwrap(), Err(PairError::Closed)));
    }

    #[tokio::test]
    async fn a_rejected_request_is_refused() {
        let server = server();
        server.pairing.open(Duration::from_secs(60));
        let client = Identity::generate().unwrap();
        let (near, far) = tokio::io::duplex(64 * 1024);
        let served = serving(&server, far);
        let asking = tokio::spawn(async move {
            pair(&client, near, "laptop", &PairingCode::generate(), || {}).await
        });
        let request = waiting(&server.pairing).await;
        server.pairing.reject(request.id).unwrap();
        assert!(matches!(asking.await.unwrap(), Err(PairError::Refused)));
        assert!(served.await.unwrap().is_err());
        assert!(server.pairing.requests().is_empty());
        assert!(server.pairing.is_open(), "a rejection leaves pairing open");
    }

    #[tokio::test]
    async fn the_client_is_told_once_the_operator_can_see_its_request() {
        let server = server();
        server.pairing.open(Duration::from_secs(60));
        let client = Identity::generate().unwrap();
        let (near, far) = tokio::io::duplex(64 * 1024);
        let _served = serving(&server, far);
        let (seen, told) = oneshot::channel();
        let _asking = tokio::spawn(async move {
            pair(&client, near, "laptop", &PairingCode::generate(), || {
                let _ = seen.send(());
            })
            .await
        });
        tokio::time::timeout(Duration::from_secs(5), told)
            .await
            .expect("the client was not told")
            .unwrap();
        assert_eq!(server.pairing.requests().len(), 1);
    }

    #[tokio::test]
    async fn a_client_that_hangs_up_while_waiting_leaves_no_request() {
        let server = server();
        server.pairing.open(Duration::from_secs(60));
        let client = Identity::generate().unwrap();
        let (near, far) = tokio::io::duplex(64 * 1024);
        let served = serving(&server, far);
        let asking = tokio::spawn(async move {
            pair(&client, near, "laptop", &PairingCode::generate(), || {}).await
        });
        waiting(&server.pairing).await;
        asking.abort();
        let _ = asking.await;
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(5), served)
                .await
                .expect("the work machine kept waiting")
                .unwrap(),
            Err(PairError::Gone)
        ));
        assert!(server.pairing.requests().is_empty());
    }

    #[tokio::test]
    async fn a_new_request_with_the_same_key_replaces_the_one_waiting() {
        let server = server();
        server.pairing.open(Duration::from_secs(60));
        let client = Identity::generate().unwrap();
        let (near, far) = tokio::io::duplex(64 * 1024);
        let _first_served = serving(&server, far);
        let first = {
            let client = client.clone();
            tokio::spawn(async move {
                pair(&client, near, "laptop", &PairingCode::generate(), || {}).await
            })
        };
        let replaced = waiting(&server.pairing).await;
        let (near, far) = tokio::io::duplex(64 * 1024);
        let _second_served = serving(&server, far);
        let code = PairingCode::generate();
        let second = {
            let (client, code) = (client.clone(), code.clone());
            tokio::spawn(async move { pair(&client, near, "laptop", &code, || {}).await })
        };
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(5), first)
                .await
                .unwrap()
                .unwrap(),
            Err(PairError::Refused)
        ));
        let request = waiting(&server.pairing).await;
        assert_ne!(request.id, replaced.id);
        assert_eq!(server.pairing.requests(), vec![request.clone()]);
        server.pairing.approve(request.id, code).await.unwrap();
        assert!(second.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn requests_beyond_the_waiting_limit_are_refused() {
        let server = server();
        server.pairing.open(Duration::from_secs(60));
        let mut waiting_clients = Vec::new();
        for _ in 0..MAX_WAITING {
            let (near, far) = tokio::io::duplex(64 * 1024);
            waiting_clients.push(serving(&server, far));
            let client = Identity::generate().unwrap();
            waiting_clients.push(tokio::spawn(async move {
                pair(&client, near, "laptop", &PairingCode::generate(), || {}).await
            }));
        }
        for _ in 0..500 {
            if server.pairing.requests().len() == MAX_WAITING {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        assert_eq!(server.pairing.requests().len(), MAX_WAITING);
        let (near, far) = tokio::io::duplex(64 * 1024);
        let served = serving(&server, far);
        let client = Identity::generate().unwrap();
        let asked = pair(&client, near, "one more", &PairingCode::generate(), || {}).await;
        assert!(matches!(asked, Err(PairError::Busy)));
        assert!(matches!(served.await.unwrap(), Err(PairError::Busy)));
        waiting_clients.iter().for_each(|task| task.abort());
    }

    /// Terminates TLS toward each side with keys of its own and relays what
    /// the client and the work machine say to each other.
    #[tokio::test]
    async fn a_relay_between_the_machines_cannot_pair_even_with_the_right_code() {
        let server = server();
        server.pairing.open(Duration::from_secs(60));
        let client = Identity::generate().unwrap();
        let relay = Identity::generate().unwrap();
        let code = PairingCode::generate();

        let (client_near, relay_far) = tokio::io::duplex(64 * 1024);
        let (relay_near, server_far) = tokio::io::duplex(64 * 1024);
        let served = serving(&server, server_far);
        let relaying = {
            let relay = relay.clone();
            tokio::spawn(async move {
                let toward_client =
                    tokio_rustls::TlsAcceptor::from(pairing_server_config(&relay).unwrap())
                        .accept(relay_far)
                        .await
                        .map_err(Error::from)?;
                let config = crate::tls::client_config_speaking(
                    &relay,
                    Arc::new(crate::tls::Anyone),
                    PAIR_ALPN,
                )?;
                let toward_server = tokio_rustls::TlsConnector::from(config)
                    .connect(
                        rustls::pki_types::ServerName::try_from("adroit.invalid").unwrap(),
                        relay_near,
                    )
                    .await
                    .map_err(Error::from)?;
                let (mut a, mut b) = (toward_client, toward_server);
                let _ = tokio::io::copy_bidirectional(&mut a, &mut b).await;
                Ok::<_, Error>(())
            })
        };
        let asking = {
            let (client, code) = (client.clone(), code.clone());
            tokio::spawn(async move { pair(&client, client_near, "laptop", &code, || {}).await })
        };
        let request = waiting(&server.pairing).await;
        assert_eq!(
            request.client,
            relay.fingerprint(),
            "the operator sees the relay"
        );
        assert!(matches!(
            server.pairing.approve(request.id, code).await,
            Err(PairError::Mismatch)
        ));
        assert!(asking.await.unwrap().is_err());
        assert!(served.await.unwrap().is_err());
        assert!(server.enrolled.0.lock().unwrap().is_empty());
        relaying.abort();
    }

    #[tokio::test]
    async fn the_work_protocol_refuses_a_pairing_client() {
        #[derive(Debug)]
        struct Nobody;
        impl crate::Peers for Nobody {
            fn admits(&self, _: &Fingerprint) -> bool {
                false
            }
        }
        let server = Identity::generate().unwrap();
        let client = Identity::generate().unwrap();
        let (near, far) = tokio::io::duplex(64 * 1024);
        let work = crate::server_config(&server, Arc::new(Nobody)).unwrap();
        let accepting = tokio::spawn(async move { crate::accept(work, far).await });
        let asked = pair(&client, near, "laptop", &PairingCode::generate(), || {}).await;
        assert!(asked.is_err());
        assert!(accepting.await.unwrap().is_err());
    }
}
