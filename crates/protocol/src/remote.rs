//! The identity of the independently running hosts a client reaches.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteEndpoint {
    pub instance: String,
    pub generation: String,
    pub pid: u32,
    pub socket: PathBuf,
    pub build: String,
    pub protocols: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteInfo {
    pub instance: String,
    pub work: RemoteEndpoint,
    pub terminal: RemoteEndpoint,
}

pub fn is_generation(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl RemoteEndpoint {
    pub fn validate(&self) -> Result<(), String> {
        if !is_generation(&self.instance)
            || !is_generation(&self.generation)
            || self.pid == 0
            || self.protocols.is_empty()
            || self.build.is_empty()
            || !self.socket.is_absolute()
            || !self.socket.to_str().is_some_and(|p| {
                p.len() <= 100
                    && p.starts_with("/tmp/adroit-r-")
                    && p.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
            })
            || self
                .socket
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            || self.socket.file_name().and_then(|s| s.to_str())
                != Some(&format!("{}.sock", self.generation))
            || self
                .socket
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                != Some(self.instance.as_str())
        {
            return Err("invalid remote host identity or generation socket".into());
        }
        Ok(())
    }
}

impl RemoteInfo {
    pub fn validate(&self) -> Result<(), String> {
        self.work.validate()?;
        self.terminal.validate()?;
        if self.instance != self.work.instance
            || self.instance != self.terminal.instance
            || self.work.generation == self.terminal.generation
            || self.work.socket == self.terminal.socket
        {
            return Err(
                "remote hosts do not identify the same instance and distinct generations".into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn endpoint() -> RemoteEndpoint {
        RemoteEndpoint {
            instance: "a".repeat(32),
            generation: "b".repeat(32),
            pid: 1,
            socket: format!(
                "/tmp/adroit-r-501/{}/{}.sock",
                "a".repeat(32),
                "b".repeat(32)
            )
            .into(),
            build: "test".into(),
            protocols: vec![1],
        }
    }

    #[test]
    fn generation_endpoints_reject_reusable_escaping_or_unexpected_character_paths() {
        let valid = endpoint();
        assert!(valid.validate().is_ok());
        for path in [
            "/tmp/daemon.sock".into(),
            valid
                .socket
                .to_string_lossy()
                .replace("adroit-r-501", "adroit-r-501:x"),
            valid
                .socket
                .to_string_lossy()
                .replace("adroit-r-501", "adroit-r-501/../adroit-r-501"),
        ] {
            let mut changed = valid.clone();
            changed.socket = path.into();
            assert!(changed.validate().is_err());
        }
        let mut stale = valid;
        stale.generation = "c".repeat(32);
        assert!(stale.validate().is_err());
    }
}
