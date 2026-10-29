//! Reach a work machine over a network through adroit's own connection.

mod admin;
mod discovery;
mod gateway;
mod key;
mod pair;
mod peers;
mod tls;

pub use admin::*;
pub use discovery::*;
pub use gateway::*;
pub use key::*;
pub use pair::*;
pub use peers::*;
pub use tls::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {reason}")]
    File { path: String, reason: String },
    #[error("{path}: {source}")]
    FileIo {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid fingerprint")]
    Fingerprint,
    #[error("TLS: {0}")]
    Tls(#[from] rustls::Error),
    #[error("connection: {0}")]
    Io(std::io::Error),
    #[error("the peer does not speak adroit's protocol")]
    Protocol,
    #[error("the work machine refused this client's key")]
    NotPaired,
    #[error("the work machine's key is not the paired one")]
    OtherServer,
    #[error("nothing answered at {0}")]
    Unreachable(String),
    #[error("the gateway: {0}")]
    Gateway(String),
}

impl From<std::io::Error> for Error {
    /// TLS reports a refused key through the stream, as an I/O error that
    /// carries the TLS one; that refusal is named apart from a lost network.
    fn from(error: std::io::Error) -> Self {
        use rustls::AlertDescription as Alert;
        match error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        {
            Some(rustls::Error::InvalidCertificate(_)) => Self::OtherServer,
            Some(rustls::Error::AlertReceived(
                Alert::BadCertificate
                | Alert::CertificateUnknown
                | Alert::UnknownCA
                | Alert::AccessDenied
                | Alert::CertificateRequired,
            )) => Self::NotPaired,
            _ => Self::Io(error),
        }
    }
}
