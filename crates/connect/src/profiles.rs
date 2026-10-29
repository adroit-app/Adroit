//! Named working environments and immutable bindings of client instances.

use crate::Target;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub target: Target,
    pub scope: Option<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub legacy_client: Option<String>,
    #[serde(default)]
    pub work_instance: Option<String>,
}

impl Profile {
    pub fn new(name: &str, target: Target) -> Self {
        let id = uuid::Uuid::new_v4().simple().to_string();
        Self {
            scope: Some(id.clone()),
            id,
            name: name.trim().into(),
            target,
            archived: false,
            legacy_client: None,
            work_instance: None,
        }
    }

    pub fn bind(&self, client: &str) -> Result<Binding, String> {
        self.validate()?;
        if !adroit_model::paths::is_instance(client) {
            return Err("Invalid client instance".into());
        }
        Ok(Binding {
            profile: self.id.clone(),
            name: self.name.clone(),
            target: self.target.clone(),
            scope: self.scope.clone(),
            client: client.into(),
            work_instance: (self.target == Target::Local).then(|| {
                self.work_instance
                    .clone()
                    .or_else(|| self.legacy_client.clone())
                    .unwrap_or_else(|| "default".into())
            }),
        })
    }

    fn validate(&self) -> Result<(), String> {
        if !adroit_model::paths::is_instance(&self.id)
            || self.name.trim().is_empty()
            || self.name.len() > 256
        {
            return Err("A profile needs a valid ID and a name of at most 256 bytes".into());
        }
        if self
            .scope
            .as_ref()
            .is_some_and(|s| !adroit_model::paths::is_instance(s))
            || self
                .legacy_client
                .as_ref()
                .is_some_and(|s| !adroit_model::paths::is_instance(s))
            || self
                .work_instance
                .as_ref()
                .is_some_and(|s| !adroit_model::paths::is_instance(s))
        {
            return Err("Invalid work environment".into());
        }
        self.target.validate()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub profile: String,
    pub name: String,
    pub target: Target,
    pub scope: Option<String>,
    pub client: String,
    #[serde(default)]
    pub work_instance: Option<String>,
}

impl Binding {
    pub fn load(state: &Path) -> Result<Option<Self>, String> {
        let path = state.join("profile.toml");
        let Some(text) = crate::read_config(&path, 65536).map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        let binding: Self = toml::from_str(&text).map_err(|e| e.to_string())?;
        if !adroit_model::paths::is_instance(&binding.profile)
            || !adroit_model::paths::is_instance(&binding.client)
            || binding
                .work_instance
                .as_ref()
                .is_some_and(|s| !adroit_model::paths::is_instance(s))
            || binding
                .scope
                .as_ref()
                .is_some_and(|s| !adroit_model::paths::is_instance(s))
        {
            return Err("Invalid profile binding".into());
        }
        binding.target.validate()?;
        Ok(Some(binding))
    }

    pub fn local_data_dir(&self, state: &Path, legacy: &Path) -> Result<PathBuf, String> {
        if self.target != Target::Local {
            return Err("Allocate agent logins on the work machine".into());
        }
        Ok(self.scope.as_ref().map_or_else(
            || legacy.to_path_buf(),
            |scope| {
                adroit_model::paths::instance_dir(
                    state,
                    self.work_instance.as_deref().unwrap_or("default"),
                )
                .join("environments")
                .join(scope)
                .join("data")
            },
        ))
    }

    pub fn save(&self, state: &Path) -> Result<(), String> {
        if let Some(existing) = Self::load(state)? {
            if existing.profile != self.profile
                || existing.target != self.target
                || existing.scope != self.scope
                || existing.client != self.client
                || existing.work_instance != self.work_instance
            {
                return Err("This instance is already bound to another profile connection".into());
            }
            return Ok(());
        }
        write(&state.join("profile.toml"), self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub version: u32,
    pub revision: u64,
    #[serde(default)]
    pub profiles: Vec<Profile>,
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            version: 1,
            revision: 0,
            profiles: Vec::new(),
        }
    }
}

impl Catalog {
    pub fn path() -> PathBuf {
        adroit_model::paths::config_dir().join("environments.toml")
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let Some(text) = crate::read_config(path, 1024 * 1024).map_err(|e| e.to_string())? else {
            return Ok(Self::default());
        };
        let catalog: Self =
            toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        catalog.validate()?;
        Ok(catalog)
    }

    fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported profile catalog version".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for profile in &self.profiles {
            profile.validate()?;
            if !ids.insert(&profile.id) {
                return Err("Duplicate environment profile ID".into());
            }
        }
        Ok(())
    }

    pub fn save(&mut self, path: &Path, expected: u64) -> Result<(), String> {
        self.validate()?;
        let parent = path.parent().ok_or("Profile catalog has no parent")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let lock = lock(&path.with_extension("lock"))?;
        if Self::load(path)?.revision != expected {
            return Err("Profiles changed in another window; reload before saving".into());
        }
        let mut next = self.clone();
        next.revision = expected
            .checked_add(1)
            .ok_or("Profile revision exhausted")?;
        write(path, &next)?;
        *self = next;
        drop(lock);
        Ok(())
    }

    pub fn import_legacy(&mut self, client: &str, state: &Path) -> Result<Binding, String> {
        if let Some(binding) = Binding::load(state)? {
            return Ok(binding);
        }
        if let Some(profile) = self
            .profiles
            .iter()
            .find(|p| p.legacy_client.as_deref() == Some(client))
        {
            return profile.bind(client);
        }
        let active = Target::load(&state.join("execution.toml"))?;
        let mut profile = Profile::new(
            if client == "default" {
                "Personal"
            } else {
                client
            },
            active,
        );
        profile.scope = None;
        profile.legacy_client = Some(client.into());
        let binding = profile.bind(client)?;
        self.profiles.push(profile);
        let pending = state.join("execution.next.toml");
        if pending.exists() {
            let mut profile =
                Profile::new("Saved connection (not opened)", Target::load(&pending)?);
            profile.scope = None;
            profile.work_instance = Some(client.into());
            self.profiles.push(profile);
        }
        Ok(binding)
    }
}

pub fn lock(path: &Path) -> Result<std::fs::File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.lock().map_err(|e| e.to_string())?;
    Ok(file)
}

pub fn write(path: &Path, value: &impl Serialize) -> Result<(), String> {
    use std::io::Write;
    let text = toml::to_string_pretty(value).map_err(|e| e.to_string())?;
    if text.len() > 1024 * 1024 {
        return Err("Profile configuration is too large".into());
    }
    let parent = path.parent().ok_or("Profile configuration has no parent")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(text.as_bytes())
        .and_then(|_| file.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Migrate once, retaining the legacy client's work and pending connection.
pub fn settle(client: &str, state: &Path) -> Result<Binding, String> {
    std::fs::create_dir_all(state).map_err(|e| e.to_string())?;
    let _claim = lock(&state.join("profile.lock"))?;
    if let Some(binding) = Binding::load(state)? {
        return Ok(binding);
    }
    let path = Catalog::path();
    for _ in 0..8 {
        let mut catalog = Catalog::load(&path)?;
        let revision = catalog.revision;
        let binding = catalog.import_legacy(client, state)?;
        match catalog.save(&path, revision) {
            Ok(()) => {
                binding.save(state)?;
                return Ok(binding);
            }
            Err(error) if error.starts_with("Profiles changed") => continue,
            Err(error) => return Err(error),
        }
    }
    Err("Profiles changed repeatedly; reopen the profile chooser".into())
}

pub fn active() -> Result<Option<Binding>, String> {
    Binding::load(&adroit_model::paths::instance_state())
}

/// Start a profile in a stable client instance, or in a new independent one.
pub fn open_command(
    program: &Path,
    profile: &Profile,
    independent: bool,
) -> Result<std::process::Command, String> {
    let client = if independent {
        uuid::Uuid::new_v4().simple().to_string()
    } else {
        profile
            .legacy_client
            .clone()
            .unwrap_or_else(|| profile.id.clone())
    };
    let binding = profile.bind(&client)?;
    let state = adroit_model::paths::instance_dir(&adroit_model::paths::state_dir(), &client);
    std::fs::create_dir_all(&state).map_err(|e| e.to_string())?;
    let _claim = lock(&state.join("profile.lock"))?;
    binding.save(&state)?;
    // Legacy clients and native commands read this too; an existing binding is never replaced.
    binding.target.save(&state.join("execution.toml"))?;
    let mut command = std::process::Command::new(program);
    command.args(["--instance", &client]);
    for key in [
        "ADROIT_WINDOWS",
        "ADROIT_HOST",
        "ADROIT_DAEMON",
        "ADROIT_REMOTE",
        "ADROIT_INSTANCE",
        "ADROIT_ENVIRONMENT",
        "ADROIT_WORK_INSTANCE",
    ] {
        command.env_remove(key);
    }
    Ok(command)
}
