//! How the Settings window changes the profiles: each change keeps the
//! settings holding together, and refuses, saying why, what would not.

use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};

use adroit_model::Agent;
use adroit_model::profile::{AgentOptions, AgentProfile, ProfileId};

use crate::settings::Settings;

/// Where a profile's agent keeps its login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Login {
    /// The agent's own environment, shared with the terminal.
    Terminal,
    /// A folder of the profile's own.
    Own,
    /// The folder of its own the profile had before it was given its
    /// terminal's, chosen again.
    Again(PathBuf),
}

/// What users are shown an agent's own environment as: the login the CLI
/// has outside adroit as well.
pub const TERMINAL: &str = "Your terminal's";

/// What users are shown a login of a profile's own as.
pub const OWN: &str = "This profile's own";

/// What the login kept in `home` is called: [`OWN`], or [`TERMINAL`] for
/// none.
pub fn login_name(home: Option<&Path>) -> &'static str {
    match home {
        Some(_) => OWN,
        None => TERMINAL,
    }
}

/// `base`, then `base` numbered from 2 on, for the first of them not taken.
pub(crate) fn numbered(base: &str) -> impl Iterator<Item = String> {
    std::iter::once(base.to_string()).chain((2..).map(move |n| format!("{base}-{n}")))
}

pub fn fresh_id(name: &str, fallback: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.truncate(60);
    let slug = match slug.trim_end_matches('-') {
        "" => fallback.to_string(),
        slug => slug.to_string(),
    };
    numbered(&slug)
        .find(|id| !taken(id))
        .expect("some numbered id is free")
}

/// The id a new profile of `agent` called `name` takes: no other profile's,
/// and no other agent's key, which stands for the profile adroit comes with
/// for that agent.
fn profile_id(settings: &Settings, name: &str, agent: Agent) -> ProfileId {
    let taken = |id: &str| {
        settings.profiles.iter().any(|p| p.id.as_str() == id)
            || Agent::parse(id).is_some_and(|other| other != agent)
    };
    fresh_id(name, agent.key(), taken)
        .parse()
        .expect("a fresh id is valid")
}

fn named(name: &str, what: &str) -> Result<String, String> {
    match name.trim() {
        "" => Err(format!("{what} needs a name")),
        name => Ok(name.to_string()),
    }
}

fn position(settings: &Settings, id: &ProfileId) -> Result<usize, String> {
    settings
        .profiles
        .iter()
        .position(|p| &p.id == id)
        .ok_or_else(|| format!("profile `{id}` is not defined"))
}

/// The profile `id`, to change.
pub fn profile_mut<'a>(
    settings: &'a mut Settings,
    id: &ProfileId,
) -> Result<&'a mut AgentProfile, String> {
    let at = position(settings, id)?;
    Ok(&mut settings.profiles[at])
}

/// A profile of `agent` called `name`, in the agent's own environment with
/// its options left to the agent: its id.
pub fn add_profile(settings: &mut Settings, name: &str, agent: Agent) -> Result<ProfileId, String> {
    let name = named(name, "A profile")?;
    let id = profile_id(settings, &name, agent);
    settings.profiles.push(AgentProfile {
        id: id.clone(),
        name,
        ..AgentProfile::builtin(agent)
    });
    Ok(id)
}

/// A copy of the profile `id`, after it, under an id of its own: in its
/// agent's own environment when that is where the profile runs, else with
/// a login of its own, made as [`set_login`] makes one, since no two
/// profiles share a folder: its id.
pub fn duplicate_profile(
    settings: &mut Settings,
    id: &ProfileId,
    data: &Path,
    canonical: impl Fn(&Path) -> Option<PathBuf>,
) -> Result<ProfileId, String> {
    let at = position(settings, id)?;
    let from = settings.profiles[at].clone();
    let name = format!("{} copy", from.label());
    let copy = profile_id(settings, &name, from.agent());
    let apart = from.home.is_some();
    settings.profiles.insert(
        at + 1,
        AgentProfile {
            id: copy.clone(),
            name,
            home: None,
            ..from
        },
    );
    if apart {
        set_login(settings, &copy, Login::Own, data, canonical)?;
    }
    Ok(copy)
}

/// The profile `id` called `name`; its id stays.
pub fn rename_profile(settings: &mut Settings, id: &ProfileId, name: &str) -> Result<(), String> {
    let name = named(name, "A profile")?;
    profile_mut(settings, id)?.name = name;
    Ok(())
}

/// The settings without the profile `id`, which is given back; the default
/// profile cannot be deleted until another is the default. Its folder is
/// left as it is: the sessions that ran from it go on as the settings they
/// started with say.
pub fn delete_profile(settings: &mut Settings, id: &ProfileId) -> Result<AgentProfile, String> {
    let at = position(settings, id)?;
    if &settings.profile == id {
        return Err(format!(
            "{} is used for new sessions; use another profile for new sessions before deleting it",
            settings.profiles[at].label()
        ));
    }
    Ok(settings.profiles.remove(at))
}

/// Whether `home` is a folder adroit made for a login, under adroit's in
/// `data`, the user's data folder, as `canonical` writes the folders that
/// exist: the only folders adroit deletes with their profile.
pub fn made_by_adroit(
    home: &Path,
    data: &Path,
    canonical: impl Fn(&Path) -> Option<PathBuf>,
) -> bool {
    let Some(ours) = plain(&data.join("adroit")).map(|p| canonical_of(&p, &canonical)) else {
        return false;
    };
    plain(home).is_some_and(|home| home != ours && home.starts_with(&ours))
}

/// What deleting a profile can do with the folder it keeps its login in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginFolder {
    /// It runs in its terminal's login, of which nothing is deleted.
    Terminal,
    /// A folder adroit made, which may be deleted with it.
    Deletable(PathBuf),
    /// A folder named by hand, which adroit leaves alone.
    Kept(PathBuf),
}

/// What deleting `profile` can do with its login's folder, `data` being
/// the user's data folder and `canonical` writing the folders that exist.
pub fn login_folder(
    profile: &AgentProfile,
    data: &Path,
    canonical: impl Fn(&Path) -> Option<PathBuf>,
) -> LoginFolder {
    match &profile.home {
        None => LoginFolder::Terminal,
        Some(home) if made_by_adroit(home, data, canonical) => LoginFolder::Deletable(home.clone()),
        Some(home) => LoginFolder::Kept(home.clone()),
    }
}

/// What is told once the profile called `name` is deleted, its login having
/// been `folder`, deleted with it when `with_folder`; `shown` writes a path
/// as the user reads it.
pub fn deleted_notice(
    name: &str,
    folder: &LoginFolder,
    with_folder: bool,
    shown: impl Fn(&Path) -> String,
) -> String {
    match (folder, with_folder) {
        (LoginFolder::Terminal, _) => format!("Deleted {name}."),
        (LoginFolder::Deletable(home), true) => {
            format!("Deleted {name} and its login folder {}.", shown(home))
        }
        (LoginFolder::Deletable(home) | LoginFolder::Kept(home), _) => format!(
            "Deleted {name}. Its login folder is kept at {}.",
            shown(home)
        ),
    }
}

/// The profile `id` the one a new session starts from when it names nothing.
pub fn make_default(settings: &mut Settings, id: &ProfileId) -> Result<(), String> {
    position(settings, id)?;
    settings.profile = id.clone();
    Ok(())
}

/// The profile `id` starting `agent`: what it held for its agent before, its
/// options, its login and its command, is reset to `agent`'s own. What to
/// tell the user of that, when the agent changed.
pub fn change_agent(
    settings: &mut Settings,
    id: &ProfileId,
    agent: Agent,
) -> Result<Option<String>, String> {
    let profile = profile_mut(settings, id)?;
    let from = profile.agent();
    if from == agent {
        return Ok(None);
    }
    profile.options = AgentOptions::of(agent);
    profile.home = None;
    profile.launch.command.clear();
    Ok(Some(format!(
        "{} now starts {}; its model, options, login and command were reset.",
        profile.label(),
        agent.label(),
    )))
}

/// The profile `id` running its agent where `login` says. A login of its
/// own keeps the folder it has, or is a new folder named by its id under
/// adroit's in `data`, the user's data folder, numbered past those that
/// exist, as a deleted profile's left, or another profile has: written
/// once, as `canonical` writes the folders that exist, which it finds no
/// other, and kept as written from then on. The folder it had chosen again
/// is refused when another profile has it since. Back in the agent's own
/// environment, the folder is left as it is.
pub fn set_login(
    settings: &mut Settings,
    id: &ProfileId,
    login: Login,
    data: &Path,
    canonical: impl Fn(&Path) -> Option<PathBuf>,
) -> Result<(), String> {
    let at = position(settings, id)?;
    let held = |home: &Path| {
        settings
            .profiles
            .iter()
            .enumerate()
            .find(|(i, p)| *i != at && p.home.as_deref() == Some(home))
            .map(|(_, p)| p)
    };
    let home = match (login, &settings.profiles[at].home) {
        (Login::Terminal, _) => None,
        (Login::Own, Some(home)) => Some(home.clone()),
        (Login::Again(home), _) => {
            if let Some(other) = held(&home) {
                return Err(format!(
                    "{} keeps its login in {} since; each profile needs a folder of its own",
                    other.label(),
                    home.display()
                ));
            }
            Some(home)
        }
        (Login::Own, None) => {
            let profiles = data.join("adroit").join("profiles");
            let fresh = numbered(id.as_str())
                .map(|name| {
                    let written = profiles.join(name);
                    plain(&written)
                        .map(|p| canonical_of(&p, &canonical))
                        .and_then(|p| plain(&p))
                        .ok_or_else(|| format!("{} is not an absolute path", written.display()))
                })
                .find(|home| match home {
                    Ok(h) => canonical(h).is_none() && held(h).is_none(),
                    Err(_) => true,
                })
                .expect("some numbered folder is free")?;
            Some(fresh)
        }
    };
    settings.profiles[at].home = home;
    Ok(())
}

/// What the main window and Settings tell of the profiles a file written
/// with connections had share a login, which run in their agent's own
/// environment since, until the file is written again.
pub fn unshared_notice(settings: &Settings) -> Option<String> {
    if settings.unshared.is_empty() {
        return None;
    }
    let named: Vec<&str> = settings
        .unshared
        .iter()
        .map(|id| {
            settings
                .profiles
                .iter()
                .find(|p| &p.id == id)
                .map_or(id.as_str(), AgentProfile::label)
        })
        .collect();
    let (runs, give) = match named.len() {
        1 => ("it now runs", "it"),
        _ => ("they now run", "each"),
    };
    Some(format!(
        "{} shared a login with another profile, which keeps it; {runs} in your terminal's login. Give {give} a login of its own under Settings → Agents.",
        named.join(", ")
    ))
}

/// What a conversation kept is listed as: its agent and the profile it ran
/// from, and that it is kept in a login of a profile's own when it is.
pub fn resumed_as(settings: &Settings, past: &adroit_model::Past) -> String {
    let started = settings.started_as(
        past.agent,
        &past.profile,
        past.home.as_deref(),
        &past.profile_name,
    );
    with_login(started, past.home.as_deref())
}

/// `path` written without `.`, `..` or a `/` doubled or at its end, when it
/// is absolute.
fn plain(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::from("/");
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(name) => out.push(name),
            Component::Prefix(_) => return None,
        }
    }
    (path.is_absolute() && out != Path::new("/")).then_some(out)
}

/// `path` with its deepest folder that exists written as `canonical` writes
/// it, the folders below, not made yet, after it.
fn canonical_of(path: &Path, canonical: &impl Fn(&Path) -> Option<PathBuf>) -> PathBuf {
    let mut rest = Vec::new();
    let mut at = path.to_path_buf();
    loop {
        if let Some(found) = canonical(&at) {
            return rest.iter().rev().fold(found, |dir, name| dir.join(name));
        }
        match (at.file_name().map(ToOwned::to_owned), at.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                at = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}

impl Settings {
    /// The agent of the profile a new session starts from.
    pub fn default_agent(&self) -> Agent {
        self.profiles
            .iter()
            .find(|p| p.id == self.profile)
            .map_or_else(Agent::default, AgentProfile::agent)
    }

    /// The profile a session of `agent` starts from when it names none: the
    /// default profile when that starts `agent`, else `agent`'s first
    /// profile, else the one adroit comes with for it.
    pub fn profile_of(&self, agent: Agent) -> Cow<'_, AgentProfile> {
        match self.position_of(agent) {
            Some(at) => Cow::Borrowed(&self.profiles[at]),
            None => Cow::Owned(self.own(agent)),
        }
    }

    /// The profile whose id is `id`: one of the profiles, or the one adroit
    /// comes with for an agent that has none, as [`Settings::profile_of`]
    /// starts it.
    pub fn find_profile(&self, id: &ProfileId) -> Option<Cow<'_, AgentProfile>> {
        if let Some(profile) = self.profiles.iter().find(|p| &p.id == id) {
            return Some(Cow::Borrowed(profile));
        }
        Agent::ALL
            .into_iter()
            .filter(|a| self.position_of(*a).is_none())
            .map(|a| self.own(a))
            .find(|p| &p.id == id)
            .map(Cow::Owned)
    }

    /// The profile [`Settings::profile_of`] names, added to the profiles when
    /// `agent` has none.
    pub fn profile_of_mut(&mut self, agent: Agent) -> &mut AgentProfile {
        let at = match self.position_of(agent) {
            Some(at) => at,
            None => {
                self.profiles.push(self.own(agent));
                self.profiles.len() - 1
            }
        };
        &mut self.profiles[at]
    }

    /// The profile adroit comes with for `agent`, under the agent's key, or
    /// numbered after it while a profile holds that: an id shared with a
    /// profile of another agent would find that one instead.
    fn own(&self, agent: Agent) -> AgentProfile {
        let taken = |id: &ProfileId| self.profiles.iter().any(|p| &p.id == id);
        let id = numbered(agent.key())
            .filter_map(|id| id.parse::<ProfileId>().ok())
            .find(|id| !taken(id))
            .expect("some numbered id is free");
        AgentProfile {
            id,
            ..AgentProfile::builtin(agent)
        }
    }

    /// What a session of `agent` started from the profile `profile` in the
    /// login `home` is shown as: its agent, and the profile's name when it
    /// is not the agent's own name, as for the profile adroit comes with. A
    /// profile these settings no longer hold, or hold under its id for
    /// another agent or login, as one deleted and its id taken again, is
    /// named `called`, what it was called as the session started, or by its
    /// id when that is not known.
    pub fn started_as(
        &self,
        agent: Agent,
        profile: &ProfileId,
        home: Option<&Path>,
        called: &str,
    ) -> String {
        let same = |p: &&AgentProfile| p.agent() == agent && p.home.as_deref() == home;
        let label = match self.profiles.iter().find(|p| &p.id == profile) {
            Some(p) if same(&p) => p.label().to_string(),
            None if home.is_none()
                && (*profile == ProfileId::builtin(agent) || self.own(agent).id == *profile) =>
            {
                agent.label().to_string()
            }
            _ if !called.trim().is_empty() => called.trim().to_string(),
            _ => profile.to_string(),
        };
        agent_and_name(agent, &label)
    }

    fn position_of(&self, agent: Agent) -> Option<usize> {
        let open = |p: &AgentProfile| p.agent() == agent;
        self.profiles
            .iter()
            .position(|p| p.id == self.profile && open(p))
            .or_else(|| self.profiles.iter().position(open))
    }
}

/// `agent` as users are shown it, then `name` when that is not the agent's
/// own.
fn agent_and_name(agent: Agent, name: &str) -> String {
    let agent = agent.label();
    if name == agent {
        agent.to_string()
    } else {
        format!("{agent} · {name}")
    }
}

/// `label`, and that it keeps a login of its own when `home` names one.
fn with_login(label: String, home: Option<&Path>) -> String {
    match home {
        None => label,
        Some(_) => format!("{label} · own login"),
    }
}

/// Every profile a new session can start from, in the order Settings lists
/// them: the profiles, and adroit's own for an agent that has none.
pub fn startable(settings: &Settings) -> Vec<AgentProfile> {
    let own = Agent::ALL
        .into_iter()
        .filter(|a| !settings.profiles.iter().any(|p| p.agent() == *a))
        .map(|a| settings.profile_of(a).into_owned());
    settings.profiles.iter().cloned().chain(own).collect()
}

/// What a draft offers `profile` as, the profile being all it chooses: its
/// agent, its name when that is not the agent's own, as a running session's
/// details name it, and that it has a login of its own when it has.
pub fn offered_as(profile: &AgentProfile) -> String {
    with_login(
        agent_and_name(profile.agent(), profile.label()),
        profile.home.as_deref(),
    )
}

#[cfg(test)]
mod tests;
