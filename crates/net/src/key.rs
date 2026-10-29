use crate::Error;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use rustls::sign::CertifiedKey;
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::Path;
use std::sync::Arc;

/// The SHA-256 of a public key's SubjectPublicKeyInfo: the whole of a
/// peer's identity, whatever address or name reached it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    pub fn of(spki: impl AsRef<[u8]>) -> Self {
        Self(Sha256::digest(spki.as_ref()).into())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({self})")
    }
}

impl serde::Serialize for Fingerprint {
    fn serialize<S: serde::Serializer>(&self, out: S) -> Result<S::Ok, S::Error> {
        out.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Fingerprint {
    fn deserialize<D: serde::Deserializer<'de>>(input: D) -> Result<Self, D::Error> {
        String::deserialize(input)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl std::str::FromStr for Fingerprint {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        if text.len() != 64 || !text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(Error::Fingerprint);
        }
        let mut bytes = [0; 32];
        for (byte, pair) in bytes.iter_mut().zip(text.as_bytes().chunks(2)) {
            let pair = std::str::from_utf8(pair).map_err(|_| Error::Fingerprint)?;
            *byte = u8::from_str_radix(pair, 16).map_err(|_| Error::Fingerprint)?;
        }
        Ok(Self(bytes))
    }
}

/// This machine's Ed25519 key, presented to peers as a raw public key.
#[derive(Clone)]
pub struct Identity {
    key: Arc<CertifiedKey>,
    fingerprint: Fingerprint,
}

impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Identity")
            .field("fingerprint", &self.fingerprint)
            .finish_non_exhaustive()
    }
}

/// Larger than any PKCS#8 Ed25519 key, small enough to refuse a stray file
/// before reading it.
const KEY_FILE_LIMIT: u64 = 1024;

impl Identity {
    pub fn generate() -> Result<Self, Error> {
        Self::from_pkcs8(Self::fresh_pkcs8()?)
    }

    /// The key in `path`, written there first when there is none. The file
    /// must be a regular file this user owns that no one else can read.
    pub fn load_or_create(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        match read_private(path, KEY_FILE_LIMIT)? {
            Some(pkcs8) => Self::from_pkcs8(pkcs8).map_err(|_| Error::File {
                path: path.display().to_string(),
                reason: "not an Ed25519 PKCS#8 key".into(),
            }),
            None => {
                let pkcs8 = Self::fresh_pkcs8()?;
                write_private(path, &pkcs8, false)?;
                Self::from_pkcs8(pkcs8)
            }
        }
    }

    pub fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    pub(crate) fn certified_key(&self) -> Arc<CertifiedKey> {
        self.key.clone()
    }

    fn fresh_pkcs8() -> Result<Vec<u8>, Error> {
        let random = ring::rand::SystemRandom::new();
        ring::signature::Ed25519KeyPair::generate_pkcs8(&random)
            .map(|document| document.as_ref().to_vec())
            .map_err(|_| Error::Tls(rustls::Error::General("key generation failed".into())))
    }

    fn from_pkcs8(pkcs8: Vec<u8>) -> Result<Self, Error> {
        let signing = rustls::crypto::ring::sign::any_eddsa_type(&PrivatePkcs8KeyDer::from(pkcs8))?;
        let spki = signing
            .public_key()
            .ok_or_else(|| rustls::Error::General("the key has no public key".into()))?
            .as_ref()
            .to_vec();
        let fingerprint = Fingerprint::of(&spki);
        let key = CertifiedKey::new(vec![CertificateDer::from(spki)], signing);
        Ok(Self {
            key: Arc::new(key),
            fingerprint,
        })
    }
}

pub(crate) fn read_private(path: &Path, limit: u64) -> Result<Option<Vec<u8>>, Error> {
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let refuse = |reason: &str| Error::File {
        path: path.display().to_string(),
        reason: reason.into(),
    };
    let io = |source| Error::FileIo {
        path: path.display().to_string(),
        source,
    };
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) if error.raw_os_error() == Some(libc::ELOOP) => {
            return Err(refuse("is a symbolic link"));
        }
        Err(error) => return Err(io(error)),
    };
    let metadata = file.metadata().map_err(io)?;
    if !metadata.is_file() {
        return Err(refuse("is not a regular file"));
    }
    // SAFETY: getuid has no preconditions.
    if metadata.uid() != unsafe { libc::getuid() } {
        return Err(refuse("is owned by another user"));
    }
    if metadata.mode() & 0o077 != 0 {
        return Err(refuse("can be read by other users"));
    }
    if metadata.len() > limit {
        return Err(refuse("is too large"));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).map_err(io)?;
    Ok(Some(bytes))
}

/// Writes beside `path` and moves the result into place, so a crash never
/// leaves half a file. An existing file is replaced only when `replace` says so.
pub(crate) fn write_private(path: &Path, bytes: &[u8], replace: bool) -> Result<(), Error> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let io = |source| Error::FileIo {
        path: path.display().to_string(),
        source,
    };
    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(io)?;
    let staged = tempfile::Builder::new()
        .prefix(".key-")
        .permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))
        .tempfile_in(parent)
        .map_err(io)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(staged.path())
        .map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)?;
    if replace {
        staged.persist(path).map_err(|error| io(error.error))?;
    } else {
        std::fs::hard_link(staged.path(), path).map_err(io)?;
    }
    std::fs::File::open(parent)
        .and_then(|dir| dir.sync_all())
        .map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_fingerprint_reads_back_from_its_text() {
        let fingerprint = Identity::generate().unwrap().fingerprint();
        let text = fingerprint.to_string();
        assert_eq!(text.len(), 64);
        assert_eq!(text.parse::<Fingerprint>().unwrap(), fingerprint);
    }

    #[test]
    fn a_fingerprint_refuses_text_that_is_not_lower_case_hex_of_its_length() {
        for text in ["", "00", &"A".repeat(64), &"g".repeat(64), &"0".repeat(66)] {
            assert!(text.parse::<Fingerprint>().is_err(), "{text:?}");
        }
    }

    #[test]
    fn every_generated_key_is_another_identity() {
        assert_ne!(
            Identity::generate().unwrap().fingerprint(),
            Identity::generate().unwrap().fingerprint()
        );
    }

    #[test]
    fn a_key_is_written_once_private_and_read_back_as_the_same_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state/key");
        let created = Identity::load_or_create(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let parent = std::fs::metadata(path.parent().unwrap()).unwrap();
        assert_eq!(parent.permissions().mode() & 0o777, 0o700);
        let loaded = Identity::load_or_create(&path).unwrap();
        assert_eq!(created.fingerprint(), loaded.fingerprint());
        let leftovers = std::fs::read_dir(path.parent().unwrap()).unwrap().count();
        assert_eq!(leftovers, 1, "only the key remains");
    }

    #[test]
    fn a_key_others_can_read_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");
        Identity::load_or_create(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(matches!(
            Identity::load_or_create(&path),
            Err(Error::File { .. })
        ));
    }

    #[test]
    fn a_key_behind_a_symbolic_link_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        Identity::load_or_create(&real).unwrap();
        let link = dir.path().join("key");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(matches!(
            Identity::load_or_create(&link),
            Err(Error::File { .. })
        ));
    }

    #[test]
    fn a_file_that_is_not_a_key_is_refused_rather_than_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");
        std::fs::write(&path, b"not a key").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(
            Identity::load_or_create(&path),
            Err(Error::File { .. })
        ));
        assert_eq!(std::fs::read(&path).unwrap(), b"not a key");
    }
}
