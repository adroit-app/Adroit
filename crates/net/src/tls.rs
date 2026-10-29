use crate::{Error, Fingerprint, Identity};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{WebPkiSupportedAlgorithms, verify_tls13_signature_with_raw_key};
use rustls::pki_types::{CertificateDer, ServerName, SubjectPublicKeyInfoDer, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{
    ClientConfig, DigitallySignedStruct, DistinguishedName, ServerConfig, SignatureScheme,
};
use std::fmt::Debug;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};

/// The one application protocol a connection may speak.
pub const ALPN: &[u8] = b"adroit/1";

/// Who a server lets in, asked again on every handshake so a revoked peer is
/// refused from its next connection on.
pub trait Peers: Debug + Send + Sync {
    fn admits(&self, peer: &Fingerprint) -> bool;
}

pub fn server_config(
    identity: &Identity,
    peers: Arc<dyn Peers>,
) -> Result<Arc<ServerConfig>, Error> {
    server_config_speaking(identity, peers, ALPN)
}

pub fn client_config(identity: &Identity, server: Fingerprint) -> Result<Arc<ClientConfig>, Error> {
    client_config_speaking(identity, Arc::new(Only(server)), ALPN)
}

/// Completes a server's handshake and names the client it proved to be.
pub async fn accept<S>(
    config: Arc<ServerConfig>,
    stream: S,
) -> Result<(tokio_rustls::server::TlsStream<S>, Fingerprint), Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let stream = tokio_rustls::TlsAcceptor::from(config)
        .accept(stream)
        .await?;
    let peer = peer_speaking(stream.get_ref().1, ALPN)?;
    Ok((stream, peer))
}

/// Completes a client's handshake with the server its config pins.
pub async fn connect<S>(
    config: Arc<ClientConfig>,
    stream: S,
) -> Result<tokio_rustls::client::TlsStream<S>, Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let stream = tokio_rustls::TlsConnector::from(config)
        .connect(peer_name(), stream)
        .await?;
    peer_speaking(stream.get_ref().1, ALPN)?;
    Ok(stream)
}

pub(crate) fn server_config_speaking(
    identity: &Identity,
    peers: Arc<dyn Peers>,
    alpn: &[u8],
) -> Result<Arc<ServerConfig>, Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = RawKeyClients {
        peers,
        algorithms: provider.signature_verification_algorithms,
    };
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_client_cert_verifier(Arc::new(verifier))
        .with_cert_resolver(Arc::new(
            rustls::server::AlwaysResolvesServerRawPublicKeys::new(identity.certified_key()),
        ));
    config.alpn_protocols = vec![alpn.to_vec()];
    config.session_storage = Arc::new(rustls::server::NoServerSessionStorage {});
    config.send_tls13_tickets = 0;
    config.max_early_data_size = 0;
    Ok(Arc::new(config))
}

pub(crate) fn client_config_speaking(
    identity: &Identity,
    servers: Arc<dyn Peers>,
    alpn: &[u8],
) -> Result<Arc<ClientConfig>, Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = RawKeyServer {
        peers: servers,
        algorithms: provider.signature_verification_algorithms,
    };
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_client_cert_resolver(Arc::new(
            rustls::client::AlwaysResolvesClientRawPublicKeys::new(identity.certified_key()),
        ));
    config.alpn_protocols = vec![alpn.to_vec()];
    config.resumption = rustls::client::Resumption::disabled();
    config.enable_early_data = false;
    Ok(Arc::new(config))
}

/// The key the other side proved it holds, once it agreed to speak `alpn`.
pub(crate) fn peer_speaking<D>(
    connection: &rustls::ConnectionCommon<D>,
    alpn: &[u8],
) -> Result<Fingerprint, Error> {
    if connection.alpn_protocol() != Some(alpn) {
        return Err(Error::Protocol);
    }
    connection
        .peer_certificates()
        .and_then(|keys| keys.first())
        .map(Fingerprint::of)
        .ok_or(Error::Protocol)
}

#[derive(Debug)]
struct Only(Fingerprint);

impl Peers for Only {
    fn admits(&self, peer: &Fingerprint) -> bool {
        *peer == self.0
    }
}

/// Any key that proves it is held. Only pairing, which learns the key it
/// trusts from the code, may use it.
#[derive(Debug)]
pub(crate) struct Anyone;

impl Peers for Anyone {
    fn admits(&self, _peer: &Fingerprint) -> bool {
        true
    }
}

/// The name TLS asks for. Nothing checks it: the server is whoever holds the
/// pinned key, whatever address or name reached it.
fn peer_name() -> ServerName<'static> {
    ServerName::try_from("adroit.invalid").expect("a valid DNS name")
}

fn bad_key() -> rustls::Error {
    rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure)
}

fn tls13_only() -> rustls::Error {
    rustls::Error::General("only TLS 1.3 is spoken".into())
}

#[derive(Debug)]
struct RawKeyServer {
    peers: Arc<dyn Peers>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for RawKeyServer {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if !intermediates.is_empty() || !self.peers.admits(&Fingerprint::of(end_entity)) {
            return Err(bad_key());
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(tls13_only())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature_with_raw_key(
            message,
            &SubjectPublicKeyInfoDer::from(cert.as_ref()),
            dss,
            &self.algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }

    fn requires_raw_public_keys(&self) -> bool {
        true
    }
}

#[derive(Debug)]
struct RawKeyClients {
    peers: Arc<dyn Peers>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl ClientCertVerifier for RawKeyClients {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        if !intermediates.is_empty() || !self.peers.admits(&Fingerprint::of(end_entity)) {
            return Err(bad_key());
        }
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(tls13_only())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature_with_raw_key(
            message,
            &SubjectPublicKeyInfoDer::from(cert.as_ref()),
            dss,
            &self.algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }

    fn requires_raw_public_keys(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[derive(Debug)]
    struct Admitted(HashSet<Fingerprint>);

    impl Peers for Admitted {
        fn admits(&self, peer: &Fingerprint) -> bool {
            self.0.contains(peer)
        }
    }

    fn admitting(peers: &[&Identity]) -> Arc<dyn Peers> {
        Arc::new(Admitted(
            peers.iter().map(|peer| peer.fingerprint()).collect(),
        ))
    }

    async fn handshake(
        server: Arc<ServerConfig>,
        client: Arc<ClientConfig>,
    ) -> (Result<Fingerprint, Error>, Result<(), Error>) {
        let (near, far) = tokio::io::duplex(64 * 1024);
        let serving = async move {
            let (mut stream, peer) = accept(server, far).await?;
            let mut greeting = [0; 5];
            stream.read_exact(&mut greeting).await?;
            assert_eq!(&greeting, b"hello");
            stream.write_all(b"world").await?;
            stream.flush().await?;
            Ok(peer)
        };
        let calling = async move {
            let mut stream = connect(client, near).await?;
            stream.write_all(b"hello").await?;
            stream.flush().await?;
            let mut answer = [0; 5];
            stream.read_exact(&mut answer).await?;
            assert_eq!(&answer, b"world");
            Ok(())
        };
        tokio::join!(serving, calling)
    }

    #[tokio::test]
    async fn pinned_peers_talk_and_the_server_learns_who_called() {
        let (server, client) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let (served, called) = handshake(
            server_config(&server, admitting(&[&client])).unwrap(),
            client_config(&client, server.fingerprint()).unwrap(),
        )
        .await;
        assert_eq!(served.unwrap(), client.fingerprint());
        called.unwrap();
    }

    #[tokio::test]
    async fn a_client_refuses_a_server_with_another_key() {
        let (server, impostor, client) = (
            Identity::generate().unwrap(),
            Identity::generate().unwrap(),
            Identity::generate().unwrap(),
        );
        let (served, called) = handshake(
            server_config(&impostor, admitting(&[&client])).unwrap(),
            client_config(&client, server.fingerprint()).unwrap(),
        )
        .await;
        assert!(served.is_err());
        assert!(matches!(called, Err(Error::OtherServer)), "{called:?}");
    }

    #[tokio::test]
    async fn a_server_refuses_a_client_it_does_not_admit() {
        let (server, client, stranger) = (
            Identity::generate().unwrap(),
            Identity::generate().unwrap(),
            Identity::generate().unwrap(),
        );
        let (served, called) = handshake(
            server_config(&server, admitting(&[&client])).unwrap(),
            client_config(&stranger, server.fingerprint()).unwrap(),
        )
        .await;
        assert!(served.is_err());
        assert!(matches!(called, Err(Error::NotPaired)), "{called:?}");
    }

    #[tokio::test]
    async fn a_server_refuses_a_client_that_names_no_protocol() {
        let (server, client) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let mut silent = (*client_config(&client, server.fingerprint()).unwrap()).clone();
        silent.alpn_protocols.clear();
        let (served, called) = handshake(
            server_config(&server, admitting(&[&client])).unwrap(),
            Arc::new(silent),
        )
        .await;
        assert!(matches!(served, Err(Error::Protocol)));
        assert!(called.is_err());
    }

    #[tokio::test]
    async fn a_client_refuses_a_server_that_names_no_protocol() {
        let (server, client) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let mut silent = (*server_config(&server, admitting(&[&client])).unwrap()).clone();
        silent.alpn_protocols.clear();
        let (_, called) = handshake(
            Arc::new(silent),
            client_config(&client, server.fingerprint()).unwrap(),
        )
        .await;
        assert!(matches!(called, Err(Error::Protocol)));
    }

    #[test]
    fn neither_side_resumes_a_session_or_sends_early_data() {
        let (server, client) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let server = server_config(&server, admitting(&[&client])).unwrap();
        assert_eq!(server.send_tls13_tickets, 0);
        assert_eq!(server.max_early_data_size, 0);
        assert!(!server.session_storage.can_cache());
        let client = client_config(&client, Identity::generate().unwrap().fingerprint()).unwrap();
        assert!(!client.enable_early_data);
    }
}
