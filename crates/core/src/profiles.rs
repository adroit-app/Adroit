//! How the Settings window changes the profiles and connections: each
//! change keeps the settings holding together, and refuses, saying why, what
//! would not.

use std::path::{Component, Path, PathBuf};

use adroit_model::Agent;
use adroit_model::profile::{AgentConnection, AgentOptions, AgentProfile, ConnectionId, ProfileId};

use crate::settings::Settings;

/// Where a new connection keeps its state: a folder of its own under
/// adroit's data folder, named by its id, or a folder chosen for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewHome {
    /// Under `data`, the user's data folder.
    Own { data: PathBuf },
    /// The folder written, `~` standing for `home`.
    Chosen { path: String, home: PathBuf },
}

/// What one profile is listed as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileRow {
    pub id: ProfileId,
    pub name: String,
    pub agent: Agent,
    /// The name of the connection it runs in.
    pub connection: String,
    /// Whether a new session starts from it when it names nothing.
    pub default: bool,
    pub archived: bool,
}

/// A connection a profile of an agent can run in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionChoice {
    pub id: ConnectionId,
    pub name: String,
    pub archived: bool,
}

/// An id made from `name`: its letters and digits in lower case, every run
/// of anything else one `-`, short enough for a number to follow, else
/// `fallback`; numbered from 2 while `taken` holds it.
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
    std::iter::once(slug.clone())
        .chain((2..).map(|n| format!("{slug}-{n}")))
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

/// A copy of the profile `id`, after it, in the same connection: its id.
pub fn duplicate_profile(settings: &mut Settings, id: &ProfileId) -> Result<ProfileId, String> {
    let at = position(settings, id)?;
    let from = settings.profiles[at].clone();
    let name = format!("{} copy", from.label());
    let copy = profile_id(settings, &name, from.agent());
    settings.profiles.insert(
        at + 1,
        AgentProfile {
            id: copy.clone(),
            name,
            archived: false,
            ..from
        },
    );
    Ok(copy)
}

/// The profile `id` called `name`; its id stays.
pub fn rename_profile(settings: &mut Settings, id: &ProfileId, name: &str) -> Result<(), String> {
    let name = named(name, "A profile")?;
    profile_mut(settings, id)?.name = name;
    Ok(())
}

/// The profile `id` kept for the sessions that ran from it alone; the
/// default profile is not, until another is the default.
pub fn archive_profile(settings: &mut Settings, id: &ProfileId) -> Result<(), String> {
    if &settings.profile == id {
        return Err(format!(
            "{} is used for new sessions; use another profile for new sessions before archiving it",
            settings.profiles[position(settings, id)?].label()
        ));
    }
    profile_mut(settings, id)?.archived = true;
    Ok(())
}

/// The profile `id` offered for new sessions again, while its connection is.
pub fn unarchive_profile(settings: &mut Settings, id: &ProfileId) -> Result<(), String> {
    let at = position(settings, id)?;
    let connection = settings.profiles[at].connection.clone();
    if let Some(c) = settings
        .connections
        .iter()
        .find(|c| c.id == connection && c.archived)
    {
        return Err(format!(
            "its account {} is archived; unarchive the account first",
            connection_label(c)
        ));
    }
    settings.profiles[at].archived = false;
    Ok(())
}

/// The profile `id` the one a new session starts from when it names nothing.
pub fn make_default(settings: &mut Settings, id: &ProfileId) -> Result<(), String> {
    let at = position(settings, id)?;
    if settings.profiles[at].archived {
        return Err(format!(
            "{} is archived; unarchive it before using it for new sessions",
            settings.profiles[at].label()
        ));
    }
    settings.profile = id.clone();
    Ok(())
}

/// The profile `id` starting `agent`: what it held for its agent before, its
/// options, its connection and its command, is reset to `agent`'s own. What
/// to tell the user of that, when the agent changed.
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
    profile.connection = ConnectionId::implicit(agent);
    profile.launch.command.clear();
    Ok(Some(format!(
        "{} now starts {}; its model, options, account and command were reset.",
        profile.label(),
        agent.label(),
    )))
}

/// The profile `id` run in `connection`, one of its agent's not archived.
pub fn set_connection(
    settings: &mut Settings,
    id: &ProfileId,
    connection: &ConnectionId,
) -> Result<(), String> {
    let agent = settings.profiles[position(settings, id)?].agent();
    match connection.implicit_agent() {
        Some(own) if own == agent => {}
        Some(own) => {
            return Err(format!(
                "a {} profile cannot use {}'s shared account",
                agent.label(),
                own.label()
            ));
        }
        None => {
            let c = settings
                .connections
                .iter()
                .find(|c| &c.id == connection)
                .ok_or_else(|| format!("account `{connection}` is not defined"))?;
            if c.agent != agent {
                return Err(format!(
                    "account {} is for {}, not {}",
                    connection_label(c),
                    c.agent.label(),
                    agent.label()
                ));
            }
            if c.archived {
                return Err(format!(
                    "account {} is archived; unarchive it first",
                    connection_label(c)
                ));
            }
        }
    }
    profile_mut(settings, id)?.connection = connection.clone();
    Ok(())
}

fn connection_label(c: &AgentConnection) -> &str {
    match c.name.trim() {
        "" => c.id.as_str(),
        name => name,
    }
}

/// What users are shown an agent's own environment as: the account the CLI
/// has outside adroit as well.
pub const SHARED: &str = "Shared with your terminal";

/// What the connection `id` is called: its name, its id when it has none,
/// or [`SHARED`] for an agent's own environment.
pub fn connection_name(settings: &Settings, id: &ConnectionId) -> String {
    if id.implicit_agent().is_some() {
        return SHARED.to_string();
    }
    settings
        .connections
        .iter()
        .find(|c| &c.id == id)
        .map_or_else(|| id.to_string(), |c| connection_label(c).to_string())
}

/// The profile a login was checked with, `answered`, as named beside the
/// answer on the page of the profile `page`: not at all when it is that one.
pub fn checked_with(
    settings: &Settings,
    answered: &ProfileId,
    page: Option<&ProfileId>,
) -> Option<String> {
    (page != Some(answered)).then(|| {
        settings
            .find_profile(answered)
            .map_or_else(|| answered.to_string(), |p| p.label().to_string())
    })
}

/// What a conversation kept is listed as: its agent and the profile it ran
/// from, and the connection that keeps it when that is one of its own.
pub fn resumed_as(settings: &Settings, past: &adroit_model::Past) -> String {
    let started = settings.started_as(past.agent, &past.profile);
    match past.connection.implicit_agent() {
        Some(_) => started,
        None => format!(
            "{started} · {}",
            connection_name(settings, &past.connection)
        ),
    }
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

/// A connection of `agent` called `name`, its home written once, as
/// `canonical` writes the folders that exist, and kept as written from then
/// on: its id.
pub fn add_connection(
    settings: &mut Settings,
    name: &str,
    agent: Agent,
    home: NewHome,
    canonical: impl Fn(&Path) -> Option<PathBuf>,
) -> Result<ConnectionId, String> {
    let name = named(name, "An account")?;
    let taken = |id: &str| {
        Agent::parse(id).is_some() || settings.connections.iter().any(|c| c.id.as_str() == id)
    };
    let id: ConnectionId = fresh_id(&name, &format!("{}-connection", agent.key()), taken)
        .parse()
        .expect("a fresh id is valid");
    let written = match home {
        NewHome::Own { data } => data.join("adroit").join("connections").join(id.as_str()),
        NewHome::Chosen { path, home } => match path.trim() {
            "" => return Err("An account needs a folder".into()),
            path => crate::settings::expand(path, &home),
        },
    };
    let home = plain(&written)
        .map(|p| canonical_of(&p, &canonical))
        .and_then(|p| plain(&p))
        .ok_or_else(|| format!("{} is not an absolute path", written.display()))?;
    if let Some(c) = settings.connections.iter().find(|c| c.home == home) {
        return Err(format!(
            "account {} already uses {}; each account needs a folder of its own",
            connection_label(c),
            home.display()
        ));
    }
    settings.connections.push(AgentConnection {
        id: id.clone(),
        name,
        agent,
        home,
        archived: false,
    });
    Ok(id)
}

/// A connection called `name` of the profile `id`'s agent, made as
/// [`add_connection`] makes one, which the profile then runs in: its id.
pub fn add_connection_for(
    settings: &mut Settings,
    id: &ProfileId,
    name: &str,
    home: NewHome,
    canonical: impl Fn(&Path) -> Option<PathBuf>,
) -> Result<ConnectionId, String> {
    let agent = settings.profiles[position(settings, id)?].agent();
    let made = add_connection(settings, name, agent, home, canonical)?;
    set_connection(settings, id, &made)?;
    Ok(made)
}

fn connection_mut<'a>(
    settings: &'a mut Settings,
    id: &ConnectionId,
) -> Result<&'a mut AgentConnection, String> {
    settings
        .connections
        .iter_mut()
        .find(|c| &c.id == id)
        .ok_or_else(|| format!("account `{id}` is not defined"))
}

/// The connection `id` called `name`; its id and its home stay.
pub fn rename_connection(
    settings: &mut Settings,
    id: &ConnectionId,
    name: &str,
) -> Result<(), String> {
    let name = named(name, "An account")?;
    connection_mut(settings, id)?.name = name;
    Ok(())
}

/// The profiles not archived that run in the connection `id`.
pub fn users<'a>(settings: &'a Settings, id: &ConnectionId) -> Vec<&'a AgentProfile> {
    settings
        .profiles
        .iter()
        .filter(|p| &p.connection == id && !p.archived)
        .collect()
}

/// The connection `id` kept for the profiles and sessions that used it,
/// once no profile not archived runs in it. Its folder is left as it is.
pub fn archive_connection(settings: &mut Settings, id: &ConnectionId) -> Result<(), String> {
    let using: Vec<&str> = users(settings, id).iter().map(|p| p.label()).collect();
    if !using.is_empty() {
        let name = connection_name(settings, id);
        return Err(format!(
            "{name} is still used by {}; move them to another account or archive them first",
            using.join(", ")
        ));
    }
    connection_mut(settings, id)?.archived = true;
    Ok(())
}

pub fn unarchive_connection(settings: &mut Settings, id: &ConnectionId) -> Result<(), String> {
    connection_mut(settings, id)?.archived = false;
    Ok(())
}

/// Every profile as the Settings window lists it, those not archived first,
/// each in the order the settings hold them.
pub fn profile_rows(settings: &Settings) -> Vec<ProfileRow> {
    let row = |p: &AgentProfile| ProfileRow {
        id: p.id.clone(),
        name: p.label().to_string(),
        agent: p.agent(),
        connection: connection_name(settings, &p.connection),
        default: p.id == settings.profile,
        archived: p.archived,
    };
    let (open, archived): (Vec<_>, Vec<_>) = settings.profiles.iter().partition(|p| !p.archived);
    open.into_iter().chain(archived).map(row).collect()
}

/// Every profile a new session can start from, in the order Settings lists
/// them: those not archived, and adroit's own for an agent that has none.
pub fn startable(settings: &Settings) -> Vec<AgentProfile> {
    let own = Agent::ALL
        .into_iter()
        .filter(|a| {
            !settings
                .profiles
                .iter()
                .any(|p| p.agent() == *a && !p.archived)
        })
        .map(|a| settings.profile_of(a).into_owned());
    settings
        .profiles
        .iter()
        .filter(|p| !p.archived)
        .cloned()
        .chain(own)
        .collect()
}

/// What a draft offers `profile` as, its agent being chosen beside it: its
/// name, and the connection it runs in when that is one of its own.
pub fn offered_as(settings: &Settings, profile: &AgentProfile) -> String {
    match profile.connection.implicit_agent() {
        Some(_) => profile.label().to_string(),
        None => format!(
            "{} · {}",
            profile.label(),
            connection_name(settings, &profile.connection)
        ),
    }
}

/// The connections a profile of `agent` running in `now` can be moved to:
/// the agent's own environment, then its connections not archived, and `now`
/// itself, archived or not.
pub fn connection_choices(
    settings: &Settings,
    agent: Agent,
    now: &ConnectionId,
) -> Vec<ConnectionChoice> {
    let own = ConnectionId::implicit(agent);
    std::iter::once(ConnectionChoice {
        name: connection_name(settings, &own),
        id: own,
        archived: false,
    })
    .chain(
        settings
            .connections
            .iter()
            .filter(|c| c.agent == agent && (!c.archived || &c.id == now))
            .map(|c| ConnectionChoice {
                id: c.id.clone(),
                name: connection_label(c).to_string(),
                archived: c.archived,
            }),
    )
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_model::testing::{connection, profile};

    fn settings() -> Settings {
        Settings {
            profile: "claude".parse().unwrap(),
            profiles: vec![
                AgentProfile {
                    name: "Claude Personal".into(),
                    ..profile("claude", Agent::Claude).build()
                },
                AgentProfile {
                    name: "Claude Work".into(),
                    ..profile("claude-work", Agent::Claude)
                        .connection("work")
                        .build()
                },
                profile("codex", Agent::Codex).build(),
            ],
            connections: vec![AgentConnection {
                name: "Work".into(),
                ..connection("work", Agent::Claude, "/Users/me/work").build()
            }],
            ..Settings::default()
        }
    }

    fn id(text: &str) -> ProfileId {
        text.parse().unwrap()
    }

    fn cid(text: &str) -> ConnectionId {
        text.parse().unwrap()
    }

    #[test]
    fn a_login_is_said_to_be_checked_with_a_profile_only_when_it_is_another() {
        let s = settings();
        let work = id("claude-work");
        assert_eq!(checked_with(&s, &work, Some(&work)), None);
        assert_eq!(
            checked_with(&s, &work, Some(&id("claude"))).as_deref(),
            Some("Claude Work")
        );
        assert_eq!(
            checked_with(&s, &work, None).as_deref(),
            Some("Claude Work")
        );
        assert_eq!(checked_with(&s, &id("gone"), None).as_deref(), Some("gone"));
    }

    /// Whether `s` would be read back as they are written, with every agent
    /// still started from a profile that is found as it is offered.
    fn holds(s: &Settings) {
        adroit_model::profile::check(&s.profile, &s.profiles, &s.connections).unwrap();
        for agent in Agent::ALL {
            let offered = s.profile_of(agent).into_owned();
            let found = s.find_profile(&offered.id).map(|p| p.into_owned());
            assert_eq!(found.as_ref(), Some(&offered), "{agent:?}");
            assert!(!offered.archived && offered.agent() == agent, "{offered:?}");
            assert!(startable(s).contains(&offered), "{agent:?}");
        }
        let offered = startable(s);
        for p in &offered {
            assert_eq!(
                s.find_profile(&p.id).map(|f| f.into_owned()).as_ref(),
                Some(p)
            );
        }
        let ids: std::collections::HashSet<_> = offered.iter().map(|p| &p.id).collect();
        assert_eq!(ids.len(), offered.len(), "{offered:?}");
    }

    fn none(_: &Path) -> Option<PathBuf> {
        None
    }

    #[test]
    fn an_id_is_made_from_the_name_in_the_characters_an_id_may_hold() {
        let free = |_: &str| false;
        assert_eq!(fresh_id("Claude Work", "x", free), "claude-work");
        assert_eq!(fresh_id("  Review / CI!! ", "x", free), "review-ci");
        assert_eq!(fresh_id("仕事", "claude", free), "claude");
        assert_eq!(fresh_id("2nd", "x", free), "2nd");
        assert_eq!(fresh_id(&"a".repeat(80), "x", free).len(), 60);
        let ids = [
            fresh_id("Work", "x", free),
            fresh_id("x".repeat(80).as_str(), "x", free) + "-99",
        ];
        for made in ids {
            assert!(made.parse::<ProfileId>().is_ok(), "{made}");
        }
    }

    #[test]
    fn an_id_already_taken_is_numbered_from_two() {
        let taken = |id: &str| ["work", "work-2"].contains(&id);
        assert_eq!(fresh_id("Work", "x", taken), "work-3");
    }

    #[test]
    fn a_new_profile_takes_an_id_from_its_name_and_starts_in_its_agent_s_own_environment() {
        let mut s = settings();
        let added = add_profile(&mut s, "Claude Work", Agent::Claude).unwrap();
        assert_eq!(added, id("claude-work-2"));
        let p = s.profiles.last().unwrap();
        assert_eq!(p.name, "Claude Work");
        assert_eq!(p.connection, ConnectionId::implicit(Agent::Claude));
        assert_eq!(p.options, AgentOptions::of(Agent::Claude));
        holds(&s);
        // Another agent's key stands for the profile adroit comes with for it.
        let named_codex = add_profile(&mut s, "Codex", Agent::Claude).unwrap();
        assert_eq!(named_codex, id("codex-2"));
        assert_eq!(
            add_profile(&mut s, " ", Agent::Codex).unwrap_err(),
            "A profile needs a name"
        );
    }

    #[test]
    fn a_duplicate_shares_its_profile_s_connection_and_options_under_an_id_of_its_own() {
        let mut s = settings();
        profile_mut(&mut s, &id("claude-work"))
            .unwrap()
            .claude_mut()
            .unwrap()
            .model = "opus".into();
        let copy = duplicate_profile(&mut s, &id("claude-work")).unwrap();
        assert_eq!(copy, id("claude-work-copy"));
        let made = &s.profiles[2];
        assert_eq!(made.id, copy);
        assert_eq!(made.name, "Claude Work copy");
        assert_eq!(made.connection, cid("work"));
        assert_eq!(made.claude().unwrap().model, "opus");
        holds(&s);
    }

    #[test]
    fn renaming_changes_the_name_alone() {
        let mut s = settings();
        rename_profile(&mut s, &id("claude-work"), " Day job ").unwrap();
        let p = &s.profiles[1];
        assert_eq!((p.id.as_str(), p.name.as_str()), ("claude-work", "Day job"));
        rename_connection(&mut s, &cid("work"), "Office").unwrap();
        let c = &s.connections[0];
        assert_eq!(
            (c.id.as_str(), c.name.as_str(), c.home.as_path()),
            ("work", "Office", Path::new("/Users/me/work"))
        );
        assert!(rename_profile(&mut s, &id("claude"), "").is_err());
        assert_eq!(
            rename_profile(&mut s, &id("gone"), "x").unwrap_err(),
            "profile `gone` is not defined"
        );
    }

    #[test]
    fn an_archived_profile_comes_back_and_the_default_one_cannot_be_archived() {
        let mut s = settings();
        archive_profile(&mut s, &id("claude-work")).unwrap();
        assert!(s.profiles[1].archived);
        holds(&s);
        assert_eq!(
            archive_profile(&mut s, &id("claude")).unwrap_err(),
            "Claude Personal is used for new sessions; use another profile for new sessions before archiving it"
        );
        assert_eq!(
            make_default(&mut s, &id("claude-work")).unwrap_err(),
            "Claude Work is archived; unarchive it before using it for new sessions"
        );
        unarchive_profile(&mut s, &id("claude-work")).unwrap();
        make_default(&mut s, &id("claude-work")).unwrap();
        assert_eq!(s.profile, id("claude-work"));
        holds(&s);
    }

    #[test]
    fn changing_a_profile_s_agent_resets_what_it_held_for_the_agent_before_and_says_so() {
        let mut s = settings();
        {
            let work = profile_mut(&mut s, &id("claude-work")).unwrap();
            work.claude_mut().unwrap().model = "opus".into();
            work.launch.command = vec!["/opt/claude".into()];
            work.launch.via_shell = true;
        }
        let told = change_agent(&mut s, &id("claude-work"), Agent::Codex)
            .unwrap()
            .unwrap();
        assert!(told.contains("Claude Work now starts Codex"), "{told}");
        assert_eq!(
            told,
            "Claude Work now starts Codex; its model, options, account and command were reset."
        );
        let p = &s.profiles[1];
        assert_eq!(p.options, AgentOptions::of(Agent::Codex));
        assert_eq!(p.connection, ConnectionId::implicit(Agent::Codex));
        assert!(p.launch.command.is_empty());
        assert!(p.launch.via_shell);
        assert_eq!(p.id, id("claude-work"));
        holds(&s);
        assert_eq!(
            change_agent(&mut s, &id("claude-work"), Agent::Codex),
            Ok(None)
        );
    }

    #[test]
    fn an_agent_left_without_a_profile_of_its_own_still_starts_from_adroit_s() {
        // The only Codex profile, under Codex's key, archived.
        let mut s = settings();
        archive_profile(&mut s, &id("codex")).unwrap();
        holds(&s);
        assert_eq!(s.profile_of(Agent::Codex).id, id("codex-2"));
        // The only Claude Code profile open, under its key, moved to Codex.
        let mut s = settings();
        archive_profile(&mut s, &id("claude-work")).unwrap();
        change_agent(&mut s, &id("claude"), Agent::Codex).unwrap();
        holds(&s);
        assert_eq!(s.profile_of(Agent::Claude).id, id("claude-2"));
    }

    #[test]
    fn a_profile_moves_only_to_a_connection_of_its_agent_not_archived() {
        let mut s = settings();
        set_connection(&mut s, &id("claude"), &cid("work")).unwrap();
        assert_eq!(s.profiles[0].connection, cid("work"));
        set_connection(
            &mut s,
            &id("claude"),
            &ConnectionId::implicit(Agent::Claude),
        )
        .unwrap();
        assert_eq!(
            set_connection(&mut s, &id("codex"), &cid("work")).unwrap_err(),
            "account Work is for Claude Code, not Codex"
        );
        assert!(
            set_connection(&mut s, &id("codex"), &ConnectionId::implicit(Agent::Claude)).is_err()
        );
        s.connections[0].archived = true;
        assert_eq!(
            set_connection(&mut s, &id("claude"), &cid("work")).unwrap_err(),
            "account Work is archived; unarchive it first"
        );
    }

    #[test]
    fn a_new_connection_keeps_its_state_under_adroit_s_data_folder_by_its_id() {
        let mut s = settings();
        let made = add_connection(
            &mut s,
            "Work",
            Agent::Claude,
            NewHome::Own {
                data: "/Users/me/.local/share".into(),
            },
            none,
        )
        .unwrap();
        assert_eq!(made, cid("work-2"));
        let c = s.connections.last().unwrap();
        assert_eq!(
            c.home,
            Path::new("/Users/me/.local/share/adroit/connections/work-2")
        );
        assert_eq!(c.agent, Agent::Claude);
        holds(&s);
        // An agent's key names its own environment.
        let named = add_connection(
            &mut s,
            "Codex",
            Agent::Codex,
            NewHome::Own { data: "/d".into() },
            none,
        )
        .unwrap();
        assert_eq!(named, cid("codex-2"));
    }

    #[test]
    fn a_chosen_home_is_written_once_absolute_and_plain_as_the_folders_that_exist_are() {
        let chosen = |path: &str| NewHome::Chosen {
            path: path.into(),
            home: "/Users/me".into(),
        };
        let mut s = settings();
        // `/tmp` is where `/private/tmp` is linked from.
        let canonical = |p: &Path| (p == Path::new("/tmp")).then(|| PathBuf::from("/private/tmp"));
        add_connection(
            &mut s,
            "A",
            Agent::Codex,
            chosen("/tmp/codex/./a//"),
            canonical,
        )
        .unwrap();
        assert_eq!(
            s.connections.last().unwrap().home,
            Path::new("/private/tmp/codex/a")
        );
        add_connection(&mut s, "B", Agent::Codex, chosen("~/agents/../b"), none).unwrap();
        assert_eq!(s.connections.last().unwrap().home, Path::new("/Users/me/b"));
        holds(&s);
        assert_eq!(
            add_connection(&mut s, "C", Agent::Codex, chosen("relative"), none).unwrap_err(),
            "relative is not an absolute path"
        );
        assert!(add_connection(&mut s, "C", Agent::Codex, chosen(" "), none).is_err());
        assert_eq!(
            add_connection(&mut s, "C", Agent::Claude, chosen("/Users/me/work/"), none)
                .unwrap_err(),
            "account Work already uses /Users/me/work; each account needs a folder of its own"
        );
    }

    #[test]
    fn a_connection_made_for_a_profile_is_of_its_agent_and_the_profile_runs_in_it() {
        let mut s = settings();
        let own = NewHome::Own { data: "/d".into() };
        let made = add_connection_for(&mut s, &id("codex"), "Codex CI", own.clone(), none).unwrap();
        assert_eq!(made, cid("codex-ci"));
        let c = s.connections.last().unwrap();
        assert_eq!(c.agent, Agent::Codex);
        assert_eq!(c.home, Path::new("/d/adroit/connections/codex-ci"));
        assert_eq!(s.profiles[2].connection, made);
        holds(&s);
        let before = s.clone();
        assert!(add_connection_for(&mut s, &id("codex"), " ", own.clone(), none).is_err());
        assert!(add_connection_for(&mut s, &id("gone"), "X", own, none).is_err());
        assert_eq!(s, before);
    }

    #[test]
    fn a_connection_profiles_still_run_in_is_not_archived_and_says_which() {
        let mut s = settings();
        duplicate_profile(&mut s, &id("claude-work")).unwrap();
        assert_eq!(
            archive_connection(&mut s, &cid("work")).unwrap_err(),
            "Work is still used by Claude Work, Claude Work copy; move them to another account or archive them first"
        );
        archive_profile(&mut s, &id("claude-work")).unwrap();
        archive_profile(&mut s, &id("claude-work-copy")).unwrap();
        archive_connection(&mut s, &cid("work")).unwrap();
        assert!(s.connections[0].archived);
        assert_eq!(s.connections[0].home, Path::new("/Users/me/work"));
        holds(&s);
        assert_eq!(
            unarchive_profile(&mut s, &id("claude-work")).unwrap_err(),
            "its account Work is archived; unarchive the account first"
        );
        unarchive_connection(&mut s, &cid("work")).unwrap();
        unarchive_profile(&mut s, &id("claude-work")).unwrap();
    }

    #[test]
    fn profiles_are_listed_with_their_connection_those_archived_last() {
        let mut s = settings();
        archive_profile(&mut s, &id("claude-work")).unwrap();
        let rows = profile_rows(&s);
        let listed: Vec<(&str, &str, bool, bool)> = rows
            .iter()
            .map(|r| {
                (
                    r.name.as_str(),
                    r.connection.as_str(),
                    r.default,
                    r.archived,
                )
            })
            .collect();
        assert_eq!(
            listed,
            [
                ("Claude Personal", "Shared with your terminal", true, false),
                ("codex", "Shared with your terminal", false, false),
                ("Claude Work", "Work", false, true),
            ]
        );
    }

    #[test]
    fn a_draft_offers_a_profile_by_its_name_and_a_connection_of_its_own() {
        let s = settings();
        let offered: Vec<String> = startable(&s).iter().map(|p| offered_as(&s, p)).collect();
        assert_eq!(
            offered,
            ["Claude Personal", "Claude Work · Work", "codex", "Pi"]
        );
    }

    #[test]
    fn a_kept_conversation_is_listed_with_its_profile_and_a_connection_of_its_own() {
        let s = settings();
        let past = |profile: &str, connection: &str| adroit_model::Past {
            agent: Agent::Claude,
            connection: cid(connection),
            profile: id(profile),
            id: "c".into(),
            cwd: "/r".into(),
            title: "t".into(),
            at_ms: 0,
        };
        assert_eq!(
            resumed_as(&s, &past("claude-work", "work")),
            "Claude Code · Claude Work · Work"
        );
        assert_eq!(
            resumed_as(&s, &past("claude", "claude")),
            "Claude Code · Claude Personal"
        );
    }

    #[test]
    fn a_session_starts_from_a_profile_not_archived_or_adroit_s_own_for_an_agent_with_none() {
        let mut s = settings();
        archive_profile(&mut s, &id("claude-work")).unwrap();
        let ids = |s: &Settings| -> Vec<String> {
            startable(s).iter().map(|p| p.id.to_string()).collect()
        };
        assert_eq!(ids(&s), ["claude", "codex", "pi"]);
        s.profiles.retain(|p| p.agent() == Agent::Claude);
        assert_eq!(ids(&s), ["claude", "codex", "pi"]);
        assert_eq!(startable(&s)[1], AgentProfile::builtin(Agent::Codex));
    }

    #[test]
    fn a_profile_is_offered_its_agent_s_own_environment_and_connections_not_archived() {
        let mut s = settings();
        let names = |s: &Settings, agent, now: &str| -> Vec<String> {
            connection_choices(s, agent, &cid(now))
                .into_iter()
                .map(|c| c.name)
                .collect()
        };
        assert_eq!(
            names(&s, Agent::Claude, "claude"),
            ["Shared with your terminal", "Work"]
        );
        assert_eq!(
            names(&s, Agent::Codex, "codex"),
            ["Shared with your terminal"]
        );
        s.connections[0].archived = true;
        assert_eq!(
            names(&s, Agent::Claude, "claude"),
            ["Shared with your terminal"]
        );
        assert_eq!(
            names(&s, Agent::Claude, "work"),
            ["Shared with your terminal", "Work"]
        );
    }
}
