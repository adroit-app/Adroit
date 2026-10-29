use crate::key::{read_private, write_private};
use crate::{Enroll, Error, Fingerprint, Peers};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const PEER_FILE_LIMIT: u64 = 1024 * 1024;

/// A client the operator paired with this work machine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paired {
    pub fingerprint: Fingerprint,
    pub name: String,
    /// Seconds since the Unix epoch.
    pub paired_at: u64,
}

/// The paired clients, kept in a private file and read again for every
/// question so a revocation made by another process applies at once.
#[derive(Debug)]
pub struct PeerFile {
    path: PathBuf,
}

impl PeerFile {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let file = Self {
            path: path.as_ref().to_path_buf(),
        };
        file.read()?;
        Ok(file)
    }

    pub fn list(&self) -> Vec<Paired> {
        self.read().unwrap_or_default()
    }

    /// Forgets `fingerprint`, and says whether it was paired.
    pub fn revoke(&self, fingerprint: &Fingerprint) -> Result<bool, Error> {
        self.change(|peers| {
            let before = peers.len();
            peers.retain(|peer| peer.fingerprint != *fingerprint);
            peers.len() != before
        })
    }

    fn read(&self) -> Result<Vec<Paired>, Error> {
        let Some(bytes) = read_private(&self.path, PEER_FILE_LIMIT)? else {
            return Ok(Vec::new());
        };
        serde_json::from_slice(&bytes).map_err(|error| Error::File {
            path: self.path.display().to_string(),
            reason: error.to_string(),
        })
    }

    /// Reads, changes and writes the file under a lock, so a pairing and a
    /// revocation in different processes do not undo each other.
    fn change<T>(&self, edit: impl FnOnce(&mut Vec<Paired>) -> T) -> Result<T, Error> {
        let _lock = Lock::take(&self.path)?;
        let mut peers = self.read()?;
        let result = edit(&mut peers);
        let bytes = serde_json::to_vec_pretty(&peers).expect("peers serialize");
        write_private(&self.path, &bytes, true)?;
        Ok(result)
    }
}

impl Peers for PeerFile {
    fn admits(&self, peer: &Fingerprint) -> bool {
        self.read()
            .is_ok_and(|peers| peers.iter().any(|paired| paired.fingerprint == *peer))
    }
}

impl Enroll for PeerFile {
    fn enroll(&self, client: Fingerprint, name: &str) -> std::io::Result<()> {
        let paired_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        self.change(|peers| {
            peers.retain(|peer| peer.fingerprint != client);
            peers.push(Paired {
                fingerprint: client,
                name: name.into(),
                paired_at,
            });
        })
        .map_err(std::io::Error::other)
    }
}

/// An exclusive lock on a file beside the peers, held until dropped.
struct Lock {
    _file: std::fs::File,
}

impl Lock {
    fn take(path: &Path) -> Result<Self, Error> {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        let io = |source| Error::FileIo {
            path: path.display().to_string(),
            source,
        };
        if let Some(parent) = path.parent() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)
                .map_err(io)?;
        }
        let mut name = path.as_os_str().to_owned();
        name.push(".lock");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&name)
            .map_err(io)?;
        // SAFETY: the descriptor stays open for as long as the lock is held.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(io(std::io::Error::last_os_error()));
        }
        Ok(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;

    fn someone() -> Fingerprint {
        Identity::generate().unwrap().fingerprint()
    }

    #[test]
    fn a_missing_file_admits_no_one() {
        let dir = tempfile::tempdir().unwrap();
        let peers = PeerFile::open(dir.path().join("peers.json")).unwrap();
        assert!(peers.list().is_empty());
        assert!(!peers.admits(&someone()));
    }

    #[test]
    fn an_enrolled_client_is_admitted_and_kept_across_opens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("peers.json");
        let client = someone();
        PeerFile::open(&path)
            .unwrap()
            .enroll(client, "laptop")
            .unwrap();
        let peers = PeerFile::open(&path).unwrap();
        assert!(peers.admits(&client));
        assert!(!peers.admits(&someone()));
        let listed = peers.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].fingerprint, client);
        assert_eq!(listed[0].name, "laptop");
    }

    #[test]
    fn enrolling_a_client_again_keeps_one_entry_with_its_new_name() {
        let dir = tempfile::tempdir().unwrap();
        let peers = PeerFile::open(dir.path().join("peers.json")).unwrap();
        let client = someone();
        peers.enroll(client, "laptop").unwrap();
        peers.enroll(client, "desk").unwrap();
        let listed = peers.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "desk");
    }

    #[test]
    fn a_revoked_client_is_refused_by_every_open_of_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("peers.json");
        let (client, other) = (someone(), someone());
        let serving = PeerFile::open(&path).unwrap();
        serving.enroll(client, "laptop").unwrap();
        serving.enroll(other, "desk").unwrap();
        let managing = PeerFile::open(&path).unwrap();
        assert!(managing.revoke(&client).unwrap());
        assert!(!managing.revoke(&client).unwrap());
        assert!(
            !serving.admits(&client),
            "the serving process reads the change"
        );
        assert!(serving.admits(&other));
    }

    #[test]
    fn a_damaged_file_admits_no_one_rather_than_everyone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("peers.json");
        let client = someone();
        let peers = PeerFile::open(&path).unwrap();
        peers.enroll(client, "laptop").unwrap();
        std::fs::write(&path, b"{not json").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(!peers.admits(&client));
        assert!(PeerFile::open(&path).is_err());
    }
}
