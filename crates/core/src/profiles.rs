//! How the Settings window changes the profiles: each change keeps the
//! settings holding together, and refuses, saying why, what would not.

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
            let fresh = std::iter::once(id.to_string())
                .chain((2..).map(|n| format!("{id}-{n}")))
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
    match past.home {
        None => started,
        Some(_) => format!("{started} · own login"),
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
    let agent = profile.agent().label();
    let named = match profile.label() {
        name if name == agent => agent.to_string(),
        name => format!("{agent} · {name}"),
    };
    match profile.home {
        None => named,
        Some(_) => format!("{named} · own login"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_model::testing::profile;

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
                        .home("/Users/me/work")
                        .build()
                },
                profile("codex", Agent::Codex).build(),
            ],
            ..Settings::default()
        }
    }

    fn id(text: &str) -> ProfileId {
        text.parse().unwrap()
    }

    /// Whether `s` would be read back as they are written, with every agent
    /// still started from a profile that is found as it is offered.
    fn holds(s: &Settings) {
        adroit_model::profile::check(&s.profile, &s.profiles).unwrap();
        for agent in Agent::ALL {
            let offered = s.profile_of(agent).into_owned();
            let found = s.find_profile(&offered.id).map(|p| p.into_owned());
            assert_eq!(found.as_ref(), Some(&offered), "{agent:?}");
            assert_eq!(offered.agent(), agent, "{offered:?}");
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
        assert_eq!(p.home, None);
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
    fn a_duplicate_keeps_its_profile_s_options_and_gets_a_login_of_its_own_when_the_profile_has_one()
     {
        let mut s = settings();
        profile_mut(&mut s, &id("claude-work"))
            .unwrap()
            .claude_mut()
            .unwrap()
            .model = "opus".into();
        let copy = duplicate_profile(&mut s, &id("claude-work"), Path::new("/d"), none).unwrap();
        assert_eq!(copy, id("claude-work-copy"));
        let made = &s.profiles[2];
        assert_eq!(made.id, copy);
        assert_eq!(made.name, "Claude Work copy");
        assert_eq!(
            made.home.as_deref(),
            Some(Path::new("/d/adroit/profiles/claude-work-copy"))
        );
        assert_eq!(made.claude().unwrap().model, "opus");
        holds(&s);
        // One in its agent's own environment is copied there.
        let copy = duplicate_profile(&mut s, &id("codex"), Path::new("/d"), none).unwrap();
        assert_eq!(s.find_profile(&copy).unwrap().home, None);
        holds(&s);
    }

    #[test]
    fn renaming_changes_the_name_alone() {
        let mut s = settings();
        rename_profile(&mut s, &id("claude-work"), " Day job ").unwrap();
        let p = &s.profiles[1];
        assert_eq!(
            (p.id.as_str(), p.name.as_str(), p.home.as_deref()),
            ("claude-work", "Day job", Some(Path::new("/Users/me/work")))
        );
        assert!(rename_profile(&mut s, &id("claude"), "").is_err());
        assert_eq!(
            rename_profile(&mut s, &id("gone"), "x").unwrap_err(),
            "profile `gone` is not defined"
        );
    }

    #[test]
    fn a_deleted_profile_is_given_back_with_its_login_and_the_default_one_cannot_be_deleted() {
        let mut s = settings();
        let deleted = delete_profile(&mut s, &id("claude-work")).unwrap();
        assert_eq!(deleted.home.as_deref(), Some(Path::new("/Users/me/work")));
        assert_eq!(s.find_profile(&id("claude-work")), None);
        holds(&s);
        let before = s.clone();
        assert_eq!(
            delete_profile(&mut s, &id("claude")).unwrap_err(),
            "Claude Personal is used for new sessions; use another profile for new sessions before deleting it"
        );
        assert_eq!(
            delete_profile(&mut s, &id("gone")).unwrap_err(),
            "profile `gone` is not defined"
        );
        assert_eq!(s, before);
        make_default(&mut s, &id("codex")).unwrap();
        delete_profile(&mut s, &id("claude")).unwrap();
        holds(&s);
    }

    #[test]
    fn deleting_offers_to_delete_only_a_folder_adroit_made() {
        let data = Path::new("/d");
        let with = |home: Option<&str>| {
            let mut p = profile("p", Agent::Claude).build();
            p.home = home.map(PathBuf::from);
            login_folder(&p, data, none)
        };
        assert_eq!(with(None), LoginFolder::Terminal);
        assert_eq!(
            with(Some("/d/adroit/profiles/p")),
            LoginFolder::Deletable("/d/adroit/profiles/p".into())
        );
        assert_eq!(
            with(Some("/Users/me/.claude")),
            LoginFolder::Kept("/Users/me/.claude".into())
        );
    }

    #[test]
    fn once_deleted_a_profile_s_folder_kept_is_shown_by_its_path() {
        let shown = |p: &Path| p.display().to_string();
        let told =
            |folder: &LoginFolder, with_folder| deleted_notice("Work", folder, with_folder, shown);
        let made = LoginFolder::Deletable("/d/adroit/profiles/work".into());
        assert_eq!(told(&LoginFolder::Terminal, false), "Deleted Work.");
        assert_eq!(
            told(&made, true),
            "Deleted Work and its login folder /d/adroit/profiles/work."
        );
        assert_eq!(
            told(&made, false),
            "Deleted Work. Its login folder is kept at /d/adroit/profiles/work."
        );
        assert_eq!(
            told(&LoginFolder::Kept("/w".into()), true),
            "Deleted Work. Its login folder is kept at /w."
        );
    }

    #[test]
    fn the_id_of_a_deleted_profile_can_be_taken_again() {
        let mut s = settings();
        delete_profile(&mut s, &id("claude-work")).unwrap();
        assert_eq!(
            add_profile(&mut s, "Claude Work", Agent::Claude).unwrap(),
            id("claude-work")
        );
        holds(&s);
    }

    #[test]
    fn only_a_folder_adroit_made_under_its_data_folder_is_deleted_with_its_profile() {
        let data = Path::new("/Users/me/.local/share");
        let made = |home: &str| made_by_adroit(Path::new(home), data, none);
        assert!(made("/Users/me/.local/share/adroit/profiles/work"));
        assert!(made("/Users/me/.local/share/adroit/connections/work"));
        assert!(!made("/Users/me/.local/share/adroit"));
        assert!(!made(
            "/Users/me/.local/share/adroit/profiles/../../../.claude"
        ));
        assert!(!made("/Users/me/.claude"));
        assert!(!made("/Users/me/.local/share/adroit-other/work"));
        // Through the links of the folders that exist, as homes are written.
        let canonical = |p: &Path| (p == Path::new("/tmp")).then(|| PathBuf::from("/private/tmp"));
        assert!(made_by_adroit(
            Path::new("/private/tmp/d/adroit/profiles/w"),
            Path::new("/tmp/d"),
            canonical
        ));
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
        assert_eq!(
            told,
            "Claude Work now starts Codex; its model, options, login and command were reset."
        );
        let p = &s.profiles[1];
        assert_eq!(p.options, AgentOptions::of(Agent::Codex));
        assert_eq!(p.home, None);
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
        // The only Codex profile, under Codex's key, deleted.
        let mut s = settings();
        delete_profile(&mut s, &id("codex")).unwrap();
        holds(&s);
        assert_eq!(s.profile_of(Agent::Codex).id, id("codex"));
        // The only Claude Code profile left, under its key, moved to Codex.
        let mut s = settings();
        delete_profile(&mut s, &id("claude-work")).unwrap();
        change_agent(&mut s, &id("claude"), Agent::Codex).unwrap();
        holds(&s);
        assert_eq!(s.profile_of(Agent::Claude).id, id("claude-2"));
    }

    #[test]
    fn a_login_of_its_own_keeps_its_state_under_adroit_s_data_folder_by_the_profile_s_id() {
        let mut s = settings();
        let data = Path::new("/Users/me/.local/share");
        set_login(&mut s, &id("codex"), Login::Own, data, none).unwrap();
        assert_eq!(
            s.profiles[2].home.as_deref(),
            Some(Path::new("/Users/me/.local/share/adroit/profiles/codex"))
        );
        holds(&s);
        // One it has already is kept as it is written.
        set_login(&mut s, &id("claude-work"), Login::Own, data, none).unwrap();
        assert_eq!(
            s.profiles[1].home.as_deref(),
            Some(Path::new("/Users/me/work"))
        );
        assert_eq!(
            set_login(&mut s, &id("gone"), Login::Own, data, none).unwrap_err(),
            "profile `gone` is not defined"
        );
    }

    #[test]
    fn a_login_of_its_own_is_written_once_through_the_links_of_the_folders_that_exist() {
        let mut s = settings();
        // `/tmp` is where `/private/tmp` is linked from.
        let canonical = |p: &Path| (p == Path::new("/tmp")).then(|| PathBuf::from("/private/tmp"));
        set_login(
            &mut s,
            &id("codex"),
            Login::Own,
            Path::new("/tmp/./d/"),
            canonical,
        )
        .unwrap();
        assert_eq!(
            s.profiles[2].home.as_deref(),
            Some(Path::new("/private/tmp/d/adroit/profiles/codex"))
        );
        assert_eq!(
            set_login(
                &mut s,
                &id("claude"),
                Login::Own,
                Path::new("relative"),
                none
            )
            .unwrap_err(),
            "relative/adroit/profiles/claude is not an absolute path"
        );
    }

    #[test]
    fn a_new_login_is_numbered_past_a_folder_that_exists_or_another_profile_has() {
        let mut s = settings();
        s.profiles[1].home = Some("/d/adroit/profiles/codex".into());
        // `codex-2` is there already, as a deleted profile left it.
        let left = |p: &Path| (p == Path::new("/d/adroit/profiles/codex-2")).then(|| p.into());
        set_login(&mut s, &id("codex"), Login::Own, Path::new("/d"), left).unwrap();
        assert_eq!(
            s.profiles[2].home.as_deref(),
            Some(Path::new("/d/adroit/profiles/codex-3"))
        );
        holds(&s);
        // So is a duplicate's.
        let copy = duplicate_profile(&mut s, &id("claude-work"), Path::new("/d"), |p: &Path| {
            (p == Path::new("/d/adroit/profiles/claude-work-copy")).then(|| p.into())
        })
        .unwrap();
        assert_eq!(
            s.find_profile(&copy).unwrap().home.as_deref(),
            Some(Path::new("/d/adroit/profiles/claude-work-copy-2"))
        );
    }

    #[test]
    fn back_in_its_terminal_s_login_a_profile_forgets_its_folder_and_takes_it_again_only_when_given_it()
     {
        let mut s = settings();
        let data = Path::new("/d");
        set_login(&mut s, &id("codex"), Login::Own, data, none).unwrap();
        let own = s.profiles[2].home.clone().unwrap();
        set_login(&mut s, &id("codex"), Login::Terminal, data, none).unwrap();
        assert_eq!(s.profiles[2].home, None);
        holds(&s);
        // Its folder is there now, so a new login is not it.
        let there = |p: &Path| (p == own).then(|| p.to_path_buf());
        set_login(&mut s, &id("codex"), Login::Own, data, there).unwrap();
        assert_eq!(
            s.profiles[2].home.as_deref(),
            Some(Path::new("/d/adroit/profiles/codex-2"))
        );
        set_login(&mut s, &id("codex"), Login::Again(own.clone()), data, none).unwrap();
        assert_eq!(s.profiles[2].home.as_ref(), Some(&own));
        // Unless another profile has it since.
        set_login(&mut s, &id("codex"), Login::Terminal, data, none).unwrap();
        s.profiles[1].home = Some(own.clone());
        assert_eq!(
            set_login(&mut s, &id("codex"), Login::Again(own), data, none).unwrap_err(),
            "Claude Work keeps its login in /d/adroit/profiles/codex since; each profile needs a folder of its own"
        );
    }

    #[test]
    fn a_draft_offers_a_profile_by_its_agent_and_name_and_says_when_it_has_a_login_of_its_own() {
        let mut s = settings();
        s.profiles.push(AgentProfile {
            name: "Codex".into(),
            ..profile("codex-2", Agent::Codex).build()
        });
        let offered: Vec<String> = startable(&s).iter().map(offered_as).collect();
        assert_eq!(
            offered,
            [
                "Claude Code · Claude Personal",
                "Claude Code · Claude Work · own login",
                "Codex · codex",
                "Codex",
                "Pi",
                "OpenCode"
            ]
        );
    }

    #[test]
    fn a_kept_conversation_is_listed_with_its_profile_and_a_login_of_its_own() {
        let s = settings();
        let past = |profile: &str, home: Option<&str>| adroit_model::Past {
            agent: Agent::Claude,
            home: home.map(PathBuf::from),
            profile: id(profile),
            profile_name: "Listed".into(),
            id: "c".into(),
            cwd: "/r".into(),
            title: "t".into(),
            at_ms: 0,
        };
        assert_eq!(
            resumed_as(&s, &past("claude-work", Some("/Users/me/work"))),
            "Claude Code · Claude Work · own login"
        );
        // Its profile deleted since it was listed.
        assert_eq!(
            resumed_as(&s, &past("gone", Some("/Users/me/gone"))),
            "Claude Code · Listed · own login"
        );
        assert_eq!(
            resumed_as(&s, &past("claude", None)),
            "Claude Code · Claude Personal"
        );
    }

    #[test]
    fn profiles_a_file_had_share_a_login_are_told_of_by_name_until_it_is_written_again() {
        let mut s = settings();
        assert_eq!(unshared_notice(&s), None);
        s.unshared = vec![id("codex")];
        assert_eq!(
            unshared_notice(&s).unwrap(),
            "codex shared a login with another profile, which keeps it; it now runs in your terminal's login. Give it a login of its own under Settings → Agents."
        );
        s.unshared = vec![id("codex"), id("claude")];
        assert_eq!(
            unshared_notice(&s).unwrap(),
            "codex, Claude Personal shared a login with another profile, which keeps it; they now run in your terminal's login. Give each a login of its own under Settings → Agents."
        );
    }

    #[test]
    fn a_session_starts_from_a_profile_or_adroit_s_own_for_an_agent_with_none() {
        let mut s = settings();
        delete_profile(&mut s, &id("claude-work")).unwrap();
        let ids = |s: &Settings| -> Vec<String> {
            startable(s).iter().map(|p| p.id.to_string()).collect()
        };
        assert_eq!(ids(&s), ["claude", "codex", "pi", "opencode"]);
        s.profiles.retain(|p| p.agent() == Agent::Claude);
        assert_eq!(ids(&s), ["claude", "codex", "pi", "opencode"]);
        assert_eq!(startable(&s)[1], AgentProfile::builtin(Agent::Codex));
    }
}
