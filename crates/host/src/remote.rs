//! Independently supervised hosts, discovered by instance and incarnation.

use adroit_sys::fs::{MetadataExt, OpenOptionsExt};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::cli::{DaemonCommand, HostCommand, RemoteCommand};
use adroit_protocol::{RemoteEndpoint, RemoteInfo, is_generation};

const LIMIT: u64 = 65536;
static OURS: OnceLock<RemoteEndpoint> = OnceLock::new();

/// Whether what is being done was asked by a paired client.
pub fn active() -> bool {
    crate::environment::remote()
}

/// This work host as paired clients reach it, once it published itself.
pub fn ours() -> Option<&'static RemoteEndpoint> {
    OURS.get()
}

/// The terminal host of this instance as paired clients reach it, while it
/// runs; read afresh, since it can be replaced while the work host runs.
pub fn terminal() -> Option<RemoteEndpoint> {
    let terminal = read_endpoint(&root(), "terminal").ok()?;
    live(&terminal).ok()?;
    Some(terminal)
}
/// Where the instance keeps what paired clients find its hosts by; one for
/// every environment it serves.
pub(crate) fn root() -> PathBuf {
    adroit_model::paths::remote_dir(&adroit_model::paths::instance_state())
}

fn invalid(error: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

pub(crate) fn private_dir(path: &Path, create: bool) -> std::io::Result<()> {
    if create {
        use adroit_sys::fs::DirBuilderExt;
        match std::fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => adroit_sys::fs::set_mode(path, 0o700)?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    let m = std::fs::symlink_metadata(path)?;
    if !m.is_dir() || m.uid() != adroit_sys::euid() || m.mode() & 0o777 != 0o700 {
        return Err(invalid(format!(
            "{} is not a private directory of this user",
            path.display()
        )));
    }
    Ok(())
}

fn read_private(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(adroit_sys::fs::O_NOFOLLOW)
        .open(path)?;
    let m = file.metadata()?;
    // SAFETY: geteuid has no preconditions.
    if !m.is_file() || m.uid() != adroit_sys::euid() || m.mode() & 0o077 != 0 || m.len() > LIMIT {
        return Err(invalid(format!(
            "{} is not a private bounded discovery file",
            path.display()
        )));
    }
    let mut bytes = Vec::new();
    (&mut file).take(LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err(invalid("discovery is too large"));
    }
    Ok(bytes)
}

pub(crate) fn random() -> std::io::Result<String> {
    let mut bytes = [0u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn instance(dir: &Path) -> std::io::Result<String> {
    let path = dir.join("instance-id");
    let id = random()?;
    let aside = dir.join(format!("instance-{id}"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&aside)?;
    file.write_all(id.as_bytes())?;
    file.sync_all()?;
    let published = std::fs::hard_link(&aside, &path);
    let _ = std::fs::remove_file(aside);
    match published {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let value = String::from_utf8(read_private(&path)?).map_err(invalid)?;
    if !is_generation(&value) {
        return Err(invalid("invalid remote instance identity"));
    }
    Ok(value)
}

fn read_endpoint(dir: &Path, role: &str) -> std::io::Result<RemoteEndpoint> {
    let endpoint: RemoteEndpoint =
        serde_json::from_slice(&read_private(&dir.join(format!("{role}.json")))?)
            .map_err(invalid)?;
    endpoint.validate().map_err(invalid)?;
    Ok(endpoint)
}

fn live(endpoint: &RemoteEndpoint) -> std::io::Result<()> {
    if adroit_term::listening(&endpoint.socket) != Some(endpoint.pid) {
        return Err(invalid(
            "remote host discovery is stale or the host is not running",
        ));
    }
    Ok(())
}

pub fn info() -> std::io::Result<RemoteInfo> {
    let dir = root();
    private_dir(&dir, false)?;
    let identity = String::from_utf8(read_private(&dir.join("instance-id"))?).map_err(invalid)?;
    let info = RemoteInfo {
        instance: identity,
        work: read_endpoint(&dir, "work")?,
        terminal: read_endpoint(&dir, "terminal")?,
    };
    info.validate().map_err(invalid)?;
    live(&info.work)?;
    live(&info.terminal)?;
    Ok(info)
}

/// What a host published of itself for paired clients to find it by, taken
/// back when the host ends.
pub struct Publication {
    endpoint: RemoteEndpoint,
    record: PathBuf,
    _claim: File,
}

impl Publication {
    /// Where paired clients reach the host.
    pub fn socket(&self) -> &Path {
        &self.endpoint.socket
    }

    pub fn endpoint(&self) -> &RemoteEndpoint {
        &self.endpoint
    }

    /// Publish this process as the instance's host of `role`, reached at
    /// `socket`, or else at a socket of its own a new generation names.
    pub fn create(role: &str, socket: Option<&Path>) -> std::io::Result<Self> {
        let dir = root();
        std::fs::create_dir_all(dir.parent().unwrap())?;
        private_dir(&dir, true)?;
        let claim = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(adroit_sys::fs::O_NOFOLLOW)
            .open(dir.join(format!("{role}.lock")))?;
        claim
            .try_lock()
            .map_err(|e| invalid(format!("remote {role} host is already owned: {e}")))?;
        let identity = instance(&dir)?;
        let generation = random()?;
        // /tmp keeps Unix socket paths within the platform's small limit.
        let sockets = PathBuf::from(format!("/tmp/adroit-r-{}", adroit_sys::euid()));
        private_dir(&sockets, true)?;
        let sockets = sockets.join(&identity);
        private_dir(&sockets, true)?;
        let generation_socket = sockets.join(format!("{generation}.sock"));
        if generation_socket.as_os_str().len() > 100 {
            return Err(invalid("remote generation socket path is too long"));
        }
        // Paired clients know a host by a socket its generation names, so a
        // host listening where its windows reach it is linked there.
        if let Some(socket) = socket {
            #[cfg(unix)]
            std::os::unix::fs::symlink(socket, &generation_socket)?;
            #[cfg(not(unix))]
            return Err(invalid(format!(
                "paired clients cannot reach {} here",
                socket.display()
            )));
        }
        let socket = generation_socket;
        let protocols = if role == "work" {
            adroit_protocol::SPOKEN.iter().map(|v| v.0).collect()
        } else {
            adroit_term::SPEAKS.to_vec()
        };
        let endpoint = RemoteEndpoint {
            instance: identity,
            generation,
            socket,
            pid: std::process::id(),
            build: crate::standing::build(),
            protocols,
        };
        let record = dir.join(format!("{role}.json"));
        let aside = dir.join(format!("{role}.{}.json", endpoint.generation));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&aside)?;
        file.write_all(&serde_json::to_vec(&endpoint).map_err(invalid)?)?;
        file.sync_all()?;
        std::fs::rename(&aside, &record)?;
        Ok(Self {
            endpoint,
            record,
            _claim: claim,
        })
    }

    /// This work host, published, as the one paired clients reach.
    pub fn ours(&self) {
        let _ = OURS.set(self.endpoint.clone());
    }
}

impl Drop for Publication {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.endpoint.socket);
        let _ = std::fs::remove_file(&self.record);
    }
}

pub fn cli(operation: RemoteCommand) -> i32 {
    if matches!(
        operation,
        RemoteCommand::Gateway { .. }
            | RemoteCommand::Pair
            | RemoteCommand::Devices
            | RemoteCommand::Revoke(_)
    ) {
        return crate::gateway::cli(operation);
    }
    if matches!(
        operation,
        RemoteCommand::Enable { .. } | RemoteCommand::Disable
    ) {
        return crate::service::cli(operation);
    }
    let done = (|| -> std::io::Result<i32> {
        if operation == RemoteCommand::Info {
            println!("{}", serde_json::to_string(&info()?).map_err(invalid)?);
            return Ok(0);
        }
        // The instance's own hosts are the ones paired clients reach; each
        // publishes itself as it serves.
        Ok(if operation == RemoteCommand::Host {
            crate::host_cli(HostCommand::Serve)
        } else {
            crate::daemon_cli(DaemonCommand::Serve)
        })
    })();
    match done {
        Ok(code) => code,
        Err(e) => {
            eprintln!("adroit: cannot use remote hosts: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn discovery_refuses_symlinks_shared_files_and_oversized_input() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("record");
        std::fs::write(&file, "private").unwrap();
        adroit_sys::fs::set_mode(&file, 0o600).unwrap();
        assert_eq!(read_private(&file).unwrap(), b"private");
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert!(read_private(&link).is_err());
        adroit_sys::fs::set_mode(&file, 0o644).unwrap();
        assert!(read_private(&file).is_err());
        adroit_sys::fs::set_mode(&file, 0o600).unwrap();
        std::fs::write(&file, vec![b'x'; LIMIT as usize + 1]).unwrap();
        assert!(read_private(&file).is_err());
    }

    #[test]
    #[cfg_attr(windows, ignore = "serves as a work machine, which runs Unix")]
    fn simultaneous_services_publish_one_complete_persistent_instance_identity() {
        let dir = tempfile::tempdir().unwrap();
        let values = std::thread::scope(|scope| {
            (0..16)
                .map(|_| scope.spawn(|| instance(dir.path()).unwrap()))
                .collect::<Vec<_>>()
                .into_iter()
                .map(|t| t.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert!(values.iter().all(|v| v == &values[0] && is_generation(v)));
        assert_eq!(instance(dir.path()).unwrap(), values[0]);
    }
}
