//! Select a work machine without moving its work into the window.

pub mod journal;
pub mod profiles;
mod remote;
pub mod work;
pub use remote::*;

use serde::{Deserialize, Serialize};
use std::path::Path;

pub fn read_config(path: &Path, limit: u64) -> std::io::Result<Option<String>> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return match std::fs::symlink_metadata(path) {
                Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => Ok(None),
                _ => Err(error),
            };
        }
        Err(error) => return Err(error),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(std::io::Error::other(
            "configuration must be a bounded regular file",
        ));
    }
    let mut text = String::new();
    file.take(limit + 1).read_to_string(&mut text)?;
    if text.len() as u64 > limit {
        return Err(std::io::Error::other("configuration is too large"));
    }
    Ok(Some(text))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "backend", rename_all = "snake_case", deny_unknown_fields)]
pub enum Target {
    #[default]
    Local,
    /// A work machine paired through its gateway: reached at `address`,
    /// known by the fingerprint of its key.
    Native { address: String, server: String },
}

/// Whether `address` is a host or IP address with a port.
pub fn valid_address(address: &str) -> bool {
    if address.parse::<std::net::SocketAddr>().is_ok() {
        return true;
    }
    address
        .rsplit_once(':')
        .is_some_and(|(host, port)| plain_host(host) && port.parse::<u16>().is_ok_and(|p| p > 0))
}

pub fn diagnose(path: &Path) -> Result<String, String> {
    let target = Target::load(path)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(check(&target))
}

/// Check a proposed target without changing the saved or running connection.
pub async fn check(target: &Target) -> Result<String, String> {
    if *target == Target::Local {
        return Ok("Local execution selected".into());
    }
    let tunnel = Remote::new(target)?.connect(None).await?;
    Ok(connected(
        tunnel.reached(),
        &tunnel.info().work.generation,
        &tunnel.info().terminal.generation,
    ))
}

/// A gateway serves the one instance it was started for, whose name the
/// client is never told, so a connection is named by where it answered.
fn connected(reached: &str, work: &str, terminal: &str) -> String {
    format!("connected to {reached} through its gateway (work {work}, terminal {terminal})")
}

/// A client's choice belongs to its instance, independently of the work instance.
pub fn execution_path(state: &Path, instance: &str) -> Result<std::path::PathBuf, String> {
    if !adroit_model::paths::is_instance(instance) {
        return Err("Invalid client instance name".into());
    }
    Ok(adroit_model::paths::instance_dir(state, instance).join("execution.toml"))
}

fn next_path(active: &Path) -> std::path::PathBuf {
    active.with_extension("next.toml")
}

/// The saved next choice, or the active choice when no change is waiting.
pub fn saved_choice(active: &Path) -> Result<Target, String> {
    let next = next_path(active);
    if next.try_exists().map_err(|e| e.to_string())? {
        Target::load(&next)
    } else {
        Target::load(active)
    }
}

/// Called under the instance's window-start lock, only before its first window.
pub fn activate_saved(active: &Path) -> Result<(), String> {
    let next = next_path(active);
    if !next.try_exists().map_err(|e| e.to_string())? {
        return Ok(());
    }
    Target::load(&next)?;
    std::fs::rename(next, active).map_err(|e| e.to_string())
}

fn plain_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}

impl Target {
    /// Stage a change without changing what running sessions' native clients read.
    pub fn save_next(&self, active: &Path) -> Result<(), String> {
        self.save(&next_path(active))
    }
    /// Replace only this instance's configuration, leaving a rejected save intact.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        use std::io::Write;
        self.validate()?;
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        if text.len() > 65536 {
            return Err("Execution configuration exceeds its size limit".into());
        }
        let parent = path
            .parent()
            .ok_or("Execution configuration has no parent folder")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        file.persist(path).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        if let Self::Native { address, server } = self
            && (!valid_address(address) || server.parse::<adroit_net::Fingerprint>().is_err())
        {
            return Err("invalid work machine address or key fingerprint".into());
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = match read_config(path, 65536) {
            Ok(Some(text)) => text,
            Ok(None) => return Ok(Self::Local),
            Err(e) => return Err(format!("cannot read execution configuration: {e}")),
        };
        let target: Self =
            toml::from_str(&text).map_err(|e| format!("invalid execution configuration: {e}"))?;
        target.validate()?;
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connection_is_named_by_where_the_gateway_answered() {
        assert_eq!(
            connected("192.168.0.46:47130", "w1", "t1"),
            "connected to 192.168.0.46:47130 through its gateway (work w1, terminal t1)"
        );
    }

    #[test]
    fn configuration_reads_reject_special_files_and_are_bounded_before_parsing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_config(dir.path(), 8).is_err());
        let file = dir.path().join("config");
        std::fs::write(&file, "123456789").unwrap();
        assert!(read_config(&file, 8).is_err());
        std::fs::write(&file, "backend = 'local'").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert_eq!(
            read_config(&link, 64).unwrap().as_deref(),
            Some("backend = 'local'")
        );
        assert_eq!(read_config(&dir.path().join("missing"), 64).unwrap(), None);
        let broken = dir.path().join("broken");
        std::os::unix::fs::symlink(dir.path().join("missing"), &broken).unwrap();
        assert!(read_config(&broken, 64).is_err());
        assert!(read_config(std::path::Path::new("/dev/zero"), 8).is_err());
    }

    #[test]
    fn absent_configuration_is_local_but_an_invalid_remote_target_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("execution.toml");
        assert_eq!(Target::load(&path).unwrap(), Target::Local);
        let server = "ab".repeat(32);
        std::fs::write(
            &path,
            format!("backend = 'native'\naddress = '-oProxyCommand=bad'\nserver = '{server}'"),
        )
        .unwrap();
        assert!(Target::load(&path).is_err());
        std::fs::write(
            &path,
            format!("backend = 'native'\naddress = 'work-box:47130'\nserver = '{server}'"),
        )
        .unwrap();
        assert!(matches!(
            Target::load(&path).unwrap(),
            Target::Native { address, .. } if address == "work-box:47130"
        ));
    }

    #[test]
    fn a_backend_adroit_does_not_know_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("execution.toml");
        std::fs::write(&path, "backend = 'telnet'\nhost = 'ai-box'\n").unwrap();
        let refused = Target::load(&path).unwrap_err();
        assert!(
            refused.contains("invalid execution configuration"),
            "{refused}"
        );
    }
}
